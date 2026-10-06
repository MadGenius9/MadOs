//! Small shared widget helpers.

use gtk::prelude::*;

pub fn page(title: &str, subtitle: Option<&str>) -> (gtk::ScrolledWindow, gtk::Box) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.add_css_class("page");
    let heading = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .css_classes(["title-1"])
        .build();
    content.append(&heading);
    if let Some(s) = subtitle {
        content.append(
            &gtk::Label::builder()
                .label(s)
                .xalign(0.0)
                .wrap(true)
                .css_classes(["dim-label"])
                .build(),
        );
    }
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&content)
        .vexpand(true)
        .build();
    (scroller, content)
}

/// A two-column key/value grid.
pub struct InfoGrid {
    pub grid: gtk::Grid,
    rows: i32,
}

impl InfoGrid {
    pub fn new() -> Self {
        let grid = gtk::Grid::builder()
            .column_spacing(24)
            .row_spacing(8)
            .css_classes(["card"])
            .build();
        Self { grid, rows: 0 }
    }

    pub fn clear(&mut self) {
        while let Some(child) = self.grid.first_child() {
            self.grid.remove(&child);
        }
        self.rows = 0;
    }

    pub fn row(&mut self, key: &str, value: &str) {
        let k = gtk::Label::builder()
            .label(key)
            .xalign(0.0)
            .yalign(0.0)
            .css_classes(["dim-label"])
            .build();
        let v = gtk::Label::builder()
            .label(value)
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .hexpand(true)
            .build();
        self.grid.attach(&k, 0, self.rows, 1, 1);
        self.grid.attach(&v, 1, self.rows, 1, 1);
        self.rows += 1;
    }
}

pub fn status_label() -> gtk::Label {
    gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build()
}

/// Asks a yes/no question; calls `on_yes` only if confirmed.
pub fn confirm(parent: &impl IsA<gtk::Widget>, message: &str, detail: &str, action: &str, on_yes: impl Fn() + 'static) {
    let dialog = gtk::AlertDialog::builder()
        .message(message)
        .detail(detail)
        .buttons(["Cancel", action])
        .cancel_button(0)
        .default_button(0)
        .modal(true)
        .build();
    let window = parent.root().and_downcast::<gtk::Window>();
    dialog.choose(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
        if res == Ok(1) {
            on_yes();
        }
    });
}
