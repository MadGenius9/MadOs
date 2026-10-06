//! First-run welcome window.
//!
//! Started at login by `/etc/xdg/autostart/org.mados.FirstRun.desktop` with
//! `--autostart`; in that mode it exits immediately once the user has seen
//! it (marker file in the user's config directory). Without `--autostart`
//! it always shows (e.g. from the application menu).

use gtk::prelude::*;
use gtk::{gdk, glib};
use mados_core::Product;
use std::path::{Path, PathBuf};

const APP_ID: &str = "org.mados.FirstRun";
const MARKER: &str = "mados/first-run-done";
const SETTINGS: &str = "/usr/bin/mados-settings";

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
}

/// Whether the autostarted window should be shown for this user.
pub fn should_show(config: Option<&Path>) -> bool {
    match config {
        Some(c) => !c.join(MARKER).exists(),
        None => false,
    }
}

/// Records that the user has seen the welcome window.
pub fn mark_done(config: &Path) -> std::io::Result<()> {
    let path = config.join(MARKER);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, b"")
}

fn logo_path() -> Option<PathBuf> {
    [
        PathBuf::from("/usr/share/mados/logo.svg"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../product/assets/logo.svg"),
    ]
    .into_iter()
    .find(|p| p.exists())
}

/// Privacy line from the running assistant (never assumed).
fn assistant_privacy() -> String {
    let r: zbus::Result<(String, bool)> = (|| {
        let conn = zbus::blocking::Connection::session()?;
        let p = mados_api::AssistantProxyBlocking::new(&conn)?;
        Ok((p.provider()?, p.local()?))
    })();
    match r {
        Ok((provider, true)) => format!("The assistant ({provider}) works on this device; nothing you ask leaves it."),
        Ok((provider, false)) => {
            format!("The assistant ({provider}) uses an online service; requests leave this device.")
        }
        Err(_) => "The assistant is not running.".into(),
    }
}

fn build(app: &gtk::Application) {
    let product = Product::load();
    let css = gtk::CssProvider::new();
    css.load_from_string(&format!(
        ".welcome {{ padding: 32px; }} .title-1 {{ font-size: 1.8em; font-weight: 700; }} .dim-label {{ opacity: 0.7; }}
         button.suggested-action {{ background: {}; color: white; }}",
        product.branding.accent
    ));
    if let Some(d) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&d, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .css_classes(["welcome"])
        .build();
    let logo = gtk::Image::builder().pixel_size(96).halign(gtk::Align::Start).build();
    if let Some(p) = logo_path() {
        logo.set_from_file(Some(p));
    }
    content.append(&logo);
    content.append(
        &gtk::Label::builder()
            .label(format!("Welcome to {}", product.product.name))
            .xalign(0.0)
            .css_classes(["title-1"])
            .build(),
    );
    content.append(
        &gtk::Label::builder()
            .label(format!(
                "Version {} · {}",
                product.version.full(),
                product.product.tagline
            ))
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build(),
    );
    let points = [
        "This is an early development build. Expect missing features.".to_string(),
        format!("{} Settings shows your system and controls Wi-Fi, Bluetooth, sound, display, power and updates.", product.product.name),
        "Updates install next to the running system and only take effect after a restart; the previous version is kept so you can roll back.".to_string(),
        assistant_privacy(),
    ];
    for p in points {
        content.append(
            &gtk::Label::builder()
                .label(format!("•  {p}"))
                .xalign(0.0)
                .wrap(true)
                .max_width_chars(60)
                .build(),
        );
    }
    let buttons = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    let settings = gtk::Button::with_label("Open Settings");
    settings.set_sensitive(Path::new(SETTINGS).exists());
    let close = gtk::Button::builder()
        .label("Get Started")
        .css_classes(["suggested-action"])
        .build();
    buttons.append(&settings);
    buttons.append(&close);
    content.append(&buttons);

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(format!("Welcome to {}", product.product.name))
        .default_width(620)
        .child(&content)
        .build();
    // Seen once is enough: record it as soon as the window is shown.
    if let Some(c) = config_dir() {
        let _ = mark_done(&c);
    }
    settings.connect_clicked(|_| {
        let _ = std::process::Command::new(SETTINGS).spawn();
    });
    let w = window.clone();
    close.connect_clicked(move |_| w.close());
    window.present();
}

fn main() -> glib::ExitCode {
    let autostart = std::env::args().any(|a| a == "--autostart");
    if autostart && !should_show(config_dir().as_deref()) {
        return glib::ExitCode::SUCCESS;
    }
    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build);
    app.run_with_args(&std::env::args().take(1).collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shown_once_per_user() {
        let d = tempfile::tempdir().unwrap();
        assert!(should_show(Some(d.path())));
        mark_done(d.path()).unwrap();
        assert!(!should_show(Some(d.path())));
        assert!(!should_show(None), "no config dir: do not pop up");
    }
}
