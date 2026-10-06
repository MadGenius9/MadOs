//! Boot/session markers for automated VM smoke tests.
//!
//! Run by `mados-boot-report.service` after `graphical.target`. Writes
//! single-line machine-readable markers to stdout, which the unit routes to
//! the journal and the console (the serial port in test VMs):
//!
//!   MADOS_BOOT_OK version=… build=… state=running|degraded failed=…
//!   MADOS_SESSION_OK type=wayland class=user desktop=…   (or MADOS_SESSION_NONE)
//!   MADOS_APPS terminal=ok files=ok browser=ok settings=ok audio=ok
//!                                                        (dev images only; relayed
//!                                                        from `madosctl session-check`)
//!   MADOS_SHUTDOWN                                       (`--stop`, at shutdown)
//!
//! tests/smoke/vm_smoke.py waits for these strings. The format is a test
//! contract: change both together.

use crate::session_check;
use mados_core::{BuildInfo, Product};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const SYSTEMCTL: &str = "/usr/bin/systemctl";
/// Present only in development images (system/variants/dev).
const SESSION_CHECK_UNIT: &str = "/usr/lib/systemd/user/mados-session-check.service";

pub fn run(args: &[&str]) -> i32 {
    let mut timeout = Duration::from_secs(180);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match *a {
            "--stop" => {
                println!("MADOS_SHUTDOWN");
                return 0;
            }
            "--session-timeout" => match it.next().and_then(|v| v.parse().ok()) {
                Some(s) => timeout = Duration::from_secs(s),
                None => return usage(),
            },
            _ => return usage(),
        }
    }

    let product = Product::load();
    let build = BuildInfo::load().map(|b| b.build_id).unwrap_or_else(|| "none".into());
    // --wait: block until startup finishes (this unit is Type=simple, so it
    // does not hold up startup itself) to report the final state.
    let state = systemctl(&["is-system-running", "--wait"]).unwrap_or_else(|| "unknown".into());
    let failed = systemctl(&["list-units", "--failed", "--plain", "--no-legend", "--no-pager"])
        .map(|out| {
            out.lines()
                .filter_map(|l| l.split_whitespace().next())
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    println!(
        "MADOS_BOOT_OK version={} build={build} kernel={} state={state} failed={}",
        product.version.full(),
        kernel.trim(),
        if failed.is_empty() { "none" } else { &failed }
    );

    let deadline = Instant::now() + timeout;
    loop {
        match graphical_session() {
            Ok(Some((s, uid))) => {
                println!("MADOS_SESSION_OK {s}");
                if Path::new(SESSION_CHECK_UNIT).exists() {
                    relay_session_check(uid, timeout);
                }
                return 0;
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_secs(2)),
            Ok(None) => {
                println!("MADOS_SESSION_NONE reason=timeout");
                return 0;
            }
            Err(e) => {
                println!("MADOS_SESSION_NONE reason=logind-error");
                eprintln!("cannot query logind: {e}");
                return 0;
            }
        }
    }
}

/// Waits for the dev session check's report and prints it as `MADOS_APPS`.
fn relay_session_check(uid: u32, timeout: Duration) {
    let path = Path::new("/run/user")
        .join(uid.to_string())
        .join(session_check::REPORT_FILE);
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(r) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<session_check::Report>(&t).ok())
        {
            println!("MADOS_APPS {}", r.marker());
            return;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    println!("MADOS_APPS_NONE reason=timeout");
}

fn usage() -> i32 {
    eprintln!("usage: madosctl boot-report [--session-timeout SECS] | --stop");
    2
}

fn systemctl(args: &[&str]) -> Option<String> {
    let out = Command::new(SYSTEMCTL).args(args).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Finds an active graphical (wayland/x11) user session via logind; returns
/// its description and the owner's uid.
fn graphical_session() -> zbus::Result<Option<(String, u32)>> {
    zbus::block_on(async {
        let conn = zbus::Connection::system().await?;
        let mgr = zbus::Proxy::new(
            &conn,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?;
        let sessions: Vec<(String, u32, String, String, zbus::zvariant::OwnedObjectPath)> =
            mgr.call("ListSessions", &()).await?;
        for (_id, uid, _user, _seat, path) in sessions {
            let s = zbus::Proxy::new(&conn, "org.freedesktop.login1", path, "org.freedesktop.login1.Session").await?;
            let typ: String = s.get_property("Type").await?;
            let class: String = s.get_property("Class").await?;
            let state: String = s.get_property("State").await?;
            if (typ == "wayland" || typ == "x11") && class == "user" && state == "active" {
                let desktop: String = s.get_property("Desktop").await.unwrap_or_default();
                return Ok(Some((
                    format!(
                        "type={typ} class={class} desktop={}",
                        if desktop.is_empty() { "unknown" } else { &desktop }
                    ),
                    uid,
                )));
            }
        }
        Ok(None)
    })
}
