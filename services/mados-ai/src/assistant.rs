//! The assistant pipeline: text → provider → intent → policy → (confirm) → ops.

use crate::intent::Intent;
use crate::ops::{Power, SystemOps};
use crate::policy::{self, Decision};
use crate::provider::Provider;
use mados_api::{AssistantReply, ReplyStatus};
use mados_core::{log_info, log_notice};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Pending confirmations expire after this long.
pub const CONFIRM_TTL: Duration = Duration::from_secs(120);

struct Pending {
    intent: Intent,
    owner: String,
    created: Instant,
}

pub struct Assistant<O: SystemOps> {
    provider: Box<dyn Provider>,
    ops: O,
    pending: Mutex<HashMap<String, Pending>>,
    next_id: AtomicU64,
}

fn reply(
    status: ReplyStatus,
    capability: Option<&str>,
    request_id: Option<String>,
    message: impl Into<String>,
) -> AssistantReply {
    AssistantReply {
        schema: 1,
        status,
        capability: capability.map(str::to_string),
        request_id,
        message: message.into(),
    }
}

/// Question shown to the user before a state-changing action.
pub fn confirmation_prompt(intent: &Intent) -> String {
    match intent {
        Intent::PowerOff => "Shut down the computer now? Unsaved work may be lost.".into(),
        Intent::Reboot => "Restart the computer now? Unsaved work may be lost.".into(),
        Intent::SetWifi { enabled } => format!("Turn Wi-Fi {}?", if *enabled { "on" } else { "off" }),
        Intent::SetBluetooth { enabled } => format!("Turn Bluetooth {}?", if *enabled { "on" } else { "off" }),
        Intent::SetBrightness { percent } => format!("Set display brightness to {percent}%?"),
        Intent::SetVolume { percent } => format!("Set the volume to {percent}%?"),
        Intent::SetMuted { muted: true } => "Mute the sound?".into(),
        Intent::SetMuted { muted: false } => "Unmute the sound?".into(),
        Intent::InstallUpdate => {
            "Download and install the system update? It takes effect after a restart; the current version is kept for rollback.".into()
        }
        other => format!("Perform {}?", policy::capability(other).id),
    }
}

fn unsupported_message(intent: &Intent) -> String {
    let what = match intent {
        Intent::ConnectDevice { .. } => "Connecting Bluetooth devices",
        Intent::FindFile { .. } => "Searching files",
        Intent::OpenApp { .. } => "Opening apps and projects",
        Intent::InstallApp { .. } => "Installing software",
        _ => "This action",
    };
    format!("{what} is not available in this development build yet.")
}

impl<O: SystemOps> Assistant<O> {
    pub fn new(provider: Box<dyn Provider>, ops: O) -> Self {
        Self {
            provider,
            ops,
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    pub fn provider_is_local(&self) -> bool {
        self.provider.is_local()
    }

    /// Handles a request from `owner` (the caller's unique bus name).
    pub async fn ask(&self, owner: &str, text: &str) -> AssistantReply {
        if text.len() > 2000 {
            return reply(ReplyStatus::NotUnderstood, None, None, "That request is too long.");
        }
        let Some(intent) = self.provider.interpret(text) else {
            return reply(
                ReplyStatus::NotUnderstood,
                None,
                None,
                "Sorry, I didn't understand. Try \"How much battery is left?\" or \"Turn Bluetooth on\".",
            );
        };
        if let Err(e) = intent.validate() {
            return reply(ReplyStatus::NotUnderstood, None, None, format!("Invalid request: {e}"));
        }
        let cap = policy::capability(&intent);
        match policy::decide(&intent) {
            Decision::Deny => {
                log_notice!("denied capability {} (forbidden)", cap.id);
                reply(
                    ReplyStatus::Denied,
                    Some(cap.id),
                    None,
                    "I can't run commands. The assistant can only use specific, permission-checked system actions.",
                )
            }
            Decision::Unsupported => reply(
                ReplyStatus::Unsupported,
                Some(cap.id),
                None,
                unsupported_message(&intent),
            ),
            Decision::Execute => self.execute(&intent).await,
            Decision::Confirm => {
                let id = self.next_id.fetch_add(1, Ordering::Relaxed).to_string();
                let prompt = confirmation_prompt(&intent);
                let mut pending = self.pending.lock().unwrap();
                pending.retain(|_, p| p.created.elapsed() < CONFIRM_TTL);
                pending.insert(
                    id.clone(),
                    Pending {
                        intent,
                        owner: owner.to_string(),
                        created: Instant::now(),
                    },
                );
                reply(ReplyStatus::NeedsConfirmation, Some(cap.id), Some(id), prompt)
            }
        }
    }

    /// Confirms a pending request. Only the caller that created it may confirm.
    pub async fn confirm(&self, owner: &str, request_id: &str) -> AssistantReply {
        let intent = {
            let mut pending = self.pending.lock().unwrap();
            match pending.get(request_id) {
                Some(p) if p.owner != owner => None,
                Some(p) if p.created.elapsed() >= CONFIRM_TTL => {
                    pending.remove(request_id);
                    None
                }
                Some(_) => pending.remove(request_id).map(|p| p.intent),
                None => None,
            }
        };
        match intent {
            Some(intent) => {
                log_info!("user confirmed {}", policy::capability(&intent).id);
                self.execute(&intent).await
            }
            None => reply(
                ReplyStatus::Error,
                None,
                None,
                "That request has expired or does not exist.",
            ),
        }
    }

    pub fn cancel(&self, owner: &str, request_id: &str) -> bool {
        let mut pending = self.pending.lock().unwrap();
        if pending.get(request_id).is_some_and(|p| p.owner == owner) {
            pending.remove(request_id);
            true
        } else {
            false
        }
    }

    async fn execute(&self, intent: &Intent) -> AssistantReply {
        let cap = policy::capability(intent);
        let result = match intent {
            Intent::SystemInfo => self.ops.system_info().await,
            Intent::BatteryStatus => self.ops.battery().await,
            Intent::DiagnosePerformance => self.ops.diagnose().await,
            Intent::PowerOff => self.ops.power(Power::Off).await,
            Intent::Reboot => self.ops.power(Power::Reboot).await,
            Intent::SetWifi { enabled } => self.ops.set_wifi(*enabled).await,
            Intent::SetBluetooth { enabled } => self.ops.set_bluetooth(*enabled).await,
            Intent::SetBrightness { percent } => self.ops.set_brightness(*percent).await,
            Intent::SetVolume { percent } => self.ops.set_volume(*percent).await,
            Intent::SetMuted { muted } => self.ops.set_muted(*muted).await,
            Intent::CheckUpdates => self.ops.check_updates().await,
            Intent::InstallUpdate => self.ops.install_update().await,
            // Policy never routes these here; fail closed if it ever does.
            _ => Err("not executable".into()),
        };
        match result {
            Ok(msg) => reply(ReplyStatus::Done, Some(cap.id), None, msg),
            Err(msg) => reply(ReplyStatus::Error, Some(cap.id), None, msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::{BoxFuture, OpResult};
    use crate::provider::RulesProvider;
    use std::sync::Arc;

    #[derive(Default, Clone)]
    struct FakeOps(Arc<Mutex<Vec<String>>>);

    impl FakeOps {
        fn log(&self, s: &str) -> BoxFuture<'_, OpResult> {
            self.0.lock().unwrap().push(s.to_string());
            let s = s.to_string();
            Box::pin(async move { Ok(format!("did {s}")) })
        }
    }

    impl SystemOps for FakeOps {
        fn system_info(&self) -> BoxFuture<'_, OpResult> {
            self.log("info")
        }
        fn battery(&self) -> BoxFuture<'_, OpResult> {
            self.log("battery")
        }
        fn diagnose(&self) -> BoxFuture<'_, OpResult> {
            self.log("diagnose")
        }
        fn power(&self, a: Power) -> BoxFuture<'_, OpResult> {
            self.log(if a == Power::Off { "off" } else { "reboot" })
        }
        fn set_wifi(&self, _: bool) -> BoxFuture<'_, OpResult> {
            self.log("wifi")
        }
        fn set_bluetooth(&self, e: bool) -> BoxFuture<'_, OpResult> {
            self.log(if e { "bt-on" } else { "bt-off" })
        }
        fn set_brightness(&self, _: u8) -> BoxFuture<'_, OpResult> {
            self.log("brightness")
        }
        fn check_updates(&self) -> BoxFuture<'_, OpResult> {
            self.log("check-updates")
        }
        fn set_volume(&self, _: u8) -> BoxFuture<'_, OpResult> {
            self.log("volume")
        }
        fn set_muted(&self, _: bool) -> BoxFuture<'_, OpResult> {
            self.log("mute")
        }
        fn install_update(&self) -> BoxFuture<'_, OpResult> {
            self.log("install-update")
        }
    }

    fn assistant() -> (Assistant<FakeOps>, FakeOps) {
        let ops = FakeOps::default();
        (Assistant::new(Box::new(RulesProvider), ops.clone()), ops)
    }

    fn calls(ops: &FakeOps) -> Vec<String> {
        ops.0.lock().unwrap().clone()
    }

    #[test]
    fn read_only_executes_immediately() {
        let (a, ops) = assistant();
        let r = zbus::block_on(a.ask(":1.1", "how much battery is left?"));
        assert_eq!(r.status, ReplyStatus::Done);
        assert_eq!(calls(&ops), vec!["battery"]);
    }

    #[test]
    fn state_change_requires_confirmation_from_same_caller() {
        let (a, ops) = assistant();
        let r = zbus::block_on(a.ask(":1.1", "Turn Bluetooth on"));
        assert_eq!(r.status, ReplyStatus::NeedsConfirmation);
        assert!(calls(&ops).is_empty(), "nothing may run before confirmation");
        let id = r.request_id.unwrap();

        let stolen = zbus::block_on(a.confirm(":1.99", &id));
        assert_eq!(stolen.status, ReplyStatus::Error);
        assert!(calls(&ops).is_empty(), "another caller cannot confirm");

        let ok = zbus::block_on(a.confirm(":1.1", &id));
        assert_eq!(ok.status, ReplyStatus::Done);
        assert_eq!(calls(&ops), vec!["bt-on"]);

        let replay = zbus::block_on(a.confirm(":1.1", &id));
        assert_eq!(replay.status, ReplyStatus::Error, "confirmation is single-use");
        assert_eq!(calls(&ops).len(), 1);
    }

    #[test]
    fn os_update_needs_confirmation_check_does_not() {
        let (a, ops) = assistant();
        assert_eq!(
            zbus::block_on(a.ask(":1.1", "check for updates")).status,
            ReplyStatus::Done
        );
        let r = zbus::block_on(a.ask(":1.1", "install updates"));
        assert_eq!(r.status, ReplyStatus::NeedsConfirmation);
        assert_eq!(calls(&ops), vec!["check-updates"]);
        zbus::block_on(a.confirm(":1.1", &r.request_id.unwrap()));
        assert_eq!(calls(&ops), vec!["check-updates", "install-update"]);
    }

    #[test]
    fn cancel_discards() {
        let (a, ops) = assistant();
        let r = zbus::block_on(a.ask(":1.1", "reboot"));
        let id = r.request_id.unwrap();
        assert!(!a.cancel(":1.2", &id));
        assert!(a.cancel(":1.1", &id));
        assert_eq!(zbus::block_on(a.confirm(":1.1", &id)).status, ReplyStatus::Error);
        assert!(calls(&ops).is_empty());
    }

    #[test]
    fn commands_denied_and_unsupported_reported() {
        let (a, ops) = assistant();
        assert_eq!(
            zbus::block_on(a.ask(":1.1", "sudo rm -rf /")).status,
            ReplyStatus::Denied
        );
        assert_eq!(
            zbus::block_on(a.ask(":1.1", "Install Discord")).status,
            ReplyStatus::Unsupported
        );
        assert_eq!(
            zbus::block_on(a.ask(":1.1", "sing a song")).status,
            ReplyStatus::NotUnderstood
        );
        assert!(calls(&ops).is_empty());
    }

    #[test]
    fn provider_json_is_strict() {
        assert_eq!(Intent::from_provider_json(r#"{"intent":"reboot"}"#), Ok(Intent::Reboot));
        assert!(Intent::from_provider_json(r#"{"intent":"shell","cmd":"ls"}"#).is_err());
        assert!(Intent::from_provider_json(r#"{"intent":"reboot","now":true}"#).is_err());
        assert!(Intent::from_provider_json(r#"{"intent":"set_brightness","percent":101}"#).is_err());
        assert!(Intent::from_provider_json(r#"{"intent":"install_app","name":"a\u0000b"}"#).is_err());
    }
}
