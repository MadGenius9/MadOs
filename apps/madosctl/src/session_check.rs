//! `madosctl session-check`: development-image check run inside the user's
//! graphical session by the dev-only `mados-session-check.service` user unit.
//!
//! Launches the default applications and records whether each one reached
//! its main loop, detected by the application claiming its D-Bus name on the
//! session bus (a stronger signal than "the process was spawned"). Also
//! checks that an audio device and a PipeWire default sink exist, and which
//! first-login welcome window is open (MadOS's, not KDE's) and whether
//! Plasma reads the MadOS desktop defaults. The result
//! is written to `$XDG_RUNTIME_DIR/mados/session-check.json`, which
//! `madosctl boot-report` (system service) relays to the serial console as a
//! `MADOS_APPS` marker for the VM smoke test.
//!
//! Programs are started by fixed absolute paths with fixed arguments.

use serde::{Deserialize, Serialize};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const REPORT_SCHEMA: u32 = 1;
pub const REPORT_FILE: &str = "mados/session-check.json";

/// Default applications of the 0.1 desktop: (report key, program, args, D-Bus name prefix).
const APPS: &[(&str, &str, &[&str], &str)] = &[
    ("terminal", "/usr/bin/konsole", &[], "org.kde.konsole"),
    ("files", "/usr/bin/dolphin", &[], "org.kde.dolphin"),
    (
        "browser",
        "/usr/bin/firefox",
        &["--new-instance", "about:blank"],
        "org.mozilla.firefox",
    ),
    ("settings", "/usr/bin/mados-settings", &[], "org.mados.Settings"),
];
const WPCTL: &str = "/usr/bin/wpctl";
/// Process names (`/proc/<pid>/comm`) of the first-login welcome windows.
const FIRST_RUN_COMM: &str = "mados-first-run";
const KDE_WELCOME_COMM: &str = "plasma-welcome";
const PLASMASHELL_COMM: &str = "plasmashell";
/// MadOS desktop defaults (look-and-feel, accent, kded); must come first in
/// Plasma's XDG_CONFIG_DIRS (scripts/stage-system.py).
const MADOS_XDG_DIR: &str = "/usr/share/mados/xdg";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Claimed its D-Bus name: initialised and running its main loop.
    Ok,
    /// Still running at the deadline but never claimed its name.
    Running,
    /// Exited before the deadline.
    Exited,
    /// Program not installed.
    Missing,
    /// Could not be started.
    Failed,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Ok => "ok",
            Outcome::Running => "running",
            Outcome::Exited => "exited",
            Outcome::Missing => "missing",
            Outcome::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppResult {
    pub name: String,
    pub outcome: Outcome,
    pub seconds: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Report {
    pub schema: u32,
    pub apps: Vec<AppResult>,
    /// "ok" (device and default sink), "no-device", "no-sink", "unknown".
    pub audio: String,
    /// org.mados.System1 reachable from the session (system bus activation).
    #[serde(default)]
    pub daemon: String,
    /// bootc deployment status through the daemon: "ok" or a short reason.
    #[serde(default)]
    pub bootc: String,
    /// org.mados.Assistant1 answered a read-only request.
    #[serde(default)]
    pub assistant: String,
    /// MadOS first-run window (XDG autostart): "running" or "absent".
    #[serde(default)]
    pub first_run: String,
    /// KDE Welcome Center, which MadOS turns off: "absent" or "running".
    #[serde(default)]
    pub kde_welcome: String,
    /// Plasma's config search path starts with the MadOS defaults: "ok" or why not.
    #[serde(default)]
    pub defaults: String,
}

impl Report {
    /// One-line marker body: `terminal=ok files=ok … audio=ok`.
    pub fn marker(&self) -> String {
        let mut parts: Vec<String> = self
            .apps
            .iter()
            .map(|a| format!("{}={}", a.name, a.outcome.as_str()))
            .collect();
        parts.push(format!("audio={}", self.audio));
        for (k, v) in [
            ("daemon", &self.daemon),
            ("bootc", &self.bootc),
            ("assistant", &self.assistant),
            ("first_run", &self.first_run),
            ("kde_welcome", &self.kde_welcome),
            ("defaults", &self.defaults),
        ] {
            if !v.is_empty() {
                parts.push(format!("{k}={v}"));
            }
        }
        parts.join(" ")
    }
}

fn bus_has_prefix(conn: &zbus::blocking::Connection, prefix: &str) -> bool {
    let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(conn) else {
        return false;
    };
    dbus.list_names()
        .map(|names| {
            names.iter().any(|n| {
                n.as_str() == prefix
                    || n.as_str().starts_with(&format!("{prefix}-"))
                    || n.as_str().starts_with(&format!("{prefix}."))
            })
        })
        .unwrap_or(false)
}

/// Starts `cmd` and waits until a bus name matching `prefix` appears on `conn`.
pub fn check_command(
    conn: &zbus::blocking::Connection,
    name: &str,
    mut cmd: Command,
    prefix: &str,
    timeout: Duration,
) -> (AppResult, Option<Child>) {
    let start = Instant::now();
    let result = |outcome| AppResult {
        name: name.to_string(),
        outcome,
        seconds: start.elapsed().as_secs_f32(),
    };
    let mut child = match cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return (result(Outcome::Failed), None),
    };
    let deadline = start + timeout;
    loop {
        if bus_has_prefix(conn, prefix) {
            return (result(Outcome::Ok), Some(child));
        }
        if let Ok(Some(_)) = child.try_wait() {
            return (result(Outcome::Exited), None);
        }
        if Instant::now() >= deadline {
            return (result(Outcome::Running), Some(child));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Number of sound cards the kernel detected, from `/proc/asound/cards`.
pub fn sound_cards(text: &str) -> usize {
    text.lines()
        .filter(|l| {
            let t = l.trim_start();
            t.split_whitespace()
                .next()
                .is_some_and(|w| w.chars().all(|c| c.is_ascii_digit()))
                && t.contains('[')
        })
        .count()
}

fn audio_status() -> String {
    let cards = std::fs::read_to_string("/proc/asound/cards")
        .map(|t| sound_cards(&t))
        .unwrap_or(0);
    if cards == 0 {
        return "no-device".into();
    }
    if !Path::new(WPCTL).exists() {
        return "unknown".into();
    }
    // Exit status 0 when PipeWire/WirePlumber have a default output sink.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let ok = Command::new(WPCTL)
            .args(["inspect", "@DEFAULT_AUDIO_SINK@"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            return "ok".into();
        }
        if Instant::now() >= deadline {
            return "no-sink".into();
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// One-word value for the marker (no spaces).
fn token(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '-'
            }
        })
        .take(40)
        .collect()
}

/// Exercises org.mados.System1 on the system bus: (daemon, bootc).
fn check_daemon() -> (String, String) {
    let r = (|| -> zbus::Result<(String, String)> {
        let conn = zbus::blocking::Connection::system()?;
        let p = mados_api::SystemProxyBlocking::new(&conn)?;
        let info: mados_core::SystemInfo =
            serde_json::from_str(&p.get_system_info()?).map_err(|e| zbus::Error::Failure(e.to_string()))?;
        let daemon = if info.product_version.is_empty() {
            "bad-reply".to_string()
        } else {
            "ok".to_string()
        };
        let upd: mados_api::UpdateStatus =
            serde_json::from_str(&p.get_update_status()?).map_err(|e| zbus::Error::Failure(e.to_string()))?;
        let bootc = if upd.available {
            "ok".into()
        } else {
            token(upd.message.as_deref().unwrap_or("unavailable"))
        };
        Ok((daemon, bootc))
    })();
    r.unwrap_or_else(|e| (format!("error-{}", token(&e.to_string())), "unknown".into()))
}

/// Exercises org.mados.Assistant1 on the session bus with a read-only request.
fn check_assistant() -> String {
    let r = (|| -> zbus::Result<mados_api::AssistantReply> {
        let conn = zbus::blocking::Connection::session()?;
        let json = mados_api::AssistantProxyBlocking::new(&conn)?.ask("what version am I running")?;
        serde_json::from_str(&json).map_err(|e| zbus::Error::Failure(e.to_string()))
    })();
    match r {
        Ok(reply) if reply.status == mados_api::ReplyStatus::Done => "ok".into(),
        Ok(reply) => token(&format!("{:?}", reply.status)),
        Err(e) => format!("error-{}", token(&e.to_string())),
    }
}

/// The `/proc/<pid>` directory of a process named `comm` owned by `uid`,
/// scanning `proc_root` (normally `/proc`).
pub fn find_process(proc_root: &Path, comm: &str, uid: u32) -> Option<PathBuf> {
    std::fs::read_dir(proc_root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|n| n.to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
                && std::fs::metadata(path).map(|m| m.uid() == uid).unwrap_or(false)
                && std::fs::read_to_string(path.join("comm")).is_ok_and(|c| c.trim_end() == comm)
        })
}

pub fn process_running(proc_root: &Path, comm: &str, uid: u32) -> bool {
    find_process(proc_root, comm, uid).is_some()
}

/// Checks a process environment (`/proc/<pid>/environ`, NUL-separated) for
/// the MadOS defaults directory at the front of XDG_CONFIG_DIRS.
pub fn defaults_status(environ: &[u8]) -> String {
    let dirs = environ
        .split(|b| *b == 0)
        .find_map(|v| v.strip_prefix(b"XDG_CONFIG_DIRS="))
        .map(|v| String::from_utf8_lossy(v).into_owned());
    match dirs {
        None => "no-xdg-config-dirs".into(),
        Some(d) if d.split(':').next() == Some(MADOS_XDG_DIR) => "ok".into(),
        Some(d) if d.split(':').any(|x| x == MADOS_XDG_DIR) => "not-first".into(),
        Some(_) => "missing".into(),
    }
}

fn check_defaults(uid: u32) -> String {
    match find_process(Path::new("/proc"), PLASMASHELL_COMM, uid) {
        None => "no-plasmashell".into(),
        Some(pid) => match std::fs::read(pid.join("environ")) {
            Ok(env) => defaults_status(&env),
            Err(_) => "unreadable".into(),
        },
    }
}

fn presence(running: bool) -> String {
    if running { "running" } else { "absent" }.to_string()
}

fn report_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|d| PathBuf::from(d).join(REPORT_FILE))
}

pub fn run(args: &[&str]) -> i32 {
    let mut timeout = Duration::from_secs(90);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match *a {
            "--timeout" => match it.next().and_then(|v| v.parse().ok()) {
                Some(s) => timeout = Duration::from_secs(s),
                None => return usage(),
            },
            _ => return usage(),
        }
    }
    let Some(path) = report_path() else {
        eprintln!("session-check: XDG_RUNTIME_DIR is not set (not in a user session)");
        return 1;
    };
    let conn = match zbus::blocking::Connection::session() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("session-check: no session bus: {e}");
            return 1;
        }
    };
    let mut results = Vec::new();
    let mut children = Vec::new();
    for (name, program, args, prefix) in APPS {
        let (res, child) = if Path::new(program).exists() {
            let mut cmd = Command::new(program);
            cmd.args(*args);
            check_command(&conn, name, cmd, prefix, timeout)
        } else {
            (
                AppResult {
                    name: name.to_string(),
                    outcome: Outcome::Missing,
                    seconds: 0.0,
                },
                None,
            )
        };
        println!("{name}: {} after {:.1}s", res.outcome.as_str(), res.seconds);
        results.push(res);
        children.extend(child);
    }
    let (daemon, bootc) = check_daemon();
    // /proc/self belongs to this process's user.
    let uid = std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(u32::MAX);
    let report = Report {
        schema: REPORT_SCHEMA,
        apps: results,
        audio: audio_status(),
        daemon,
        bootc,
        assistant: check_assistant(),
        first_run: presence(process_running(Path::new("/proc"), FIRST_RUN_COMM, uid)),
        kde_welcome: presence(process_running(Path::new("/proc"), KDE_WELCOME_COMM, uid)),
        defaults: check_defaults(uid),
    };
    println!(
        "audio: {} daemon: {} bootc: {} assistant: {} first_run: {} kde_welcome: {} defaults: {}",
        report.audio,
        report.daemon,
        report.bootc,
        report.assistant,
        report.first_run,
        report.kde_welcome,
        report.defaults
    );
    for mut c in children {
        let _ = c.kill();
        let _ = c.wait();
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::write(&path, serde_json::to_string(&report).expect("serializable")) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("session-check: cannot write {}: {e}", path.display());
            1
        }
    }
}

fn usage() -> i32 {
    eprintln!("usage: madosctl session-check [--timeout SECS]");
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Private dbus-daemon, killed on drop.
    struct PrivateBus(std::process::Child, String);

    impl PrivateBus {
        fn start() -> Option<Self> {
            use std::io::BufRead;
            let mut child = Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address=1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .ok()?;
            let mut line = String::new();
            std::io::BufReader::new(child.stdout.take()?)
                .read_line(&mut line)
                .ok()?;
            Some(Self(child, line.trim().to_string()))
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// Helper process: when re-executed with MADOS_TEST_CLAIM set, claims
    /// that bus name and stays alive (a stand-in for an application).
    #[test]
    fn claimer_helper() {
        let Ok(name) = std::env::var("MADOS_TEST_CLAIM") else {
            return;
        };
        let conn = zbus::blocking::connection::Builder::session()
            .unwrap()
            .name(name)
            .unwrap()
            .build()
            .unwrap();
        std::thread::sleep(Duration::from_secs(30));
        drop(conn);
    }

    #[test]
    fn detects_bus_name_running_and_exit() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("SKIP: dbus-daemon not available");
            return;
        };
        let conn = zbus::blocking::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .unwrap();

        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args(["--exact", "session_check::tests::claimer_helper", "--nocapture"])
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.1)
            .env("MADOS_TEST_CLAIM", "org.example.App-123");
        let (r, child) = check_command(&conn, "app", cmd, "org.example.App", Duration::from_secs(20));
        assert_eq!(r.outcome, Outcome::Ok);
        if let Some(mut c) = child {
            let _ = c.kill();
            let _ = c.wait();
        }

        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let (r, child) = check_command(&conn, "idle", cmd, "org.example.Never", Duration::from_secs(1));
        assert_eq!(r.outcome, Outcome::Running, "alive but never claimed its name");
        if let Some(mut c) = child {
            let _ = c.kill();
            let _ = c.wait();
        }

        let (r, child) = check_command(
            &conn,
            "crash",
            Command::new("false"),
            "org.example.Never",
            Duration::from_secs(5),
        );
        assert_eq!(r.outcome, Outcome::Exited);
        assert!(child.is_none());

        let (r, _) = check_command(
            &conn,
            "absent",
            Command::new("/nonexistent/app"),
            "x",
            Duration::from_secs(1),
        );
        assert_eq!(r.outcome, Outcome::Failed);
    }

    #[test]
    fn counts_sound_cards() {
        let qemu = " 0 [Intel         ]: HDA-Intel - HDA Intel\n                      HDA Intel at 0xfebf0000 irq 34\n";
        assert_eq!(sound_cards(qemu), 1);
        assert_eq!(sound_cards("--- no soundcards ---\n"), 0);
        let two = " 0 [PCH            ]: HDA-Intel - HDA Intel PCH\n    x\n 1 [NVidia         ]: HDA-Intel - HDA NVidia\n    y\n";
        assert_eq!(sound_cards(two), 2);
    }

    #[test]
    fn finds_processes_by_name_and_owner() {
        let root = std::env::temp_dir().join(format!("mados-proc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (pid, comm) in [("42", "mados-first-run\n"), ("7", "plasmashell\n")] {
            std::fs::create_dir_all(root.join(pid)).unwrap();
            std::fs::write(root.join(pid).join("comm"), comm).unwrap();
        }
        std::fs::create_dir_all(root.join("self")).unwrap();
        std::fs::write(root.join("self").join("comm"), "plasma-welcome\n").unwrap();
        let me = std::fs::metadata(&root).unwrap().uid();
        assert!(process_running(&root, "mados-first-run", me));
        assert!(!process_running(&root, "mados-first-run", me + 1));
        assert!(!process_running(&root, "plasma-welcome", me)); // not a pid directory
        assert!(!process_running(&root.join("missing"), "plasmashell", me));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn checks_mados_defaults_first_in_config_dirs() {
        let env = |v: &str| format!("HOME=/home/m\0{v}\0LANG=C\0").into_bytes();
        assert_eq!(
            defaults_status(&env("XDG_CONFIG_DIRS=/usr/share/mados/xdg:/etc/xdg")),
            "ok"
        );
        assert_eq!(
            defaults_status(&env("XDG_CONFIG_DIRS=/etc/xdg:/usr/share/mados/xdg")),
            "not-first"
        );
        assert_eq!(defaults_status(&env("XDG_CONFIG_DIRS=/etc/xdg")), "missing");
        assert_eq!(defaults_status(&env("X=1")), "no-xdg-config-dirs");
    }

    #[test]
    fn marker_format() {
        let r = Report {
            schema: 1,
            apps: vec![
                AppResult {
                    name: "terminal".into(),
                    outcome: Outcome::Ok,
                    seconds: 1.0,
                },
                AppResult {
                    name: "browser".into(),
                    outcome: Outcome::Running,
                    seconds: 90.0,
                },
            ],
            audio: "ok".into(),
            daemon: "ok".into(),
            bootc: "ok".into(),
            assistant: String::new(),
            first_run: "running".into(),
            kde_welcome: "absent".into(),
            defaults: "ok".into(),
        };
        assert_eq!(
            r.marker(),
            "terminal=ok browser=running audio=ok daemon=ok bootc=ok first_run=running kde_welcome=absent defaults=ok"
        );
        assert_eq!(token("bootc is not installed; x"), "bootc-is-not-installed--x");
    }
}
