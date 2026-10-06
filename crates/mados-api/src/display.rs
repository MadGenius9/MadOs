//! Displays: connected outputs (from sysfs DRM) and built-in backlight.
//!
//! Brightness changes go through systemd-logind's `Session.SetBrightness`,
//! which lets the user of the active session adjust the backlight without
//! privileges. Mode and arrangement changes belong to the compositor (KWin
//! in 0.1) and are not implemented here.
//!
//! Filesystem reads take a `root` so tests can use a fixture tree.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use zbus::zvariant::OwnedObjectPath;
use zbus::Connection;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Output {
    /// Connector name, e.g. `eDP-1`, `HDMI-A-1`, `Virtual-1`.
    pub name: String,
    pub connected: bool,
    /// First (preferred) mode the connector advertises, e.g. `1920x1080`.
    pub preferred_mode: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Backlight {
    /// sysfs device name, e.g. `intel_backlight`.
    pub name: String,
    pub max: u32,
    pub current: u32,
}

impl Backlight {
    pub fn percent(&self) -> u8 {
        if self.max == 0 {
            return 0;
        }
        ((u64::from(self.current) * 100 + u64::from(self.max) / 2) / u64::from(self.max)).min(100) as u8
    }

    /// Raw value for a percentage. Never 0 for percent > 0, so a low setting
    /// cannot turn the panel fully dark.
    pub fn raw_for(&self, percent: u8) -> u32 {
        let raw = (u64::from(percent.min(100)) * u64::from(self.max) / 100) as u32;
        if percent > 0 {
            raw.max(1)
        } else {
            0
        }
    }
}

fn read_trim(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Connectors of all DRM cards (`/sys/class/drm/cardN-<connector>`), sorted.
pub fn outputs(root: &Path) -> Vec<Output> {
    let drm = root.join("sys/class/drm");
    let Ok(entries) = fs::read_dir(&drm) else {
        return Vec::new();
    };
    let mut outs: Vec<Output> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            // "card0-HDMI-A-1" -> "HDMI-A-1"; skip "card0", "renderD128", "version".
            let rest = file.strip_prefix("card")?;
            let (num, connector) = rest.split_once('-')?;
            if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let dir = drm.join(&file);
            let connected = read_trim(&dir.join("status")).as_deref() == Some("connected");
            let preferred_mode = read_trim(&dir.join("modes"))
                .and_then(|m| m.lines().next().map(str::to_string))
                .filter(|m| !m.is_empty());
            Some(Output {
                name: connector.to_string(),
                connected,
                preferred_mode,
            })
        })
        .collect();
    outs.sort_by(|a, b| (!a.connected, &a.name).cmp(&(!b.connected, &b.name)));
    outs
}

/// The first backlight device with a usable range, if any.
pub fn backlight(root: &Path) -> Option<Backlight> {
    let dir = root.join("sys/class/backlight");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names.into_iter().find_map(|name| {
        let max: u32 = read_trim(&dir.join(&name).join("max_brightness"))?.parse().ok()?;
        let current: u32 = read_trim(&dir.join(&name).join("brightness"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        (max > 0).then_some(Backlight { name, max, current })
    })
}

/// Sets the backlight to `percent` via logind on the caller's graphical
/// session. Returns `Ok(false)` when there is no backlight.
pub async fn set_brightness(conn: &Connection, root: &Path, percent: u8) -> zbus::Result<bool> {
    let Some(bl) = backlight(root) else {
        return Ok(false);
    };
    let user = zbus::Proxy::new(
        conn,
        "org.freedesktop.login1",
        "/org/freedesktop/login1/user/self",
        "org.freedesktop.login1.User",
    )
    .await?;
    let (_id, session): (String, OwnedObjectPath) = user.get_property("Display").await?;
    let session = zbus::Proxy::new(
        conn,
        "org.freedesktop.login1",
        session,
        "org.freedesktop.login1.Session",
    )
    .await?;
    session
        .call_method("SetBrightness", &("backlight", bl.name.as_str(), bl.raw_for(percent)))
        .await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(root: &Path, rel: &str, s: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    #[test]
    fn outputs_from_sysfs() {
        let d = tempfile::tempdir().unwrap();
        assert!(outputs(d.path()).is_empty());
        w(d.path(), "sys/class/drm/card0/dev", "226:0\n");
        w(d.path(), "sys/class/drm/version", "drm 1.1.0\n");
        w(d.path(), "sys/class/drm/card0-eDP-1/status", "connected\n");
        w(d.path(), "sys/class/drm/card0-eDP-1/modes", "2880x1800\n1920x1200\n");
        w(d.path(), "sys/class/drm/card0-HDMI-A-1/status", "disconnected\n");
        w(d.path(), "sys/class/drm/card0-HDMI-A-1/modes", "");
        let o = outputs(d.path());
        assert_eq!(o.len(), 2);
        assert_eq!(
            o[0],
            Output {
                name: "eDP-1".into(),
                connected: true,
                preferred_mode: Some("2880x1800".into())
            }
        );
        assert_eq!(
            o[1],
            Output {
                name: "HDMI-A-1".into(),
                connected: false,
                preferred_mode: None
            }
        );
    }

    #[test]
    fn backlight_and_percent() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(backlight(d.path()), None);
        w(d.path(), "sys/class/backlight/acpi_video0/max_brightness", "0\n");
        assert_eq!(backlight(d.path()), None, "zero range is unusable");
        w(
            d.path(),
            "sys/class/backlight/intel_backlight/max_brightness",
            "96000\n",
        );
        w(d.path(), "sys/class/backlight/intel_backlight/brightness", "48000\n");
        let bl = backlight(d.path()).unwrap();
        assert_eq!(
            (bl.name.as_str(), bl.max, bl.current, bl.percent()),
            ("intel_backlight", 96000, 48000, 50)
        );
        assert_eq!(bl.raw_for(100), 96000);
        assert_eq!(bl.raw_for(0), 0);
        assert_eq!(bl.raw_for(250), 96000);
        let tiny = Backlight {
            name: "x".into(),
            max: 7,
            current: 0,
        };
        assert_eq!(tiny.raw_for(1), 1, "1% must not be fully dark");
    }
}
