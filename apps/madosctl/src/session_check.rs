//! `madosctl session-check`: development-image check run inside the user's
//! graphical session by the dev-only `mados-session-check.service` user unit.
//!
//! Launches the default applications and records whether each one reached
//! its main loop, detected by the application claiming its D-Bus name on the
//! session bus (a stronger signal than "the process was spawned"). Also
//! checks that an audio device and a PipeWire default sink exist. The result
//! is written to `$XDG_RUNTIME_DIR/mados/session-check.json`, which
//! `madosctl boot-report` (system service) relays to the serial console as a
//! `MADOS_APPS` marker for the VM smoke test.
//!
//! Programs are started by fixed absolute paths with fixed arguments.

use serde::{Deserialize, Serialize};
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
    let report = Report {
        schema: REPORT_SCHEMA,
        apps: results,
        audio: audio_status(),
    };
    println!("audio: {}", report.audio);
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
        };
        assert_eq!(r.marker(), "terminal=ok browser=running audio=ok");
    }
}
