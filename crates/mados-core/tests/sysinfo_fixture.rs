//! Runs the system probe against a synthetic root filesystem.

use mados_core::sysinfo::{Probe, SessionEnv};
use mados_core::Product;
use std::fs;
use std::path::Path;

fn write(root: &Path, rel: &str, content: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, content).unwrap();
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    write(r, "proc/sys/kernel/osrelease", "6.19.4-200.fc44.x86_64\n");
    write(r, "proc/sys/kernel/hostname", "mados-vm\n");
    write(r, "proc/uptime", "1234.56 4000.00\n");
    write(
        r,
        "proc/cpuinfo",
        "processor\t: 0\nmodel name\t: AMD Ryzen 7 7840U w/ Radeon 780M Graphics\n\nprocessor\t: 1\nmodel name\t: AMD Ryzen 7 7840U w/ Radeon 780M Graphics\n",
    );
    write(
        r,
        "proc/meminfo",
        "MemTotal:       16303152 kB\nMemFree:  1 kB\nMemAvailable:    8000000 kB\n",
    );
    write(
        r,
        "etc/os-release",
        "NAME=\"TestOS\"\nID=fedora\nPRETTY_NAME=\"TestOS 0.1.0-dev\"\nMADOS_BASE_PRETTY_NAME=\"Fedora Linux 44 (Kinoite)\"\n",
    );
    write(
        r,
        "usr/lib/mados/build-info.json",
        r#"{"build_id":"20261006.1-abc1234","version":"0.1.0-dev","git_commit":"abc1234","build_time":"2026-10-06T00:00:00Z","base_image":"quay.io/fedora-ostree-desktops/kinoite:44","variant":"dev"}"#,
    );
    write(
        r,
        "usr/share/hwdata/pci.ids",
        "1af4  Red Hat, Inc.\n\t1050  Virtio 1.0 GPU\n",
    );
    // A GPU card plus a connector entry that must be ignored.
    write(r, "sys/class/drm/card0/device/vendor", "0x1af4\n");
    write(r, "sys/class/drm/card0/device/device", "0x1050\n");
    write(r, "sys/class/drm/card0/device/uevent", "DRIVER=virtio-pci\n");
    write(r, "sys/class/drm/card0-Virtual-1/status", "connected\n");
    fs::create_dir_all(r.join("var")).unwrap();
    dir
}

#[test]
fn probe_reads_fixture_tree() {
    let dir = fixture();
    let info = Probe::with_root(dir.path()).collect(
        &Product::embedded(),
        &SessionEnv {
            current_desktop: Some("KDE".into()),
            session_type: Some("wayland".into()),
        },
    );
    assert_eq!(info.kernel.as_deref(), Some("6.19.4-200.fc44.x86_64"));
    assert_eq!(info.hostname.as_deref(), Some("mados-vm"));
    assert_eq!(info.uptime_seconds, Some(1234));
    let cpu = info.cpu.unwrap();
    assert_eq!(cpu.logical_cpus, 2);
    assert!(cpu.model.starts_with("AMD Ryzen"));
    let mem = info.memory.unwrap();
    assert_eq!(mem.total_bytes, 16303152 * 1024);
    assert_eq!(mem.available_bytes, Some(8000000 * 1024));
    assert_eq!(info.build_id.as_deref(), Some("20261006.1-abc1234"));
    assert_eq!(info.variant.as_deref(), Some("dev"));
    assert_eq!(info.base_os.as_deref(), Some("Fedora Linux 44 (Kinoite)"));
    assert_eq!(info.gpus.len(), 1, "connector entries must be skipped");
    let gpu = &info.gpus[0];
    assert_eq!(gpu.vendor.as_deref(), Some("Red Hat, Inc."));
    assert_eq!(gpu.model.as_deref(), Some("Virtio 1.0 GPU"));
    assert_eq!(gpu.driver.as_deref(), Some("virtio-pci"));
    // "/" and "/var" are the same filesystem inside the tempdir: listed once.
    assert_eq!(info.storage.len(), 1);
    assert_eq!(info.desktop.as_deref(), Some("KDE"));
}

#[test]
fn probe_handles_empty_root() {
    let dir = tempfile::tempdir().unwrap();
    let info = Probe::with_root(dir.path()).collect(&Product::embedded(), &SessionEnv::default());
    assert_eq!(info.kernel, None);
    assert_eq!(info.cpu, None);
    assert_eq!(info.memory, None);
    assert!(info.gpus.is_empty());
    assert_eq!(info.build_id, None);
    // Product metadata is always available (embedded).
    assert_eq!(info.product_version, Product::embedded().version.full());
}

#[test]
fn probe_reads_host_without_panicking() {
    let info = mados_core::sysinfo::collect(&SessionEnv::from_process_env());
    assert!(!info.architecture.is_empty());
    // JSON round trip is the D-Bus wire format.
    let json = serde_json::to_string(&info).unwrap();
    let back: mados_core::SystemInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(back, info);
}
