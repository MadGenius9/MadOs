//! Installed applications, from freedesktop desktop entries.
//!
//! Sources: the system (`/usr/share/applications`, part of the OS image),
//! system-wide Flatpaks (`/var/lib/flatpak/exports/share/applications`) and
//! per-user Flatpaks (`~/.local/share/flatpak/exports/share/applications`).
//! Entries with `NoDisplay=true`, `Hidden=true` or a non-Application type are
//! not apps a user launches and are skipped.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppSource {
    /// Part of the OS image.
    System,
    /// Flatpak installed for all users.
    FlatpakSystem,
    /// Flatpak installed for the current user.
    FlatpakUser,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct AppEntry {
    /// Desktop file id without `.desktop`, e.g. `org.mozilla.firefox`.
    pub id: String,
    pub name: String,
    pub source: AppSource,
}

/// Parses the `[Desktop Entry]` group; returns the display name if this is a
/// visible application.
pub fn parse_desktop_entry(text: &str) -> Option<String> {
    let mut in_entry = false;
    let mut name = None;
    let mut is_app = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        match (k.trim(), v.trim()) {
            ("Type", t) => is_app = t == "Application",
            ("NoDisplay", "true") | ("Hidden", "true") => return None,
            ("Name", n) if name.is_none() => name = Some(n.to_string()),
            _ => {}
        }
    }
    if is_app {
        name.filter(|n| !n.is_empty())
    } else {
        None
    }
}

fn scan(dir: &Path, source: AppSource, out: &mut Vec<AppEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.filter_map(|e| e.ok()) {
        let path = e.path();
        let Some(id) = path
            .file_name()
            .and_then(|f| f.to_str())
            .and_then(|f| f.strip_suffix(".desktop"))
        else {
            continue;
        };
        if let Some(name) = std::fs::read_to_string(&path)
            .ok()
            .as_deref()
            .and_then(parse_desktop_entry)
        {
            out.push(AppEntry {
                id: id.to_string(),
                name,
                source,
            });
        }
    }
}

/// Installed apps under `root` (normally `/`) and the user's `home`.
/// A Flatpak overrides a system entry with the same id; user overrides system.
pub fn installed(root: &Path, home: Option<&Path>) -> Vec<AppEntry> {
    let mut all = Vec::new();
    scan(&root.join("usr/share/applications"), AppSource::System, &mut all);
    scan(
        &root.join("var/lib/flatpak/exports/share/applications"),
        AppSource::FlatpakSystem,
        &mut all,
    );
    if let Some(h) = home {
        scan(
            &h.join(".local/share/flatpak/exports/share/applications"),
            AppSource::FlatpakUser,
            &mut all,
        );
    }
    // Later sources win for the same id.
    let mut by_id: std::collections::BTreeMap<String, AppEntry> = Default::default();
    for app in all {
        by_id.insert(app.id.clone(), app);
    }
    let mut apps: Vec<AppEntry> = by_id.into_values().collect();
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    apps
}

/// The running user's home directory, if known.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_parsing() {
        assert_eq!(
            parse_desktop_entry("[Desktop Entry]\nType=Application\nName=Files\nName[de]=Dateien\n"),
            Some("Files".into())
        );
        assert_eq!(
            parse_desktop_entry("[Desktop Entry]\nType=Application\nName=X\nNoDisplay=true\n"),
            None
        );
        assert_eq!(parse_desktop_entry("[Desktop Entry]\nType=Link\nName=Web\n"), None);
        assert_eq!(
            parse_desktop_entry(
                "[Desktop Action new]\nName=New Window\n[Desktop Entry]\nName=Browser\nType=Application\n"
            ),
            Some("Browser".into()),
            "names in other groups are ignored"
        );
    }

    #[test]
    fn sources_and_overrides() {
        let d = tempfile::tempdir().unwrap();
        let w = |rel: &str, body: &str| {
            let p = d.path().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        };
        let app = |name: &str| format!("[Desktop Entry]\nType=Application\nName={name}\n");
        w("root/usr/share/applications/org.kde.konsole.desktop", &app("Konsole"));
        w(
            "root/usr/share/applications/org.mozilla.firefox.desktop",
            &app("Firefox (system)"),
        );
        w(
            "root/usr/share/applications/hidden.desktop",
            "[Desktop Entry]\nType=Application\nName=H\nNoDisplay=true\n",
        );
        w(
            "root/var/lib/flatpak/exports/share/applications/org.mozilla.firefox.desktop",
            &app("Firefox"),
        );
        w(
            "home/.local/share/flatpak/exports/share/applications/com.example.Notes.desktop",
            &app("notes"),
        );
        let apps = installed(&d.path().join("root"), Some(&d.path().join("home")));
        let got: Vec<(&str, AppSource)> = apps.iter().map(|a| (a.name.as_str(), a.source)).collect();
        assert_eq!(
            got,
            [
                ("Firefox", AppSource::FlatpakSystem),
                ("Konsole", AppSource::System),
                ("notes", AppSource::FlatpakUser)
            ]
        );
        assert!(installed(Path::new("/nonexistent"), None).is_empty());
    }
}
