//! MadOS audio client.
//!
//! Talks to the user's sound server through the PulseAudio client protocol
//! (libpulse). On Fedora that server is PipeWire's `pipewire-pulse`; the
//! protocol is PipeWire's stable, documented compatibility API, used the
//! same way by pavucontrol and desktop volume applets. Everything runs as
//! the user against the user's own server — no privileges involved.
//!
//! The API is synchronous (each call connects, does its work and
//! disconnects); callers run it off their UI thread.

use pulse::callbacks::ListResult;
use pulse::context::{Context, FlagSet, State};
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::operation::{Operation, State as OpState};
use pulse::volume::{ChannelVolumes, Volume};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

extern crate libpulse_binding as pulse;

const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no sound server is running for this user")]
    NoServer,
    #[error("the sound server did not answer in time")]
    Timeout,
    #[error("there is no output device")]
    NoOutput,
    #[error("the sound server refused the change")]
    Refused,
    #[error("sound server error: {0}")]
    Other(String),
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Output {
    /// Server-side device name, e.g. `alsa_output.pci-0000_00_1b.0.analog-stereo`.
    pub name: String,
    /// Human-readable description, e.g. `Built-in Audio Analog Stereo`.
    pub description: String,
    /// Average channel volume, 0–100 (values above 100 are reported as is).
    pub volume_percent: u32,
    pub muted: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AudioStatus {
    pub schema: u32,
    /// Server name and version, e.g. `PulseAudio (on PipeWire 1.4.2)`.
    pub server: String,
    pub outputs: Vec<Output>,
}

impl AudioStatus {
    pub fn default_output(&self) -> Option<&Output> {
        self.outputs.iter().find(|o| o.is_default)
    }
}

pub fn volume_to_percent(v: Volume) -> u32 {
    ((u64::from(v.0) * 100 + u64::from(Volume::NORMAL.0) / 2) / u64::from(Volume::NORMAL.0)) as u32
}

pub fn percent_to_volume(percent: u32) -> Volume {
    // No amplification beyond 100% from MadOS controls.
    Volume((u64::from(percent.min(100)) * u64::from(Volume::NORMAL.0) / 100) as u32)
}

/// A connected client; disconnects on drop.
struct Client {
    mainloop: Mainloop,
    context: Context,
}

impl Client {
    fn connect() -> Result<Self, AudioError> {
        let mut mainloop = Mainloop::new().ok_or_else(|| AudioError::Other("cannot create main loop".into()))?;
        // Client name shown by sound-server tools (e.g. volume mixers).
        let client_name = mados_core::Product::load().product.name;
        let mut context =
            Context::new(&mainloop, &client_name).ok_or_else(|| AudioError::Other("cannot create context".into()))?;
        // NOAUTOSPAWN: never start a sound server as a side effect.
        context
            .connect(None, FlagSet::NOAUTOSPAWN, None)
            .map_err(|_| AudioError::NoServer)?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            iterate(&mut mainloop)?;
            match context.get_state() {
                State::Ready => break,
                State::Failed | State::Terminated => return Err(AudioError::NoServer),
                _ if Instant::now() > deadline => return Err(AudioError::Timeout),
                _ => {}
            }
        }
        Ok(Self { mainloop, context })
    }

    fn wait<T: ?Sized>(&mut self, mut op: Operation<T>) -> Result<(), AudioError> {
        let deadline = Instant::now() + TIMEOUT;
        while op.get_state() == OpState::Running {
            if Instant::now() > deadline {
                op.cancel();
                return Err(AudioError::Timeout);
            }
            iterate(&mut self.mainloop)?;
        }
        if op.get_state() == OpState::Cancelled {
            return Err(AudioError::Other("operation cancelled".into()));
        }
        Ok(())
    }

    fn status(&mut self) -> Result<AudioStatus, AudioError> {
        let server: Rc<RefCell<(String, Option<String>)>> = Rc::default();
        let s = server.clone();
        let op = self.context.introspect().get_server_info(move |info| {
            let name = info.server_name.as_deref().unwrap_or("unknown").to_string();
            let version = info.server_version.as_deref().unwrap_or("").to_string();
            *s.borrow_mut() = (
                format!("{name} {version}").trim().to_string(),
                info.default_sink_name.as_ref().map(|n| n.to_string()),
            );
        });
        self.wait(op)?;
        let (server_name, default_sink) = server.borrow().clone();

        let outputs: Rc<RefCell<Vec<Output>>> = Rc::default();
        let o = outputs.clone();
        let default = default_sink.clone();
        let op = self.context.introspect().get_sink_info_list(move |r| {
            if let ListResult::Item(sink) = r {
                let name = sink.name.as_deref().unwrap_or_default().to_string();
                o.borrow_mut().push(Output {
                    description: sink.description.as_deref().unwrap_or(&name).to_string(),
                    volume_percent: volume_to_percent(sink.volume.avg()),
                    muted: sink.mute,
                    is_default: default.as_deref() == Some(name.as_str()),
                    name,
                });
            }
        });
        self.wait(op)?;
        let mut outputs = outputs.borrow().clone();
        outputs.sort_by(|a, b| (!a.is_default, &a.description).cmp(&(!b.is_default, &b.description)));
        Ok(AudioStatus {
            schema: 1,
            server: server_name,
            outputs,
        })
    }

    fn sink_volumes(&mut self, name: &str) -> Result<ChannelVolumes, AudioError> {
        let cv: Rc<RefCell<Option<ChannelVolumes>>> = Rc::default();
        let c = cv.clone();
        let op = self.context.introspect().get_sink_info_by_name(name, move |r| {
            if let ListResult::Item(sink) = r {
                *c.borrow_mut() = Some(sink.volume);
            }
        });
        self.wait(op)?;
        let v = *cv.borrow();
        v.ok_or(AudioError::NoOutput)
    }

    fn run_success_op(
        &mut self,
        start: impl FnOnce(&Context, Box<dyn FnMut(bool) + 'static>) -> Operation<dyn FnMut(bool)>,
    ) -> Result<(), AudioError> {
        let ok = Rc::new(RefCell::new(false));
        let k = ok.clone();
        let op = start(&self.context, Box::new(move |success| *k.borrow_mut() = success));
        self.wait(op)?;
        let done = *ok.borrow();
        if done {
            Ok(())
        } else {
            Err(AudioError::Refused)
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.context.disconnect();
    }
}

fn iterate(mainloop: &mut Mainloop) -> Result<(), AudioError> {
    match mainloop.iterate(true) {
        IterateResult::Success(_) => Ok(()),
        IterateResult::Quit(_) => Err(AudioError::Other("main loop quit".into())),
        IterateResult::Err(e) => Err(AudioError::Other(
            e.to_string().unwrap_or_else(|| "main loop error".into()),
        )),
    }
}

/// Output devices with volume and mute state.
pub fn status() -> Result<AudioStatus, AudioError> {
    Client::connect()?.status()
}

fn default_output_name(client: &mut Client) -> Result<String, AudioError> {
    client
        .status()?
        .default_output()
        .map(|o| o.name.clone())
        .ok_or(AudioError::NoOutput)
}

/// Sets the default output's volume (all channels) to `percent` (capped at 100).
pub fn set_volume(percent: u32) -> Result<(), AudioError> {
    let mut client = Client::connect()?;
    let name = default_output_name(&mut client)?;
    let mut cv = client.sink_volumes(&name)?;
    let channels = cv.len();
    cv.set(channels, percent_to_volume(percent));
    client.run_success_op(|ctx, cb| ctx.introspect().set_sink_volume_by_name(&name, &cv, Some(cb)))
}

/// Mutes or unmutes the default output.
pub fn set_muted(muted: bool) -> Result<(), AudioError> {
    let mut client = Client::connect()?;
    let name = default_output_name(&mut client)?;
    client.run_success_op(|ctx, cb| ctx.introspect().set_sink_mute_by_name(&name, muted, Some(cb)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_conversion() {
        assert_eq!(volume_to_percent(Volume::NORMAL), 100);
        assert_eq!(volume_to_percent(Volume::MUTED), 0);
        assert_eq!(volume_to_percent(percent_to_volume(37)), 37);
        assert_eq!(percent_to_volume(250), Volume::NORMAL, "never amplify above 100%");
    }
}
