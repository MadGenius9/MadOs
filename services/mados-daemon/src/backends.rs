//! Production backends: polkit, logind and bootc.

use crate::{Authorizer, BoxFuture, PowerAction, PowerBackend, UpdateBackend};
use mados_api::{Deployment, UpdateStatus};
use mados_core::log_warn;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use zbus::zvariant::Value as ZValue;
use zbus::{proxy, Connection};

#[proxy(
    interface = "org.freedesktop.PolicyKit1.Authority",
    default_service = "org.freedesktop.PolicyKit1",
    default_path = "/org/freedesktop/PolicyKit1/Authority"
)]
trait PolkitAuthority {
    #[allow(clippy::type_complexity)]
    fn check_authorization(
        &self,
        subject: &(&str, HashMap<&str, ZValue<'_>>),
        action_id: &str,
        details: &HashMap<&str, &str>,
        flags: u32,
        cancellation_id: &str,
    ) -> zbus::Result<(bool, bool, HashMap<String, String>)>;
}

#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait LogindManager {
    fn power_off(&self, interactive: bool) -> zbus::Result<()>;
    fn reboot(&self, interactive: bool) -> zbus::Result<()>;
}

/// polkit `CheckAuthorizationFlags.AllowUserInteraction`.
const POLKIT_ALLOW_USER_INTERACTION: u32 = 1;

/// Authorizes callers with polkit, using the caller's unique bus name as the
/// subject (`system-bus-name`), so polkit evaluates the *caller's* identity
/// and session, never the daemon's.
pub struct PolkitAuthorizer {
    conn: Connection,
}

impl PolkitAuthorizer {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }
}

impl Authorizer for PolkitAuthorizer {
    fn check<'a>(&'a self, sender: &'a str, action: &'a str) -> BoxFuture<'a, Result<bool, String>> {
        Box::pin(async move {
            let proxy = PolkitAuthorityProxy::new(&self.conn).await.map_err(|e| e.to_string())?;
            let mut subject_details = HashMap::new();
            subject_details.insert("name", ZValue::from(sender));
            let (authorized, _challenge, _) = proxy
                .check_authorization(
                    &("system-bus-name", subject_details),
                    action,
                    &HashMap::new(),
                    POLKIT_ALLOW_USER_INTERACTION,
                    "",
                )
                .await
                .map_err(|e| e.to_string())?;
            Ok(authorized)
        })
    }
}

/// Performs power actions through systemd-logind.
pub struct LogindPower {
    conn: Connection,
}

impl LogindPower {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }
}

impl PowerBackend for LogindPower {
    fn execute(&self, action: PowerAction) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let proxy = LogindManagerProxy::new(&self.conn).await.map_err(|e| e.to_string())?;
            // interactive=false: authorization already happened in polkit
            // against the original caller.
            match action {
                PowerAction::PowerOff => proxy.power_off(false).await,
                PowerAction::Reboot => proxy.reboot(false).await,
            }
            .map_err(|e| e.to_string())
        })
    }
}

/// Reads deployment status from `bootc status`.
pub struct BootcUpdates {
    pub bootc: PathBuf,
    pub timeout: Duration,
}

impl Default for BootcUpdates {
    fn default() -> Self {
        Self {
            bootc: PathBuf::from("/usr/bin/bootc"),
            timeout: Duration::from_secs(30),
        }
    }
}

impl UpdateBackend for BootcUpdates {
    fn status(&self) -> BoxFuture<'_, UpdateStatus> {
        let bootc = self.bootc.clone();
        let timeout = self.timeout;
        Box::pin(blocking::unblock(move || run_bootc_status(&bootc, timeout)))
    }
}

fn unavailable(message: impl Into<String>) -> UpdateStatus {
    UpdateStatus {
        schema: 1,
        available: false,
        message: Some(message.into()),
        ..Default::default()
    }
}

fn run_bootc_status(bootc: &std::path::Path, timeout: Duration) -> UpdateStatus {
    if !bootc.exists() {
        return unavailable("bootc is not installed; this system is not image-based");
    }
    let child = Command::new(bootc)
        .args(["status", "--format=json", "--format-version=1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return unavailable(format!("cannot run bootc: {e}")),
    };
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                log_warn!("bootc status timed out after {timeout:?}");
                return unavailable("bootc status timed out");
            }
            Err(e) => return unavailable(format!("bootc wait failed: {e}")),
        }
    }
    let out = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => return unavailable(format!("bootc output failed: {e}")),
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        log_warn!("bootc status failed: {}", err.trim());
        return unavailable(format!("bootc status failed: {}", err.trim()));
    }
    parse_bootc_status(&String::from_utf8_lossy(&out.stdout))
}

/// Parses `bootc status --format=json --format-version=1` output leniently.
pub fn parse_bootc_status(json: &str) -> UpdateStatus {
    let v: Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(e) => return unavailable(format!("unparseable bootc status: {e}")),
    };
    let status = &v["status"];
    let dep = |key: &str| -> Option<Deployment> {
        let d = status.get(key)?;
        if d.is_null() {
            return None;
        }
        let img = &d["image"];
        let s = |val: &Value| val.as_str().map(str::to_string);
        Some(Deployment {
            image: s(&img["image"]["image"]),
            version: s(&img["version"]),
            timestamp: s(&img["timestamp"]),
            digest: s(&img["imageDigest"]),
        })
    };
    let booted = dep("booted");
    UpdateStatus {
        schema: 1,
        available: booted.is_some(),
        staged: dep("staged"),
        rollback: dep("rollback"),
        message: if booted.is_none() {
            Some("system is not booted from a bootc deployment".into())
        } else {
            None
        },
        booted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "apiVersion": "org.containers.bootc/v1", "kind": "BootcHost",
      "spec": {"image": {"image": "ghcr.io/madgenius9/mados:dev", "transport": "registry"}},
      "status": {
        "staged": null,
        "booted": {"image": {"image": {"image": "ghcr.io/madgenius9/mados:dev", "transport": "registry"},
                   "version": "0.1.0-dev", "timestamp": "2026-10-06T00:00:00Z", "imageDigest": "sha256:aaa"},
                   "pinned": false},
        "rollback": {"image": {"image": {"image": "ghcr.io/madgenius9/mados:dev", "transport": "registry"},
                   "version": "0.1.0-dev", "timestamp": "2026-10-01T00:00:00Z", "imageDigest": "sha256:bbb"}}
      }
    }"#;

    #[test]
    fn parses_bootc_status() {
        let s = parse_bootc_status(SAMPLE);
        assert!(s.available);
        let booted = s.booted.unwrap();
        assert_eq!(booted.image.as_deref(), Some("ghcr.io/madgenius9/mados:dev"));
        assert_eq!(booted.digest.as_deref(), Some("sha256:aaa"));
        assert!(s.staged.is_none());
        assert_eq!(s.rollback.unwrap().digest.as_deref(), Some("sha256:bbb"));
    }

    #[test]
    fn handles_garbage_and_non_bootc() {
        assert!(!parse_bootc_status("not json").available);
        let s = parse_bootc_status(r#"{"status":{"booted":null}}"#);
        assert!(!s.available);
        assert!(s.message.is_some());
    }

    #[test]
    fn missing_bootc_binary() {
        let s = run_bootc_status(std::path::Path::new("/nonexistent/bootc"), Duration::from_secs(1));
        assert!(!s.available);
    }
}
