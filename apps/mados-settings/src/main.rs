//! MadOS Settings.
//!
//! A native GTK 4 application. It reads unprivileged information directly
//! (via mados-core) and performs system changes only through the MadOS system
//! API on D-Bus (mados-daemon, mados-ai). It never runs privileged commands.

mod bg;
mod pages;
mod widgets;

use gtk::prelude::*;
use gtk::{gdk, glib};
use mados_core::Product;
use std::path::PathBuf;

pub const APP_ID: &str = "org.mados.Settings";
pub const KDE_SETTINGS: &str = "/usr/bin/systemsettings";

pub fn kde_settings_available() -> bool {
    std::path::Path::new(KDE_SETTINGS).exists()
}

/// Installed logo, or the repository copy when running from a checkout.
pub fn logo_path() -> Option<PathBuf> {
    let installed = PathBuf::from("/usr/share/mados/logo.svg");
    if installed.exists() {
        return Some(installed);
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../product/assets/logo.svg");
    dev.exists().then_some(dev)
}

fn css(product: &Product) -> String {
    format!(
        r#"
        .page {{ padding: 24px 32px; }}
        .card {{ padding: 16px; border-radius: 12px; background-color: alpha(currentColor, 0.05); }}
        .title-1 {{ font-size: 1.6em; font-weight: 700; }}
        .heading {{ font-weight: 700; }}
        .dim-label {{ opacity: 0.7; }}
        stacksidebar row:selected {{ background-color: {accent}; color: white; }}
        button.suggested-action {{ background: {accent}; color: white; }}
        button.suggested-action:disabled {{ opacity: 0.45; }}
        levelbar block.filled {{ background-color: {accent}; }}
        "#,
        accent = product.branding.accent
    )
}

fn build(app: &gtk::Application) {
    let product = Product::load();
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&css(&product));
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .hexpand(true)
        .build();
    let add = |name: &str, title: &str, w: gtk::Widget| {
        stack.add_titled(&w, Some(name), title);
    };
    // Order follows docs/architecture/overview.md (Settings categories).
    add("about", "About", pages::about(&product));
    add("network", "Network & Wi-Fi", pages::network());
    add("bluetooth", "Bluetooth", pages::bluetooth());
    add("display", "Display", pages::display());
    add("sound", "Sound", pages::sound());
    add("power", "Power", pages::power());
    add("storage", "Storage", pages::storage());
    add("users", "Users", pages::users());
    add("apps", "Applications", pages::applications());
    add("updates", "Updates", pages::updates());
    add("assistant", "Assistant", pages::assistant());
    add("privacy", "Privacy", pages::privacy());

    let sidebar = gtk::StackSidebar::builder().stack(&stack).width_request(200).build();
    let layout = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    layout.append(&sidebar);
    layout.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    layout.append(&stack);

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(format!("{} Settings", product.product.name))
        .default_width(960)
        .default_height(640)
        .child(&layout)
        .build();
    if let Some(page) = initial_page() {
        stack.set_visible_child_name(&page);
    }
    window.present();
}

/// `--page=NAME` (e.g. from the About launcher) or `MADOS_SETTINGS_PAGE`.
fn initial_page() -> Option<String> {
    std::env::args()
        .find_map(|a| a.strip_prefix("--page=").map(str::to_string))
        .or_else(|| std::env::var("MADOS_SETTINGS_PAGE").ok())
}

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build);
    // Our own flags are handled above; GTK sees only the program name.
    app.run_with_args(&std::env::args().take(1).collect::<Vec<_>>())
}
