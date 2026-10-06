//! `madosctl` — MadOS command-line tool.
//!
//!   madosctl version            product version
//!   madosctl about [--json]     system information (local, unprivileged)
//!   madosctl update-status      deployment status via org.mados.System1
//!   madosctl boot-report        boot/session marker for VM smoke tests

mod boot_report;

use mados_core::sysinfo::{self, format_bytes, SessionEnv};
use mados_core::Product;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["version"] | ["--version"] => {
            println!("{}", Product::load().version.full());
            0
        }
        ["about"] => {
            print!("{}", about_text(&sysinfo::collect(&SessionEnv::from_process_env())));
            0
        }
        ["about", "--json"] => {
            let info = sysinfo::collect(&SessionEnv::from_process_env());
            println!("{}", serde_json::to_string_pretty(&info).expect("serializable"));
            0
        }
        ["update-status"] => update_status(),
        ["boot-report", rest @ ..] => boot_report::run(rest),
        _ => {
            eprintln!(
                "usage: madosctl version | about [--json] | update-status | boot-report [--session-timeout SECS]"
            );
            2
        }
    };
    std::process::exit(code);
}

pub fn about_text(i: &mados_core::SystemInfo) -> String {
    let na = || "Unavailable".to_string();
    let mut rows: Vec<(&str, String)> = vec![
        ("Product", format!("{} {}", i.product_name, i.product_version)),
        (
            "Build",
            i.build_id.clone().unwrap_or_else(|| "Not a MadOS image build".into()),
        ),
        (
            "Base",
            i.base_os
                .clone()
                .or_else(|| i.os_pretty_name.clone())
                .unwrap_or_else(na),
        ),
        ("Kernel", i.kernel.clone().unwrap_or_else(na)),
        ("Architecture", i.architecture.clone()),
        ("Hostname", i.hostname.clone().unwrap_or_else(na)),
        (
            "CPU",
            i.cpu
                .as_ref()
                .map(|c| format!("{} ({} threads)", c.model, c.logical_cpus))
                .unwrap_or_else(na),
        ),
        (
            "Memory",
            i.memory
                .as_ref()
                .map(|m| format_bytes(m.total_bytes))
                .unwrap_or_else(na),
        ),
    ];
    if i.gpus.is_empty() {
        rows.push(("Graphics", na()));
    }
    for g in &i.gpus {
        let name = [g.vendor.as_deref(), g.model.as_deref()]
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        let driver = g.driver.as_deref().map(|d| format!(" [{d}]")).unwrap_or_default();
        rows.push((
            "Graphics",
            format!("{}{driver}", if name.is_empty() { g.card.clone() } else { name }),
        ));
    }
    for s in &i.storage {
        rows.push((
            "Storage",
            format!(
                "{}: {} free of {}",
                s.mount_point,
                format_bytes(s.available_bytes),
                format_bytes(s.total_bytes)
            ),
        ));
    }
    let session = match (&i.desktop, &i.session_type) {
        (Some(d), Some(t)) => format!("{d} ({t})"),
        (Some(d), None) => d.clone(),
        (None, Some(t)) => t.clone(),
        (None, None) => "No graphical session".into(),
    };
    rows.push(("Session", session));
    rows.iter().map(|(k, v)| format!("{k:<13} {v}\n")).collect()
}

fn update_status() -> i32 {
    let r: zbus::Result<String> = zbus::block_on(async {
        let conn = zbus::Connection::system().await?;
        mados_api::SystemProxy::new(&conn).await?.get_update_status().await
    });
    match r {
        Ok(json) => {
            let s: mados_api::UpdateStatus = match serde_json::from_str(&json) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("madosctl: bad reply from mados-daemon: {e}");
                    return 1;
                }
            };
            if !s.available {
                println!("Updates: unavailable ({})", s.message.unwrap_or_default());
                return 0;
            }
            let show = |label: &str, d: &Option<mados_api::Deployment>| match d {
                Some(d) => println!(
                    "{label:<9} {} {} {}",
                    d.image.as_deref().unwrap_or("?"),
                    d.version.as_deref().unwrap_or(""),
                    d.digest.as_deref().map(|x| &x[..x.len().min(19)]).unwrap_or("")
                ),
                None => println!("{label:<9} none"),
            };
            show("Booted", &s.booted);
            show("Staged", &s.staged);
            show("Rollback", &s.rollback);
            0
        }
        Err(e) => {
            eprintln!("madosctl: cannot reach mados-daemon: {e}");
            1
        }
    }
}
