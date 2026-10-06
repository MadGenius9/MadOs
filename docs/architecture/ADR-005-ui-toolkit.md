# ADR-005: GTK 4 + Rust for MadOS system applications

- **Status:** Accepted (for mados-settings in 0.1)
- **Date:** 2026-10-06

## Options

| Option | Notes |
|---|---|
| **GTK 4 via gtk4-rs (Rust)** | Native Wayland, no web runtime, MIT bindings, same language as services; widgets are themeable with CSS so MadOS can own the look; libadwaita deliberately not used (it imposes GNOME's design language) |
| Qt 6 / QML (C++ or cxx-qt) | Best fit with Plasma visually; Rust bindings less mature; C++ adds a second systems language |
| Tauri (Rust + web UI) | Allowed by project rules; adds WebKitGTK + a Node build to the image; heavier for a settings app |
| Slint | Promising Rust-native toolkit; licensing (GPL/royalty-free terms) needs review |
| Electron | Excluded by project rules |

## Decision

GTK 4 through gtk4-rs for `mados-settings` and future small system apps.
Blocking work (D-Bus, filesystem probing) runs on worker threads and results
return to the UI thread through `glib::spawn_future_local`.

## Consequences

- GTK 4 apps run fine under Plasma; visual integration with Breeze is
  imperfect until MadOS has its own shell and theme (M9).
- Revisit for the shell itself (M9), where compositor integration matters
  more than toolkit consistency.
