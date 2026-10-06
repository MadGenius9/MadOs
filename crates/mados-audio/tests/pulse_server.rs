//! mados-audio against a real PulseAudio server (null sinks) started for the
//! test. Skips when `pulseaudio` is not installed. One test function, because
//! PULSE_SERVER is process-global.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Server {
    child: Child,
    /// Keeps the socket directory alive for the server's lifetime.
    _dir: tempfile::TempDir,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_server() -> Option<Server> {
    let dir = tempfile::tempdir().ok()?;
    let sock = dir.path().join("native");
    let child = Command::new("pulseaudio")
        .args([
            "--daemonize=no",
            "-n",
            "--exit-idle-time=-1",
            "--disallow-exit",
            "--use-pid-file=no",
            "--system=no",
            &format!(
                "--load=module-native-protocol-unix socket={} auth-anonymous=1",
                sock.display()
            ),
            "--load=module-null-sink sink_name=speakers sink_properties=device.description=Speakers",
            "--load=module-null-sink sink_name=headphones sink_properties=device.description=Headphones",
            "--load=module-default-device-restore",
        ])
        .env("HOME", dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !sock.exists() {
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    std::env::set_var("PULSE_SERVER", format!("unix:{}", sock.display()));
    Some(Server { child, _dir: dir })
}

fn pactl(args: &[&str]) -> String {
    let out = Command::new("pactl").args(args).output().expect("pactl");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn status_volume_and_mute_against_real_server() {
    if Command::new("pulseaudio").arg("--version").output().is_err() {
        eprintln!("SKIP: pulseaudio not installed");
        return;
    }
    // No server -> a clear error, never a made-up status.
    std::env::set_var("PULSE_SERVER", "unix:/nonexistent/mados-test/native");
    assert!(matches!(mados_audio::status(), Err(mados_audio::AudioError::NoServer)));

    let Some(_server) = start_server() else {
        eprintln!("SKIP: could not start pulseaudio");
        return;
    };
    let st = mados_audio::status().unwrap();
    assert!(st.server.to_lowercase().contains("pulseaudio"), "{}", st.server);
    let names: Vec<&str> = st.outputs.iter().map(|o| o.description.as_str()).collect();
    assert_eq!(st.outputs.len(), 2, "{names:?}");
    let default = st.default_output().expect("a default output").clone();
    assert_eq!(st.outputs[0], default, "default output listed first");

    mados_audio::set_volume(35).unwrap();
    let st = mados_audio::status().unwrap();
    assert_eq!(st.default_output().unwrap().volume_percent, 35);
    // Cross-check with pactl: an independent client sees the same volume.
    assert!(pactl(&["get-sink-volume", &default.name]).contains("35%"));
    // The other output is untouched.
    let other = st.outputs.iter().find(|o| !o.is_default).unwrap();
    assert_eq!(other.volume_percent, 100);

    mados_audio::set_volume(400).unwrap();
    assert_eq!(
        mados_audio::status().unwrap().default_output().unwrap().volume_percent,
        100,
        "capped at 100%"
    );

    mados_audio::set_muted(true).unwrap();
    assert!(mados_audio::status().unwrap().default_output().unwrap().muted);
    assert!(pactl(&["get-sink-mute", &default.name]).contains("yes"));
    mados_audio::set_muted(false).unwrap();
    assert!(!mados_audio::status().unwrap().default_output().unwrap().muted);
}
