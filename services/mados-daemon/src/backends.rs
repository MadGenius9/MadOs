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

/// Upper bound for downloading and staging an update.
const STAGE_TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// Upper bound for fetching update metadata or switching deployments.
const CHECK_TIMEOUT: Duration = Duration::from_secs(5 * 60);

impl UpdateBackend for BootcUpdates {
    fn status(&self) -> BoxFuture<'_, UpdateStatus> {
        let bootc = self.bootc.clone();
        let timeout = self.timeout;
        Box::pin(blocking::unblock(move || run_bootc_status(&bootc, timeout)))
    }

    fn check(&self) -> BoxFuture<'_, Result<UpdateStatus, String>> {
        let bootc = self.bootc.clone();
        let timeout = self.timeout;
        Box::pin(blocking::unblock(move || {
            // Fetches metadata only; bootc records the result as cachedUpdate.
            run_bootc(&bootc, &["upgrade", "--check"], CHECK_TIMEOUT)?;
            Ok(run_bootc_status(&bootc, timeout))
        }))
    }

    fn stage(&self) -> BoxFuture<'_, Result<String, String>> {
        let bootc = self.bootc.clone();
        Box::pin(blocking::unblock(move || {
            // No --apply: the new deployment takes effect at the next boot,
            // never as an automatic reboot.
            run_bootc(&bootc, &["upgrade"], STAGE_TIMEOUT)?;
            Ok("Update installed. Restart to use it; the current version stays available for rollback.".into())
        }))
    }

    fn rollback(&self) -> BoxFuture<'_, Result<String, String>> {
        let bootc = self.bootc.clone();
        Box::pin(blocking::unblock(move || {
            run_bootc(&bootc, &["rollback"], CHECK_TIMEOUT)?;
            Ok("Rollback prepared. Restart to boot the previous version.".into())
        }))
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

/// Runs bootc with fixed arguments and a deadline; returns stdout.
fn run_bootc(bootc: &std::path::Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    if !bootc.exists() {
        return Err("bootc is not installed; this system is not image-based".into());
    }
    let mut child = Command::new(bootc)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run bootc: {e}"))?;
    // Drain pipes on threads so a chatty child cannot block on a full pipe.
    let mut out_pipe = child.stdout.take().expect("piped");
    let mut err_pipe = child.stderr.take().expect("piped");
    let out_t = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = std::io::Read::read_to_end(&mut out_pipe, &mut v);
        v
    });
    let err_t = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = std::io::Read::read_to_end(&mut err_pipe, &mut v);
        v
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                log_warn!("bootc {} timed out after {timeout:?}", args.join(" "));
                return Err(format!("bootc {} timed out", args.join(" ")));
            }
            Err(e) => return Err(format!("bootc wait failed: {e}")),
        }
    };
    let stdout = String::from_utf8_lossy(&out_t.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_t.join().unwrap_or_default())
        .trim()
        .to_string();
    if !status.success() {
        log_warn!("bootc {} failed: {stderr}", args.join(" "));
        return Err(format!("bootc {} failed: {stderr}", args.join(" ")));
    }
    Ok(stdout)
}

fn run_bootc_status(bootc: &std::path::Path, timeout: Duration) -> UpdateStatus {
    match run_bootc(bootc, &["status", "--format=json", "--format-version=1"], timeout) {
        Ok(json) => parse_bootc_status(&json),
        Err(e) => unavailable(e),
    }
}

/// Parses `bootc status --format=json --format-version=1` output leniently.
pub fn parse_bootc_status(json: &str) -> UpdateStatus {
    let v: Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(e) => return unavailable(format!("unparseable bootc status: {e}")),
    };
    let status = &v["status"];
    // ImageStatus object -> Deployment (same shape for "image" and "cachedUpdate").
    let image = |img: &Value| -> Option<Deployment> {
        if img.is_null() || !img.is_object() {
            return None;
        }
        let s = |val: &Value| val.as_str().map(str::to_string);
        Some(Deployment {
            image: s(&img["image"]["image"]),
            version: s(&img["version"]),
            timestamp: s(&img["timestamp"]),
            digest: s(&img["imageDigest"]),
        })
    };
    let dep = |key: &str| -> Option<Deployment> { image(status.get(key)?.get("image")?) };
    let cached_update = status.get("booted").and_then(|b| b.get("cachedUpdate")).and_then(image);
    let booted = dep("booted");
    UpdateStatus {
        schema: 1,
        available: booted.is_some(),
        staged: dep("staged"),
        rollback: dep("rollback"),
        cached_update,
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

    /// Writes a fake `bootc` that logs its arguments and answers like bootc.
    fn fake_bootc(dir: &std::path::Path, fail_upgrade: bool) -> PathBuf {
        let path = dir.join("bootc");
        let status = SAMPLE.replace(
            r#""pinned": false},"#,
            r#""pinned": false, "cachedUpdate": {"image": {"image": "ghcr.io/madgenius9/mados:dev", "transport": "registry"}, "version": "0.1.1-dev", "timestamp": "2026-10-07T00:00:00Z", "imageDigest": "sha256:ccc"}},"#,
        );
        let script = format!(
            "#!/bin/sh\necho \"$*\" >> '{log}'\ncase \"$1\" in\n  status) cat <<'JSON'\n{status}\nJSON\n  ;;\n  upgrade) {upgrade} ;;\n  rollback) echo 'Next boot: rollback deployment' ;;\nesac\n",
            log = dir.join("calls.log").display(),
            upgrade = if fail_upgrade { "echo 'error: registry unreachable' >&2; exit 1" } else { "echo staged" },
        );
        std::fs::write(&path, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn check_stage_and_rollback_use_fixed_bootc_commands() {
        let d = tempfile::tempdir().unwrap();
        let b = BootcUpdates {
            bootc: fake_bootc(d.path(), false),
            timeout: Duration::from_secs(10),
        };
        let st = zbus::block_on(b.check()).unwrap();
        assert!(st.available);
        let upd = st.cached_update.expect("cached update parsed");
        assert_eq!(
            (upd.version.as_deref(), upd.digest.as_deref()),
            (Some("0.1.1-dev"), Some("sha256:ccc"))
        );
        assert!(zbus::block_on(b.stage()).unwrap().contains("Restart"));
        assert!(zbus::block_on(b.rollback()).unwrap().contains("previous version"));
        let calls = std::fs::read_to_string(d.path().join("calls.log")).unwrap();
        assert_eq!(
            calls.lines().collect::<Vec<_>>(),
            [
                "upgrade --check",
                "status --format=json --format-version=1",
                "upgrade",
                "rollback"
            ]
        );
    }

    #[test]
    fn failed_upgrade_reports_stderr() {
        let d = tempfile::tempdir().unwrap();
        let b = BootcUpdates {
            bootc: fake_bootc(d.path(), true),
            timeout: Duration::from_secs(10),
        };
        let err = zbus::block_on(b.stage()).unwrap_err();
        assert!(err.contains("registry unreachable"), "{err}");
        assert!(
            zbus::block_on(b.check()).is_err(),
            "check fails when upgrade --check fails"
        );
    }

    #[test]
    fn missing_bootc_binary() {
        let s = run_bootc_status(std::path::Path::new("/nonexistent/bootc"), Duration::from_secs(1));
        assert!(!s.available);
    }
}
