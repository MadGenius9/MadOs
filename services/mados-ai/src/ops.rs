//! Execution of approved intents through system APIs.
//!
//! Read-only answers come from `/proc` and `/sys` directly (unprivileged).
//! State changes go through the owning system service's D-Bus API, so the
//! service's own authorization (polkit) applies to the *user*:
//!   power      → org.mados.System1 (mados-daemon, polkit org.mados.system.power)
//!   Wi-Fi      → NetworkManager WirelessEnabled
//!   Bluetooth  → BlueZ Adapter1.Powered
//!   brightness → logind Session.SetBrightness (active session only)

use mados_core::sysinfo::{self, format_bytes, SessionEnv};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use zbus::zvariant::{OwnedObjectPath, Value};
use zbus::Connection;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub type OpResult = Result<String, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    Off,
    Reboot,
}

pub trait SystemOps: Send + Sync {
    fn system_info(&self) -> BoxFuture<'_, OpResult>;
    fn battery(&self) -> BoxFuture<'_, OpResult>;
    fn diagnose(&self) -> BoxFuture<'_, OpResult>;
    fn power(&self, action: Power) -> BoxFuture<'_, OpResult>;
    fn set_wifi(&self, enabled: bool) -> BoxFuture<'_, OpResult>;
    fn set_bluetooth(&self, enabled: bool) -> BoxFuture<'_, OpResult>;
    fn set_brightness(&self, percent: u8) -> BoxFuture<'_, OpResult>;
}

/// Live implementation against the running system.
pub struct LiveOps {
    pub root: PathBuf,
    pub system_bus: Option<Connection>,
    pub session_env: SessionEnv,
}

impl LiveOps {
    pub async fn new() -> Self {
        Self {
            root: PathBuf::from("/"),
            system_bus: Connection::system().await.ok(),
            session_env: SessionEnv::from_process_env(),
        }
    }

    fn bus(&self) -> Result<&Connection, String> {
        self.system_bus
            .as_ref()
            .ok_or_else(|| "system bus unavailable".to_string())
    }
}

fn on_off(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

impl SystemOps for LiveOps {
    fn system_info(&self) -> BoxFuture<'_, OpResult> {
        let env = self.session_env.clone();
        Box::pin(async move {
            let info = blocking::unblock(move || sysinfo::collect(&env)).await;
            Ok(summarize_info(&info))
        })
    }

    fn battery(&self) -> BoxFuture<'_, OpResult> {
        let root = self.root.clone();
        Box::pin(async move { blocking::unblock(move || battery_report(&root)).await })
    }

    fn diagnose(&self) -> BoxFuture<'_, OpResult> {
        let root = self.root.clone();
        Box::pin(async move { blocking::unblock(move || performance_report(&root)).await })
    }

    fn power(&self, action: Power) -> BoxFuture<'_, OpResult> {
        Box::pin(async move {
            let proxy = mados_api::SystemProxy::new(self.bus()?)
                .await
                .map_err(|e| e.to_string())?;
            let r = match action {
                Power::Off => proxy.power_off().await,
                Power::Reboot => proxy.reboot().await,
            };
            match r {
                Ok(()) => Ok(match action {
                    Power::Off => "Shutting down.".into(),
                    Power::Reboot => "Restarting.".into(),
                }),
                Err(zbus::Error::MethodError(name, _, _)) if name.as_str().ends_with("NotAuthorized") => {
                    Err("Not authorized to change the power state.".into())
                }
                Err(e) => Err(format!("System service error: {e}")),
            }
        })
    }

    fn set_wifi(&self, enabled: bool) -> BoxFuture<'_, OpResult> {
        Box::pin(async move {
            let props = zbus::fdo::PropertiesProxy::builder(self.bus()?)
                .destination("org.freedesktop.NetworkManager")
                .and_then(|b| b.path("/org/freedesktop/NetworkManager"))
                .map_err(|e| e.to_string())?
                .build()
                .await
                .map_err(|e| e.to_string())?;
            props
                .set(
                    "org.freedesktop.NetworkManager"
                        .try_into()
                        .map_err(|e: zbus::names::Error| e.to_string())?,
                    "WirelessEnabled",
                    Value::from(enabled),
                )
                .await
                .map_err(|e| format!("NetworkManager refused: {e}"))?;
            Ok(format!("Wi-Fi turned {}.", on_off(enabled)))
        })
    }

    fn set_bluetooth(&self, enabled: bool) -> BoxFuture<'_, OpResult> {
        Box::pin(async move {
            let bus = self.bus()?;
            let om = zbus::fdo::ObjectManagerProxy::builder(bus)
                .destination("org.bluez")
                .and_then(|b| b.path("/"))
                .map_err(|e| e.to_string())?
                .build()
                .await
                .map_err(|e| e.to_string())?;
            let objects = om
                .get_managed_objects()
                .await
                .map_err(|_| "Bluetooth service is not running.".to_string())?;
            let mut adapters: Vec<&OwnedObjectPath> = objects
                .iter()
                .filter(|(_, ifaces)| ifaces.keys().any(|i| i.as_str() == "org.bluez.Adapter1"))
                .map(|(p, _)| p)
                .collect();
            adapters.sort_by(|a, b| a.as_str().cmp(b.as_str()));
            let adapter = adapters.first().ok_or("No Bluetooth adapter found.")?;
            let props = zbus::fdo::PropertiesProxy::builder(bus)
                .destination("org.bluez")
                .and_then(|b| b.path(adapter.as_str()))
                .map_err(|e| e.to_string())?
                .build()
                .await
                .map_err(|e| e.to_string())?;
            props
                .set(
                    "org.bluez.Adapter1"
                        .try_into()
                        .map_err(|e: zbus::names::Error| e.to_string())?,
                    "Powered",
                    Value::from(enabled),
                )
                .await
                .map_err(|e| format!("BlueZ refused: {e}"))?;
            Ok(format!("Bluetooth turned {}.", on_off(enabled)))
        })
    }

    fn set_brightness(&self, percent: u8) -> BoxFuture<'_, OpResult> {
        let root = self.root.clone();
        Box::pin(async move {
            let (name, max) = blocking::unblock(move || first_backlight(&root))
                .await
                .ok_or("No adjustable display backlight found (external monitor or VM).")?;
            let value = (u64::from(percent.min(100)) * u64::from(max) / 100) as u32;
            let bus = self.bus()?;
            // The user's graphical session, resolved by logind for this uid.
            let user = zbus::Proxy::new(
                bus,
                "org.freedesktop.login1",
                "/org/freedesktop/login1/user/self",
                "org.freedesktop.login1.User",
            )
            .await
            .map_err(|e| e.to_string())?;
            let (_id, session_path): (String, OwnedObjectPath) = user
                .get_property("Display")
                .await
                .map_err(|e| format!("no graphical session: {e}"))?;
            let session = zbus::Proxy::new(
                bus,
                "org.freedesktop.login1",
                session_path,
                "org.freedesktop.login1.Session",
            )
            .await
            .map_err(|e| e.to_string())?;
            session
                .call_method("SetBrightness", &("backlight", name.as_str(), value))
                .await
                .map_err(|e| format!("logind refused: {e}"))?;
            Ok(format!("Brightness set to {percent}%."))
        })
    }
}

pub fn summarize_info(i: &mados_core::SystemInfo) -> String {
    let na = "unavailable";
    let mut lines = vec![format!("{} {}", i.product_name, i.product_version)];
    if let Some(b) = &i.base_os {
        lines.push(format!("Base: {b}"));
    }
    lines.push(format!("Kernel: {}", i.kernel.as_deref().unwrap_or(na)));
    lines.push(format!(
        "CPU: {}",
        i.cpu
            .as_ref()
            .map(|c| format!("{} ({} threads)", c.model, c.logical_cpus))
            .unwrap_or(na.into())
    ));
    lines.push(format!(
        "Memory: {}",
        i.memory
            .as_ref()
            .map(|m| format_bytes(m.total_bytes))
            .unwrap_or(na.into())
    ));
    for g in &i.gpus {
        lines.push(format!(
            "GPU: {}",
            [g.vendor.as_deref(), g.model.as_deref()]
                .iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    lines.join("\n")
}

fn read(root: &Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(root.join(rel))
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn battery_report(root: &Path) -> OpResult {
    let dir = root.join("sys/class/power_supply");
    let mut supplies: Vec<_> = std::fs::read_dir(&dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    supplies.sort();
    let mut parts = Vec::new();
    for p in supplies {
        let r = |f: &str| std::fs::read_to_string(p.join(f)).ok().map(|s| s.trim().to_string());
        if r("type").as_deref() != Some("Battery") {
            continue;
        }
        let Some(cap) = r("capacity") else { continue };
        let status = r("status").unwrap_or_else(|| "Unknown".into());
        parts.push(format!("{cap}% ({})", status.to_lowercase()));
    }
    if parts.is_empty() {
        Ok("No battery detected. This looks like a desktop or a virtual machine.".into())
    } else {
        Ok(format!("Battery: {}.", parts.join(", ")))
    }
}

pub fn performance_report(root: &Path) -> OpResult {
    let load = read(root, "proc/loadavg").ok_or("cannot read /proc/loadavg")?;
    let load1: f64 = load
        .split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let cpus = read(root, "proc/cpuinfo")
        .map(|c| c.lines().filter(|l| l.starts_with("processor")).count())
        .filter(|n| *n > 0)
        .unwrap_or(1);
    let mem: HashMap<String, u64> = read(root, "proc/meminfo")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.trim_end_matches(':').to_string(), it.next()?.parse().ok()?))
        })
        .collect();
    let mut findings = Vec::new();
    if load1 > cpus as f64 * 1.5 {
        findings.push(format!("The CPU is overloaded (load {load1:.2} on {cpus} threads)."));
    }
    if let (Some(total), Some(avail)) = (mem.get("MemTotal"), mem.get("MemAvailable")) {
        if *total > 0 && avail * 100 / total < 10 {
            findings.push(format!(
                "Memory is nearly full ({} free of {}).",
                format_bytes(avail * 1024),
                format_bytes(total * 1024)
            ));
        }
    }
    let top = top_memory_processes(root, 3);
    let mut msg = if findings.is_empty() {
        format!("Nothing looks overloaded right now (load {load1:.2} on {cpus} threads).")
    } else {
        findings.join(" ")
    };
    if !top.is_empty() {
        let list: Vec<String> = top
            .iter()
            .map(|(n, kib)| format!("{n} ({})", format_bytes(kib * 1024)))
            .collect();
        msg.push_str(&format!(" Largest memory users: {}.", list.join(", ")));
    }
    Ok(msg)
}

fn top_memory_processes(root: &Path, n: usize) -> Vec<(String, u64)> {
    let Ok(rd) = std::fs::read_dir(root.join("proc")) else {
        return Vec::new();
    };
    let mut procs: Vec<(String, u64)> = rd
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|e| {
            let status = std::fs::read_to_string(e.path().join("status")).ok()?;
            let mut name = None;
            let mut rss = None;
            for l in status.lines() {
                if let Some(v) = l.strip_prefix("Name:") {
                    name = Some(v.trim().to_string());
                } else if let Some(v) = l.strip_prefix("VmRSS:") {
                    rss = v.split_whitespace().next().and_then(|x| x.parse().ok());
                }
            }
            Some((name?, rss?))
        })
        .collect();
    procs.sort_by_key(|p| std::cmp::Reverse(p.1));
    procs.truncate(n);
    procs
}

fn first_backlight(root: &Path) -> Option<(String, u32)> {
    let mut names: Vec<String> = std::fs::read_dir(root.join("sys/class/backlight"))
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names.into_iter().find_map(|n| {
        let max: u32 = read(root, &format!("sys/class/backlight/{n}/max_brightness"))?
            .parse()
            .ok()?;
        (max > 0).then_some((n, max))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn w(root: &Path, rel: &str, s: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    #[test]
    fn battery_present_and_absent() {
        let d = tempfile::tempdir().unwrap();
        assert!(battery_report(d.path()).unwrap().starts_with("No battery"));
        w(d.path(), "sys/class/power_supply/AC/type", "Mains\n");
        w(d.path(), "sys/class/power_supply/BAT0/type", "Battery\n");
        w(d.path(), "sys/class/power_supply/BAT0/capacity", "73\n");
        w(d.path(), "sys/class/power_supply/BAT0/status", "Discharging\n");
        assert_eq!(battery_report(d.path()).unwrap(), "Battery: 73% (discharging).");
    }

    #[test]
    fn performance_findings() {
        let d = tempfile::tempdir().unwrap();
        w(d.path(), "proc/loadavg", "9.00 5.00 2.00 3/400 1234\n");
        w(d.path(), "proc/cpuinfo", "processor : 0\nprocessor : 1\n");
        w(
            d.path(),
            "proc/meminfo",
            "MemTotal: 1000000 kB\nMemAvailable: 50000 kB\n",
        );
        w(d.path(), "proc/42/status", "Name:\tfirefox\nVmRSS:\t 800000 kB\n");
        w(d.path(), "proc/43/status", "Name:\tkworker\n");
        let r = performance_report(d.path()).unwrap();
        assert!(r.contains("CPU is overloaded"), "{r}");
        assert!(r.contains("Memory is nearly full"), "{r}");
        assert!(r.contains("firefox"), "{r}");
    }

    #[test]
    fn backlight_detection() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(first_backlight(d.path()), None);
        w(
            d.path(),
            "sys/class/backlight/intel_backlight/max_brightness",
            "96000\n",
        );
        assert_eq!(first_backlight(d.path()), Some(("intel_backlight".into(), 96000)));
    }
}
