//! Real system information from Linux interfaces.
//!
//! Every value is optional: a missing file or unsupported interface yields
//! `None` (shown as "Unavailable" by UIs), never a fabricated value.
//!
//! All filesystem reads go through [`Probe::root`] so tests can point the
//! probe at a fixture tree instead of `/`.

use crate::{BuildInfo, Product};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};

/// JSON schema version of [`SystemInfo`]; bump on incompatible changes.
pub const SYSINFO_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct SystemInfo {
    pub schema: u32,
    pub product_name: String,
    pub product_version: String,
    pub build_id: Option<String>,
    pub variant: Option<String>,
    pub os_pretty_name: Option<String>,
    pub base_os: Option<String>,
    pub kernel: Option<String>,
    pub architecture: String,
    pub hostname: Option<String>,
    pub cpu: Option<CpuInfo>,
    pub memory: Option<MemoryInfo>,
    pub gpus: Vec<GpuInfo>,
    pub storage: Vec<StorageInfo>,
    pub desktop: Option<String>,
    pub session_type: Option<String>,
    pub uptime_seconds: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct CpuInfo {
    pub model: String,
    pub logical_cpus: u32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub available_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct GpuInfo {
    /// DRM card name, e.g. `card0`.
    pub card: String,
    pub vendor_id: Option<String>,
    pub device_id: Option<String>,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub driver: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct StorageInfo {
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

/// Environment values that describe the caller's session. Passed explicitly
/// because a system daemon has no session environment of its own.
#[derive(Debug, Clone, Default)]
pub struct SessionEnv {
    pub current_desktop: Option<String>,
    pub session_type: Option<String>,
}

impl SessionEnv {
    pub fn from_process_env() -> Self {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Self {
            current_desktop: get("XDG_CURRENT_DESKTOP"),
            session_type: get("XDG_SESSION_TYPE"),
        }
    }
}

/// Reads system information relative to a root directory.
#[derive(Debug, Clone)]
pub struct Probe {
    pub root: PathBuf,
    /// Mount points to report storage for (relative to `root`).
    pub mount_points: Vec<String>,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/"),
            // On bootc/OSTree systems /var holds user data (/var/home).
            mount_points: vec!["/".into(), "/var".into()],
        }
    }
}

impl Probe {
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            ..Self::default()
        }
    }

    fn path(&self, abs: &str) -> PathBuf {
        self.root.join(abs.trim_start_matches('/'))
    }

    fn read(&self, abs: &str) -> Option<String> {
        fs::read_to_string(self.path(abs)).ok()
    }

    fn read_trimmed(&self, abs: &str) -> Option<String> {
        self.read(abs).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }

    pub fn collect(&self, product: &Product, session: &SessionEnv) -> SystemInfo {
        let build = BuildInfo::from_file(&self.path(crate::names::BUILD_INFO_FILE));
        let os = self.os_release();
        SystemInfo {
            schema: SYSINFO_SCHEMA,
            product_name: product.product.name.clone(),
            product_version: product.version.full(),
            build_id: build.as_ref().map(|b| b.build_id.clone()),
            variant: build.as_ref().map(|b| b.variant.clone()).filter(|v| !v.is_empty()),
            os_pretty_name: os.get("PRETTY_NAME").cloned(),
            base_os: os.get("MADOS_BASE_PRETTY_NAME").cloned(),
            kernel: self.read_trimmed("/proc/sys/kernel/osrelease"),
            architecture: std::env::consts::ARCH.to_string(),
            hostname: self.read_trimmed("/proc/sys/kernel/hostname"),
            cpu: self.cpu(),
            memory: self.memory(),
            gpus: self.gpus(),
            storage: self.storage(),
            desktop: session.current_desktop.clone(),
            session_type: session.session_type.clone(),
            uptime_seconds: self.uptime(),
        }
    }

    /// Parses `/etc/os-release`, falling back to `/usr/lib/os-release`.
    pub fn os_release(&self) -> HashMap<String, String> {
        let text = self
            .read("/etc/os-release")
            .or_else(|| self.read("/usr/lib/os-release"))
            .unwrap_or_default();
        parse_os_release(&text)
    }

    pub fn cpu(&self) -> Option<CpuInfo> {
        let text = self.read("/proc/cpuinfo")?;
        let mut model = None;
        let mut count = 0u32;
        for line in text.lines() {
            let Some((k, v)) = line.split_once(':') else { continue };
            match k.trim() {
                "processor" => count += 1,
                "model name" if model.is_none() => model = Some(v.trim().to_string()),
                _ => {}
            }
        }
        let model = model.filter(|m| !m.is_empty())?;
        Some(CpuInfo {
            model,
            logical_cpus: count.max(1),
        })
    }

    pub fn memory(&self) -> Option<MemoryInfo> {
        let text = self.read("/proc/meminfo")?;
        let mut total = None;
        let mut avail = None;
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let key = parts.next();
            let kib = parts.next().and_then(|v| v.parse::<u64>().ok());
            match key {
                Some("MemTotal:") => total = kib.map(|k| k * 1024),
                Some("MemAvailable:") => avail = kib.map(|k| k * 1024),
                _ => {}
            }
        }
        Some(MemoryInfo {
            total_bytes: total?,
            available_bytes: avail,
        })
    }

    pub fn uptime(&self) -> Option<u64> {
        let text = self.read("/proc/uptime")?;
        let secs: f64 = text.split_whitespace().next()?.parse().ok()?;
        Some(secs as u64)
    }

    pub fn gpus(&self) -> Vec<GpuInfo> {
        let drm = self.path("/sys/class/drm");
        let Ok(entries) = fs::read_dir(&drm) else {
            return Vec::new();
        };
        let mut cards: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            // "card0" yes, "card0-HDMI-A-1" (connector) no.
            .filter(|n| {
                n.strip_prefix("card")
                    .is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()))
            })
            .collect();
        cards.sort();
        let pci_ids = self.read("/usr/share/hwdata/pci.ids");
        cards
            .into_iter()
            .map(|card| {
                let dev = drm.join(&card).join("device");
                let read_id = |f: &str| {
                    fs::read_to_string(dev.join(f))
                        .ok()
                        .map(|s| s.trim().trim_start_matches("0x").to_lowercase())
                        .filter(|s| !s.is_empty())
                };
                let vendor_id = read_id("vendor");
                let device_id = read_id("device");
                let driver = fs::read_link(dev.join("driver"))
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .or_else(|| uevent_driver(&dev.join("uevent")));
                let (vendor, model) = match (&vendor_id, &device_id) {
                    (Some(v), Some(d)) => lookup_pci(pci_ids.as_deref(), v, d),
                    (Some(v), None) => (lookup_pci(pci_ids.as_deref(), v, "").0, None),
                    _ => (None, None),
                };
                GpuInfo {
                    card,
                    vendor: vendor.or_else(|| vendor_id.as_deref().and_then(known_vendor).map(String::from)),
                    vendor_id,
                    device_id,
                    model,
                    driver,
                }
            })
            .collect()
    }

    pub fn storage(&self) -> Vec<StorageInfo> {
        let mut seen = Vec::new();
        let mut out = Vec::new();
        for mp in &self.mount_points {
            let path = self.path(mp);
            let Some((fsid, total, avail)) = statvfs(&path) else {
                continue;
            };
            // Skip mount points on the same filesystem as one already listed.
            if seen.contains(&fsid) {
                continue;
            }
            seen.push(fsid);
            out.push(StorageInfo {
                mount_point: mp.clone(),
                total_bytes: total,
                available_bytes: avail,
            });
        }
        out
    }
}

/// Collects info for the running system.
pub fn collect(session: &SessionEnv) -> SystemInfo {
    Probe::default().collect(&Product::load(), session)
}

pub fn parse_os_release(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let (k, v) = line.split_once('=')?;
            let v = v.trim();
            let v = v
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .unwrap_or(v);
            Some((k.trim().to_string(), v.replace("\\\"", "\"")))
        })
        .collect()
}

fn uevent_driver(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()?
        .lines()
        .find_map(|l| l.strip_prefix("DRIVER=").map(str::to_string))
}

fn known_vendor(id: &str) -> Option<&'static str> {
    Some(match id {
        "8086" => "Intel",
        "1002" => "AMD",
        "10de" => "NVIDIA",
        "1af4" => "Red Hat (virtio)",
        "1234" => "QEMU",
        "15ad" => "VMware",
        "80ee" => "VirtualBox",
        _ => return None,
    })
}

/// Looks up vendor and device names in a pci.ids database.
fn lookup_pci(db: Option<&str>, vendor: &str, device: &str) -> (Option<String>, Option<String>) {
    let Some(db) = db else { return (None, None) };
    let mut vendor_name = None;
    for line in db.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if !line.starts_with('\t') {
            if vendor_name.is_some() {
                break; // left our vendor's block without finding the device
            }
            if line.len() > 4 && line[..4].eq_ignore_ascii_case(vendor) {
                vendor_name = Some(line[4..].trim().to_string());
            }
            if line.starts_with('C') && line.as_bytes().get(1) == Some(&b' ') {
                break; // device class section begins
            }
            continue;
        }
        if vendor_name.is_some() && !line.starts_with("\t\t") {
            let l = &line[1..];
            if !device.is_empty() && l.len() > 4 && l[..4].eq_ignore_ascii_case(device) {
                return (vendor_name, Some(l[4..].trim().to_string()));
            }
        }
    }
    (vendor_name, None)
}

/// Returns (filesystem id, total bytes, available bytes).
fn statvfs(path: &Path) -> Option<(u64, u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `st` is a valid out-pointer.
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut st) };
    if rc != 0 {
        return None;
    }
    let frsize = if st.f_frsize > 0 { st.f_frsize } else { st.f_bsize } as u64;
    Some((
        st.f_fsid as u64,
        st.f_blocks as u64 * frsize,
        st.f_bavail as u64 * frsize,
    ))
}

/// Formats a byte count for humans (binary units).
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_parsing() {
        let m = parse_os_release(
            "# comment\nNAME=\"TestOS\"\nID=fedora\nPRETTY_NAME='TestOS 0.1'\nBAD LINE\nVERSION_ID=44\n",
        );
        assert_eq!(m["NAME"], "TestOS");
        assert_eq!(m["ID"], "fedora");
        assert_eq!(m["PRETTY_NAME"], "TestOS 0.1");
        assert_eq!(m["VERSION_ID"], "44");
        assert!(!m.contains_key("BAD LINE"));
    }

    #[test]
    fn pci_lookup() {
        let db = "# x\n1af4  Red Hat, Inc.\n\t1050  Virtio 1.0 GPU\n\t\t1af4 1100  QEMU\n8086  Intel Corporation\n\t46a6  Alder Lake-P GT2\nC 00  Unclassified\n";
        assert_eq!(
            lookup_pci(Some(db), "8086", "46a6"),
            (Some("Intel Corporation".into()), Some("Alder Lake-P GT2".into()))
        );
        assert_eq!(
            lookup_pci(Some(db), "1af4", "1050"),
            (Some("Red Hat, Inc.".into()), Some("Virtio 1.0 GPU".into()))
        );
        assert_eq!(
            lookup_pci(Some(db), "1af4", "ffff"),
            (Some("Red Hat, Inc.".into()), None)
        );
        assert_eq!(lookup_pci(Some(db), "dead", "beef"), (None, None));
        assert_eq!(lookup_pci(None, "8086", "46a6"), (None, None));
    }

    #[test]
    fn bytes_formatting() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(16 * 1024 * 1024 * 1024), "16.0 GiB");
    }

    #[test]
    fn statvfs_on_tmp() {
        let (_, total, avail) = statvfs(Path::new("/tmp")).expect("statvfs /tmp");
        assert!(total > 0 && avail <= total);
    }
}
