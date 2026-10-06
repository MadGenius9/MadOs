//! display::set_brightness against a mock logind on a private bus.

use mados_api::display;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

mod common;
use common::mock_logind;

struct PrivateBus(Child, String);

impl PrivateBus {
    fn start() -> Option<Self> {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut line = String::new();
        BufReader::new(child.stdout.take()?).read_line(&mut line).ok()?;
        Some(Self(child, line.trim().to_string()))
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn sysfs(max: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("sys/class/backlight/intel_backlight");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("max_brightness"), max).unwrap();
    std::fs::write(dir.join("brightness"), "100\n").unwrap();
    d
}

#[test]
fn sets_brightness_on_users_session() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("SKIP: dbus-daemon not available");
        return;
    };
    zbus::block_on(async {
        let (_server, calls) = mock_logind::serve(&bus.1, false).await;
        let conn = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let root = sysfs("1000\n");
        assert!(display::set_brightness(&conn, root.path(), 40).await.unwrap());
        assert_eq!(
            *calls.lock().unwrap(),
            vec![("backlight".into(), "intel_backlight".into(), 400)]
        );

        let no_backlight = tempfile::tempdir().unwrap();
        assert!(!display::set_brightness(&conn, no_backlight.path(), 40).await.unwrap());
        assert_eq!(calls.lock().unwrap().len(), 1, "nothing sent without a backlight");
        assert!(!display::set_brightness(&conn, Path::new("/nonexistent"), 10)
            .await
            .unwrap());
    });
}

#[test]
fn refusal_is_reported() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("SKIP: dbus-daemon not available");
        return;
    };
    zbus::block_on(async {
        let (_server, calls) = mock_logind::serve(&bus.1, true).await;
        let conn = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let root = sysfs("1000\n");
        let err = display::set_brightness(&conn, root.path(), 40)
            .await
            .expect_err("refused");
        assert!(err.to_string().contains("not the active session"), "{err}");
        assert!(calls.lock().unwrap().is_empty());
    });
}
