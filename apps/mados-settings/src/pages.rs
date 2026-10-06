//! Settings pages. Each page shows real data or performs a real action through
//! the MadOS system API; anything not implemented says so explicitly.

use crate::bg;
use crate::widgets::{self, InfoGrid};
use gtk::prelude::*;
use mados_api::{AssistantProxyBlocking, AssistantReply, ReplyStatus, SystemProxyBlocking, UpdateStatus};
use mados_core::sysinfo::{self, format_bytes, SessionEnv};
use mados_core::{Product, SystemInfo};
use std::cell::RefCell;
use std::rc::Rc;

fn na() -> String {
    "Unavailable".into()
}

// ---------------------------------------------------------------- About

pub fn about(product: &Product) -> gtk::Widget {
    let (root, content) = widgets::page("About", None);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    let logo = gtk::Image::builder()
        .pixel_size(72)
        .icon_name("org.mados.Settings")
        .build();
    if let Some(path) = crate::logo_path() {
        logo.set_from_file(Some(path));
    }
    header.append(&logo);
    let names = gtk::Box::new(gtk::Orientation::Vertical, 4);
    names.append(
        &gtk::Label::builder()
            .label(&product.product.full_name)
            .xalign(0.0)
            .css_classes(["title-1"])
            .build(),
    );
    names.append(
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
    header.append(&names);
    content.append(&header);

    let grid = Rc::new(RefCell::new(InfoGrid::new()));
    content.append(&grid.borrow().grid);
    let refresh = gtk::Button::builder()
        .label("Refresh")
        .halign(gtk::Align::Start)
        .build();
    content.append(&refresh);

    let load = {
        let grid = grid.clone();
        move || {
            let grid = grid.clone();
            bg::run(
                || sysinfo::collect(&SessionEnv::from_process_env()),
                move |info| fill_about(&mut grid.borrow_mut(), &info),
            );
        }
    };
    load();
    refresh.connect_clicked(move |_| load());
    root.upcast()
}

fn fill_about(g: &mut InfoGrid, i: &SystemInfo) {
    g.clear();
    g.row("Version", &format!("{} {}", i.product_name, i.product_version));
    g.row("Build ID", i.build_id.as_deref().unwrap_or("Not a MadOS image build"));
    if let Some(v) = &i.variant {
        g.row("Image variant", v);
    }
    g.row(
        "Base system",
        &i.base_os
            .clone()
            .or_else(|| i.os_pretty_name.clone())
            .unwrap_or_else(na),
    );
    g.row("Linux kernel", &i.kernel.clone().unwrap_or_else(na));
    g.row("Architecture", &i.architecture);
    g.row("Hostname", &i.hostname.clone().unwrap_or_else(na));
    g.row(
        "Processor",
        &i.cpu
            .as_ref()
            .map(|c| format!("{} ({} threads)", c.model, c.logical_cpus))
            .unwrap_or_else(na),
    );
    g.row(
        "Memory",
        &i.memory
            .as_ref()
            .map(|m| format_bytes(m.total_bytes))
            .unwrap_or_else(na),
    );
    if i.gpus.is_empty() {
        g.row("Graphics", &na());
    }
    for gpu in &i.gpus {
        let name: Vec<&str> = [gpu.vendor.as_deref(), gpu.model.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        let mut s = if name.is_empty() {
            gpu.card.clone()
        } else {
            name.join(" ")
        };
        if let Some(d) = &gpu.driver {
            s.push_str(&format!(" (driver: {d})"));
        }
        g.row("Graphics", &s);
    }
    let storage: Vec<String> = i
        .storage
        .iter()
        .map(|s| {
            format!(
                "{} free of {} ({})",
                format_bytes(s.available_bytes),
                format_bytes(s.total_bytes),
                s.mount_point
            )
        })
        .collect();
    g.row("Storage", &if storage.is_empty() { na() } else { storage.join("\n") });
    let session = match (&i.desktop, &i.session_type) {
        (Some(d), Some(t)) => format!("{d} on {t}"),
        (Some(d), None) => d.clone(),
        (None, Some(t)) => t.clone(),
        (None, None) => na(),
    };
    g.row("Desktop session", &session);
}

// ---------------------------------------------------------------- Storage

pub fn storage() -> gtk::Widget {
    let (root, content) = widgets::page("Storage", Some("Space on the system and data filesystems."));
    let list = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.append(&list);
    let l = list.clone();
    bg::run(
        || sysinfo::collect(&SessionEnv::default()).storage,
        move |entries| {
            if entries.is_empty() {
                l.append(&gtk::Label::new(Some("Storage information is unavailable.")));
            }
            for s in entries {
                let used = s.total_bytes.saturating_sub(s.available_bytes);
                let card = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(6)
                    .css_classes(["card"])
                    .build();
                card.append(
                    &gtk::Label::builder()
                        .label(&s.mount_point)
                        .xalign(0.0)
                        .css_classes(["heading"])
                        .build(),
                );
                let bar = gtk::LevelBar::builder().min_value(0.0).max_value(1.0).build();
                if s.total_bytes > 0 {
                    bar.set_value(used as f64 / s.total_bytes as f64);
                }
                card.append(&bar);
                card.append(
                    &gtk::Label::builder()
                        .label(format!(
                            "{} used · {} free · {} total",
                            format_bytes(used),
                            format_bytes(s.available_bytes),
                            format_bytes(s.total_bytes)
                        ))
                        .xalign(0.0)
                        .css_classes(["dim-label"])
                        .build(),
                );
                l.append(&card);
            }
        },
    );
    root.upcast()
}

// ---------------------------------------------------------------- Power

pub fn power() -> gtk::Widget {
    let (root, content) = widgets::page(
        "Power",
        Some("Restart and shut down go through the MadOS system service and are authorized by polkit."),
    );
    let status = widgets::status_label();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let restart = gtk::Button::with_label("Restart…");
    let shutdown = gtk::Button::builder()
        .label("Shut Down…")
        .css_classes(["destructive-action"])
        .build();
    row.append(&restart);
    row.append(&shutdown);
    content.append(&row);
    content.append(&status);
    content.append(
        &gtk::Label::builder()
            .label("Battery, suspend and power-profile settings: not yet implemented in MadOS Settings.")
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build(),
    );

    let act = |reboot: bool, status: gtk::Label| {
        move |btn: &gtk::Button| {
            let status = status.clone();
            let (msg, verb) = if reboot {
                ("Restart now?", "Restart")
            } else {
                ("Shut down now?", "Shut Down")
            };
            widgets::confirm(
                btn,
                msg,
                "Unsaved work in open applications may be lost.",
                verb,
                move || {
                    let status = status.clone();
                    status.set_label("Requesting…");
                    bg::run(
                        move || -> zbus::Result<()> {
                            let p = SystemProxyBlocking::new(&bg::system_bus()?)?;
                            if reboot {
                                p.reboot()
                            } else {
                                p.power_off()
                            }
                        },
                        move |r| match r {
                            Ok(()) => status.set_label("Request accepted."),
                            Err(e) => status.set_label(&bg::describe(&e)),
                        },
                    );
                },
            );
        }
    };
    restart.connect_clicked(act(true, status.clone()));
    shutdown.connect_clicked(act(false, status));
    root.upcast()
}

// ---------------------------------------------------------------- Updates

pub fn updates() -> gtk::Widget {
    let (root, content) = widgets::page(
        "Updates",
        Some("MadOS is image-based: updates install as a new deployment and the previous one is kept for rollback."),
    );
    let grid = Rc::new(RefCell::new(InfoGrid::new()));
    content.append(&grid.borrow().grid);
    let status = widgets::status_label();
    content.append(&status);
    content.append(
        &gtk::Label::builder()
            .label("Installing updates from Settings is not yet implemented. In this development build, run \u{201c}sudo bootc upgrade\u{201d} in a terminal, then restart. \u{201c}sudo bootc rollback\u{201d} returns to the previous deployment.")
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build(),
    );
    let refresh = gtk::Button::builder()
        .label("Refresh")
        .halign(gtk::Align::Start)
        .build();
    content.append(&refresh);

    let load = move || {
        let grid = grid.clone();
        let status = status.clone();
        status.set_label("Reading deployment status…");
        bg::run(
            || -> zbus::Result<UpdateStatus> {
                let json = SystemProxyBlocking::new(&bg::system_bus()?)?.get_update_status()?;
                serde_json::from_str(&json).map_err(|e| zbus::Error::Failure(e.to_string()))
            },
            move |r| {
                let mut g = grid.borrow_mut();
                g.clear();
                match r {
                    Ok(s) if s.available => {
                        status.set_label("");
                        for (label, d) in [
                            ("Running", &s.booted),
                            ("Pending", &s.staged),
                            ("Rollback", &s.rollback),
                        ] {
                            let text = match d {
                                Some(d) => format!(
                                    "{}\n{}{}",
                                    d.image.as_deref().unwrap_or("unknown image"),
                                    d.version
                                        .as_deref()
                                        .map(|v| format!("version {v} "))
                                        .unwrap_or_default(),
                                    d.timestamp.as_deref().unwrap_or("")
                                ),
                                None => "None".into(),
                            };
                            g.row(label, &text);
                        }
                    }
                    Ok(s) => status.set_label(&format!("Update status unavailable: {}", s.message.unwrap_or_default())),
                    Err(e) => status.set_label(&format!("Update status unavailable: {}", bg::describe(&e))),
                }
            },
        );
    };
    load();
    refresh.connect_clicked(move |_| load());
    root.upcast()
}

// ---------------------------------------------------------------- Assistant

pub fn assistant() -> gtk::Widget {
    let (root, content) = widgets::page(
        "Assistant",
        Some("Development preview. Requests are matched to a fixed set of permission-checked actions; the assistant cannot run commands. Changes always ask for confirmation."),
    );
    let entry = gtk::Entry::builder()
        .placeholder_text("Try: How much battery is left?")
        .hexpand(true)
        .build();
    let ask = gtk::Button::with_label("Ask");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&entry);
    row.append(&ask);
    content.append(&row);
    let answer = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .css_classes(["card"])
        .visible(false)
        .build();
    content.append(&answer);
    let confirm_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .visible(false)
        .build();
    let yes = gtk::Button::builder()
        .label("Confirm")
        .css_classes(["suggested-action"])
        .build();
    let no = gtk::Button::with_label("Cancel");
    confirm_row.append(&yes);
    confirm_row.append(&no);
    content.append(&confirm_row);

    let pending: Rc<RefCell<Option<String>>> = Rc::default();

    let show = {
        let answer = answer.clone();
        let confirm_row = confirm_row.clone();
        let pending = pending.clone();
        move |r: zbus::Result<AssistantReply>| {
            answer.set_visible(true);
            match r {
                Ok(reply) => {
                    answer.set_label(&reply.message);
                    let needs = reply.status == ReplyStatus::NeedsConfirmation;
                    confirm_row.set_visible(needs);
                    *pending.borrow_mut() = if needs { reply.request_id } else { None };
                }
                Err(e) => {
                    confirm_row.set_visible(false);
                    answer.set_label(&format!("Assistant unavailable: {}", bg::describe(&e)));
                }
            }
        }
    };

    let submit = {
        let entry = entry.clone();
        let show = show.clone();
        move || {
            let text = entry.text().to_string();
            if text.trim().is_empty() {
                return;
            }
            let show = show.clone();
            bg::run(move || assistant_call(|p| p.ask(&text)), show);
        }
    };
    {
        let submit = submit.clone();
        entry.connect_activate(move |_| submit());
    }
    ask.connect_clicked(move |_| submit());
    {
        let pending = pending.clone();
        let show = show.clone();
        yes.connect_clicked(move |_| {
            if let Some(id) = pending.borrow_mut().take() {
                let show = show.clone();
                bg::run(move || assistant_call(|p| p.confirm(&id)), show);
            }
        });
    }
    no.connect_clicked(move |_| {
        if let Some(id) = pending.borrow_mut().take() {
            bg::run(
                move || -> zbus::Result<()> { AssistantProxyBlocking::new(&bg::session_bus()?)?.cancel(&id) },
                |_| {},
            );
        }
        confirm_row.set_visible(false);
        answer.set_label("Cancelled.");
    });
    root.upcast()
}

fn assistant_call(
    f: impl FnOnce(&AssistantProxyBlocking<'static>) -> zbus::Result<String>,
) -> zbus::Result<AssistantReply> {
    let proxy = AssistantProxyBlocking::new(&bg::session_bus()?)?;
    let json = f(&proxy)?;
    serde_json::from_str(&json).map_err(|e| zbus::Error::Failure(e.to_string()))
}

// ---------------------------------------------------------------- Not yet implemented

/// A category MadOS Settings does not implement yet. Says so, and offers the
/// upstream KDE module when it exists (a real, working control).
pub fn unavailable(title: &str, kcm: Option<&'static str>) -> gtk::Widget {
    let (root, content) = widgets::page(title, None);
    content.append(
        &gtk::Label::builder()
            .label(format!(
                "{title} settings are not yet implemented in MadOS Settings (development build)."
            ))
            .xalign(0.0)
            .wrap(true)
            .build(),
    );
    if let Some(kcm) = kcm {
        let available = crate::kde_settings_available();
        let btn = gtk::Button::builder()
            .label("Open in KDE System Settings")
            .halign(gtk::Align::Start)
            .sensitive(available)
            .build();
        let status = widgets::status_label();
        if !available {
            status.set_label("KDE System Settings is not installed on this system.");
        }
        let st = status.clone();
        btn.connect_clicked(move |_| {
            if let Err(e) = std::process::Command::new(crate::KDE_SETTINGS).arg(kcm).spawn() {
                st.set_label(&format!("Could not open KDE System Settings: {e}"));
            }
        });
        content.append(&btn);
        content.append(&status);
    }
    root.upcast()
}
