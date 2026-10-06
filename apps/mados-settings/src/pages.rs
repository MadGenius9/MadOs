//! Settings pages. Each page shows real data or performs a real action through
//! the MadOS system API; anything not implemented says so explicitly.

use crate::bg;
use crate::widgets::{self, InfoGrid};
use gtk::glib;
use gtk::prelude::*;
use mados_api::{AssistantProxyBlocking, AssistantReply, ReplyStatus, SystemProxyBlocking, UpdateStatus};
use mados_core::sysinfo::{self, format_bytes, SessionEnv};
use mados_core::{Product, SystemInfo};
use std::cell::RefCell;
use std::rc::Rc;

fn product_name() -> String {
    Product::load().product.name
}

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
    g.row(
        "Build ID",
        &i.build_id
            .clone()
            .unwrap_or_else(|| format!("Not a {} image build", i.product_name)),
    );
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
        Some("Restart and shut down go through the system service and are authorized by polkit."),
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
            .label(format!(
                "Battery, suspend and power-profile settings: not yet implemented in {} Settings.",
                product_name()
            ))
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

/// Deployment status plus whether a job is running.
type UpdateRead = zbus::Result<(UpdateStatus, bool)>;

fn read_update_status(check: bool) -> UpdateRead {
    let proxy = SystemProxyBlocking::new(&bg::system_bus()?)?;
    let json = if check {
        proxy.check_for_update()?
    } else {
        proxy.get_update_status()?
    };
    let status = serde_json::from_str(&json).map_err(|e| zbus::Error::Failure(e.to_string()))?;
    Ok((status, proxy.busy().unwrap_or(false)))
}

/// Starts an update/rollback job and waits for its UpdateJobFinished signal.
fn run_update_job(operation: &'static str) -> zbus::Result<(bool, String)> {
    let conn = bg::system_bus()?;
    let proxy = SystemProxyBlocking::new(&conn)?;
    // Subscribe first so the completion signal cannot be missed.
    let signals = proxy.receive_update_job_finished()?;
    if operation == "update" {
        proxy.start_update()?;
    } else {
        proxy.start_rollback()?;
    }
    for sig in signals {
        let args = sig.args()?;
        if *args.operation() == operation {
            return Ok((*args.success(), args.message().to_string()));
        }
    }
    Err(zbus::Error::Failure("the system service stopped".into()))
}

fn describe_deployment(d: &mados_api::Deployment) -> String {
    format!(
        "{}\n{}{}",
        d.image.as_deref().unwrap_or("unknown image"),
        d.version
            .as_deref()
            .map(|v| format!("version {v} "))
            .unwrap_or_default(),
        d.timestamp.as_deref().unwrap_or("")
    )
}

pub fn updates() -> gtk::Widget {
    let (root, content) = widgets::page(
        "Updates",
        Some(&format!(
            "{} is image-based: an update installs as a new version next to the running one and takes effect \
             after a restart. The previous version is kept, so you can roll back.",
            product_name()
        )),
    );
    let grid = Rc::new(RefCell::new(InfoGrid::new()));
    content.append(&grid.borrow().grid);
    let status = widgets::status_label();
    content.append(&status);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let check = gtk::Button::with_label("Check for Updates");
    let install = gtk::Button::builder()
        .label("Download and Install")
        .css_classes(["suggested-action"])
        .build();
    let rollback = gtk::Button::with_label("Roll Back…");
    let restart = gtk::Button::builder().label("Restart Now…").visible(false).build();
    for b in [&check, &install, &rollback, &restart] {
        b.set_sensitive(false);
        buttons.append(b);
    }
    content.append(&buttons);
    content.append(
        &gtk::Label::builder()
            .label("Installing or rolling back requires an administrator password. Nothing restarts automatically.")
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build(),
    );

    // Renders a status read (or check) result and enables what is possible.
    let render: Rc<dyn Fn(UpdateRead)> = {
        let (grid, status) = (grid.clone(), status.clone());
        let (check, install, rollback, restart) = (check.clone(), install.clone(), rollback.clone(), restart.clone());
        Rc::new(move |r| {
            let mut g = grid.borrow_mut();
            g.clear();
            match r {
                Ok((s, busy)) if s.available => {
                    g.row(
                        "Running",
                        &s.booted
                            .as_ref()
                            .map(describe_deployment)
                            .unwrap_or_else(|| "Unknown".into()),
                    );
                    g.row(
                        "Installed, pending restart",
                        &s.staged
                            .as_ref()
                            .map(describe_deployment)
                            .unwrap_or_else(|| "None".into()),
                    );
                    g.row(
                        "Rollback",
                        &s.rollback
                            .as_ref()
                            .map(describe_deployment)
                            .unwrap_or_else(|| "None".into()),
                    );
                    g.row(
                        "Available update",
                        &s.cached_update
                            .as_ref()
                            .map(describe_deployment)
                            .unwrap_or_else(|| "None known (check for updates)".into()),
                    );
                    if busy {
                        status.set_label("An update or rollback is in progress…");
                    }
                    check.set_sensitive(!busy);
                    install.set_sensitive(!busy && s.cached_update.is_some());
                    rollback.set_sensitive(!busy && s.rollback.is_some());
                    restart.set_visible(s.staged.is_some());
                    restart.set_sensitive(s.staged.is_some());
                }
                Ok((s, _)) => {
                    status.set_label(&format!("Updates unavailable: {}", s.message.unwrap_or_default()));
                    for b in [&check, &install, &rollback] {
                        b.set_sensitive(false);
                    }
                }
                Err(e) => {
                    status.set_label(&format!("Updates unavailable: {}", bg::describe(&e)));
                    for b in [&check, &install, &rollback] {
                        b.set_sensitive(false);
                    }
                }
            }
        })
    };
    // Reloads (or checks); `notice` is shown once the reload has finished so
    // it is not overwritten by the reload's own progress text.
    let load: Rc<dyn Fn(bool, Option<String>)> = {
        let (render, status) = (render.clone(), status.clone());
        Rc::new(move |do_check: bool, notice: Option<String>| {
            status.set_label(if do_check {
                "Checking for updates…"
            } else {
                "Reading update status…"
            });
            let (render, status) = (render.clone(), status.clone());
            bg::run(
                move || read_update_status(do_check),
                move |r| {
                    if r.is_ok() {
                        status.set_label(notice.as_deref().unwrap_or(""));
                    }
                    render(r)
                },
            );
        })
    };
    let job = {
        let (load, status, buttons) = (
            load.clone(),
            status.clone(),
            [check.clone(), install.clone(), rollback.clone()],
        );
        move |operation: &'static str| {
            for b in &buttons {
                b.set_sensitive(false);
            }
            status.set_label(if operation == "update" {
                "Downloading and installing the update…"
            } else {
                "Preparing rollback…"
            });
            let load = load.clone();
            bg::run(
                move || run_update_job(operation),
                move |r| match r {
                    Ok((_, message)) => load(false, Some(message)),
                    Err(e) => load(false, Some(bg::describe(&e))),
                },
            );
        }
    };
    {
        let load = load.clone();
        check.connect_clicked(move |_| load(true, None));
    }
    {
        let job = job.clone();
        install.connect_clicked(move |_| job("update"));
    }
    rollback.connect_clicked(move |btn| {
        let job = job.clone();
        widgets::confirm(
            btn,
            "Roll back to the previous version?",
            "The previous version becomes active after the next restart.",
            "Roll Back",
            move || job("rollback"),
        );
    });
    {
        let status = status.clone();
        restart.connect_clicked(move |btn| {
            let status = status.clone();
            widgets::confirm(
                btn,
                "Restart now?",
                "Unsaved work in open applications may be lost.",
                "Restart",
                move || {
                    let status = status.clone();
                    bg::run(
                        || -> zbus::Result<()> { SystemProxyBlocking::new(&bg::system_bus()?)?.reboot() },
                        move |r| {
                            if let Err(e) = r {
                                status.set_label(&bg::describe(&e));
                            }
                        },
                    );
                },
            );
        });
    }
    load(false, None);
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

// ---------------------------------------------------------------- Network

fn network_status() -> zbus::Result<mados_api::network::NetworkStatus> {
    let conn = bg::system_bus()?;
    zbus::block_on(mados_api::network::status(conn.inner()))
}

pub fn network() -> gtk::Widget {
    let (root, content) = widgets::page("Network & Wi-Fi", None);
    let summary = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["heading"])
        .build();
    content.append(&summary);

    let wifi_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .css_classes(["card"])
        .build();
    let wifi_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    wifi_text.append(
        &gtk::Label::builder()
            .label("Wi-Fi")
            .xalign(0.0)
            .css_classes(["heading"])
            .build(),
    );
    let wifi_note = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    wifi_text.append(&wifi_note);
    wifi_text.set_hexpand(true);
    wifi_row.append(&wifi_text);
    let switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .sensitive(false)
        .build();
    wifi_row.append(&switch);
    content.append(&wifi_row);

    let grid = Rc::new(RefCell::new(InfoGrid::new()));
    content.append(&grid.borrow().grid);
    let status = widgets::status_label();
    content.append(&status);
    content.append(
        &gtk::Label::builder()
            .label(format!(
                "Choosing and connecting to networks is not yet implemented in {} Settings.",
                product_name()
            ))
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build(),
    );
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let refresh = gtk::Button::with_label("Refresh");
    buttons.append(&refresh);
    let kde = gtk::Button::builder()
        .label("Open in KDE System Settings")
        .sensitive(crate::kde_settings_available())
        .build();
    kde.connect_clicked(|_| {
        let _ = std::process::Command::new(crate::KDE_SETTINGS)
            .arg("kcm_networkmanagement")
            .spawn();
    });
    buttons.append(&kde);
    content.append(&buttons);

    // Set while the switch is updated from NetworkManager's state, so that
    // programmatic changes are not treated as user requests.
    let syncing = Rc::new(std::cell::Cell::new(false));

    let load: Rc<dyn Fn()> = {
        let (grid, status, summary, switch, wifi_note, syncing) = (
            grid.clone(),
            status.clone(),
            summary.clone(),
            switch.clone(),
            wifi_note.clone(),
            syncing.clone(),
        );
        Rc::new(move || {
            let (grid, status, summary, switch, wifi_note, syncing) = (
                grid.clone(),
                status.clone(),
                summary.clone(),
                switch.clone(),
                wifi_note.clone(),
                syncing.clone(),
            );
            bg::run(network_status, move |r| {
                let mut g = grid.borrow_mut();
                g.clear();
                match r {
                    Ok(s) => {
                        summary.set_label(&format!("Network: {} · Internet access: {}", s.state, s.connectivity));
                        syncing.set(true);
                        switch.set_active(s.wifi_enabled);
                        switch.set_state(s.wifi_enabled);
                        syncing.set(false);
                        let has_wifi = s.has_wifi_device();
                        switch.set_sensitive(has_wifi && s.wifi_hardware_enabled);
                        wifi_note.set_label(if !has_wifi {
                            "No Wi-Fi adapter detected."
                        } else if !s.wifi_hardware_enabled {
                            "Wi-Fi is disabled by a hardware switch."
                        } else if s.wifi_enabled {
                            "Wi-Fi radio is on."
                        } else {
                            "Wi-Fi radio is off."
                        });
                        if s.devices.is_empty() {
                            g.row("Devices", "No network devices");
                        }
                        for d in &s.devices {
                            let mut text = d.state.clone();
                            if let Some(c) = &d.connection {
                                text.push_str(&format!(" · {c}"));
                            }
                            if !d.ipv4.is_empty() {
                                text.push_str(&format!("\n{}", d.ipv4.join(", ")));
                            }
                            g.row(&format!("{} ({})", d.interface, d.kind), &text);
                        }
                    }
                    Err(e) => {
                        summary.set_label("Network status unavailable");
                        switch.set_sensitive(false);
                        status.set_label(&format!("NetworkManager: {}", bg::describe(&e)));
                    }
                }
            });
        })
    };

    {
        let (load, status, syncing) = (load.clone(), status.clone(), syncing.clone());
        switch.connect_state_set(move |sw, want| {
            if syncing.get() {
                return glib::Propagation::Proceed;
            }
            sw.set_sensitive(false);
            status.set_label(if want {
                "Turning Wi-Fi on…"
            } else {
                "Turning Wi-Fi off…"
            });
            let (load, status) = (load.clone(), status.clone());
            bg::run(
                move || -> zbus::Result<()> {
                    let conn = bg::system_bus()?;
                    zbus::block_on(mados_api::network::set_wifi_enabled(conn.inner(), want))
                },
                move |r| {
                    match r {
                        Ok(()) => status.set_label(""),
                        Err(e) => status.set_label(&format!("Could not change Wi-Fi: {}", bg::describe(&e))),
                    }
                    // Show NetworkManager's actual state either way.
                    load();
                },
            );
            glib::Propagation::Stop
        });
    }
    load();
    refresh.connect_clicked(move |_| load());
    root.upcast()
}

// ---------------------------------------------------------------- Bluetooth

fn bluetooth_status() -> zbus::Result<mados_api::bluetooth::BluetoothStatus> {
    let conn = bg::system_bus()?;
    zbus::block_on(mados_api::bluetooth::status(conn.inner()))
}

pub fn bluetooth() -> gtk::Widget {
    let (root, content) = widgets::page("Bluetooth", None);
    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .css_classes(["card"])
        .build();
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.append(
        &gtk::Label::builder()
            .label("Bluetooth")
            .xalign(0.0)
            .css_classes(["heading"])
            .build(),
    );
    let note = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    text.append(&note);
    card.append(&text);
    let switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .sensitive(false)
        .build();
    card.append(&switch);
    content.append(&card);

    content.append(
        &gtk::Label::builder()
            .label("Paired devices")
            .xalign(0.0)
            .css_classes(["heading"])
            .build(),
    );
    let grid = Rc::new(RefCell::new(InfoGrid::new()));
    content.append(&grid.borrow().grid);
    let status = widgets::status_label();
    content.append(&status);
    content.append(
        &gtk::Label::builder()
            .label(format!(
                "Pairing and connecting devices is not yet implemented in {} Settings.",
                product_name()
            ))
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build(),
    );
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let refresh = gtk::Button::with_label("Refresh");
    buttons.append(&refresh);
    let kde = gtk::Button::builder()
        .label("Open in KDE System Settings")
        .sensitive(crate::kde_settings_available())
        .build();
    kde.connect_clicked(|_| {
        let _ = std::process::Command::new(crate::KDE_SETTINGS)
            .arg("kcm_bluetooth")
            .spawn();
    });
    buttons.append(&kde);
    content.append(&buttons);

    let syncing = Rc::new(std::cell::Cell::new(false));
    let load: Rc<dyn Fn()> = {
        let (grid, status, note, switch, syncing) = (
            grid.clone(),
            status.clone(),
            note.clone(),
            switch.clone(),
            syncing.clone(),
        );
        Rc::new(move || {
            let (grid, status, note, switch, syncing) = (
                grid.clone(),
                status.clone(),
                note.clone(),
                switch.clone(),
                syncing.clone(),
            );
            bg::run(bluetooth_status, move |r| {
                let mut g = grid.borrow_mut();
                g.clear();
                match r {
                    Ok(s) => match s.adapter {
                        Some(a) => {
                            syncing.set(true);
                            switch.set_active(a.powered);
                            switch.set_state(a.powered);
                            syncing.set(false);
                            switch.set_sensitive(true);
                            note.set_label(&format!(
                                "{} ({}) is {}.",
                                if a.name.is_empty() { "Adapter" } else { &a.name },
                                a.address,
                                if a.powered { "on" } else { "off" }
                            ));
                            if s.devices.is_empty() {
                                g.row("Devices", "No paired devices");
                            }
                            for d in &s.devices {
                                let state = match (d.connected, d.paired) {
                                    (true, _) => "Connected",
                                    (false, true) => "Paired, not connected",
                                    _ => "Not paired",
                                };
                                g.row(&d.name, &format!("{state}\n{}", d.address));
                            }
                        }
                        None => {
                            switch.set_sensitive(false);
                            note.set_label("No Bluetooth adapter detected.");
                        }
                    },
                    Err(e) => {
                        switch.set_sensitive(false);
                        note.set_label("Bluetooth service unavailable.");
                        status.set_label(&format!("BlueZ: {}", bg::describe(&e)));
                    }
                }
            });
        })
    };
    {
        let (load, status, syncing) = (load.clone(), status.clone(), syncing.clone());
        switch.connect_state_set(move |sw, want| {
            if syncing.get() {
                return glib::Propagation::Proceed;
            }
            sw.set_sensitive(false);
            status.set_label(if want {
                "Turning Bluetooth on…"
            } else {
                "Turning Bluetooth off…"
            });
            let (load, status) = (load.clone(), status.clone());
            bg::run(
                move || -> zbus::Result<bool> {
                    let conn = bg::system_bus()?;
                    zbus::block_on(mados_api::bluetooth::set_powered(conn.inner(), want))
                },
                move |r| {
                    match r {
                        Ok(_) => status.set_label(""),
                        Err(e) => status.set_label(&format!("Could not change Bluetooth: {}", bg::describe(&e))),
                    }
                    load();
                },
            );
            glib::Propagation::Stop
        });
    }
    load();
    refresh.connect_clicked(move |_| load());
    root.upcast()
}

// ---------------------------------------------------------------- Display

pub fn display() -> gtk::Widget {
    let (root, content) = widgets::page("Display", None);
    let root_fs = std::path::PathBuf::from("/");

    content.append(
        &gtk::Label::builder()
            .label("Brightness")
            .xalign(0.0)
            .css_classes(["heading"])
            .build(),
    );
    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .css_classes(["card"])
        .build();
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 100.0, 1.0);
    scale.set_hexpand(true);
    scale.set_draw_value(true);
    scale.set_value_pos(gtk::PositionType::Right);
    let note = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    card.append(&scale);
    card.append(&note);
    content.append(&card);

    content.append(
        &gtk::Label::builder()
            .label("Outputs")
            .xalign(0.0)
            .css_classes(["heading"])
            .build(),
    );
    let grid = Rc::new(RefCell::new(InfoGrid::new()));
    content.append(&grid.borrow().grid);
    let status = widgets::status_label();
    content.append(&status);
    content.append(
        &gtk::Label::builder()
            .label(format!(
                "Resolution, scaling and arrangement are not yet implemented in {} Settings.",
                product_name()
            ))
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build(),
    );
    let kde = gtk::Button::builder()
        .label("Open in KDE System Settings")
        .halign(gtk::Align::Start)
        .sensitive(crate::kde_settings_available())
        .build();
    kde.connect_clicked(|_| {
        let _ = std::process::Command::new(crate::KDE_SETTINGS)
            .arg("kcm_kscreen")
            .spawn();
    });
    content.append(&kde);

    // Set while the slider shows the value read from sysfs, so that is not
    // sent back as a brightness request.
    let syncing = Rc::new(std::cell::Cell::new(false));

    // Outputs and current brightness (sysfs reads, unprivileged).
    {
        let (grid, scale, note, syncing) = (grid.clone(), scale.clone(), note.clone(), syncing.clone());
        let r = root_fs.clone();
        bg::run(
            move || (mados_api::display::outputs(&r), mados_api::display::backlight(&r)),
            move |(outs, bl)| {
                let mut g = grid.borrow_mut();
                if outs.is_empty() {
                    g.row("Outputs", "No display connectors found");
                }
                for o in &outs {
                    let state = match (o.connected, &o.preferred_mode) {
                        (true, Some(m)) => format!("Connected · preferred mode {m}"),
                        (true, None) => "Connected".to_string(),
                        (false, _) => "Disconnected".to_string(),
                    };
                    g.row(&o.name, &state);
                }
                match bl {
                    Some(b) => {
                        syncing.set(true);
                        scale.set_value(f64::from(b.percent().max(1)));
                        syncing.set(false);
                        note.set_label(&format!("Built-in display backlight ({})", b.name));
                    }
                    None => {
                        // No fake value: there is nothing to adjust.
                        scale.set_visible(false);
                        note.set_label("No adjustable built-in display (external monitor or virtual machine).");
                    }
                }
            },
        );
    }

    // Apply slider changes through logind, debounced so dragging sends a
    // request only after the value settles.
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    scale.connect_value_changed(move |sc| {
        if syncing.get() {
            return;
        }
        if let Some(id) = pending.borrow_mut().take() {
            id.remove();
        }
        let percent = sc.value().round().clamp(1.0, 100.0) as u8;
        let (status, pending_inner, r) = (status.clone(), pending.clone(), root_fs.clone());
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
            pending_inner.borrow_mut().take();
            bg::run(
                move || -> zbus::Result<bool> {
                    let conn = bg::system_bus()?;
                    zbus::block_on(mados_api::display::set_brightness(conn.inner(), &r, percent))
                },
                move |res| match res {
                    Ok(_) => status.set_label(""),
                    Err(e) => status.set_label(&format!("Could not change brightness: {}", bg::describe(&e))),
                },
            );
        });
        *pending.borrow_mut() = Some(id);
    });
    root.upcast()
}

// ---------------------------------------------------------------- Sound

pub fn sound() -> gtk::Widget {
    let (root, content) = widgets::page("Sound", None);
    content.append(
        &gtk::Label::builder()
            .label("Output")
            .xalign(0.0)
            .css_classes(["heading"])
            .build(),
    );
    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .css_classes(["card"])
        .build();
    let device = gtk::Label::builder().xalign(0.0).wrap(true).build();
    card.append(&device);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_hexpand(true);
    scale.set_draw_value(true);
    scale.set_value_pos(gtk::PositionType::Right);
    row.append(&scale);
    row.append(&gtk::Label::new(Some("Mute")));
    let mute = gtk::Switch::builder().valign(gtk::Align::Center).build();
    row.append(&mute);
    card.append(&row);
    content.append(&card);

    content.append(
        &gtk::Label::builder()
            .label("Output devices")
            .xalign(0.0)
            .css_classes(["heading"])
            .build(),
    );
    let grid = Rc::new(RefCell::new(InfoGrid::new()));
    content.append(&grid.borrow().grid);
    let status = widgets::status_label();
    content.append(&status);
    content.append(
        &gtk::Label::builder()
            .label(format!(
                "Choosing the output device and input settings are not yet implemented in {} Settings.",
                product_name()
            ))
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build(),
    );
    let kde = gtk::Button::builder()
        .label("Open in KDE System Settings")
        .halign(gtk::Align::Start)
        .sensitive(crate::kde_settings_available())
        .build();
    kde.connect_clicked(|_| {
        let _ = std::process::Command::new(crate::KDE_SETTINGS)
            .arg("kcm_pulseaudio")
            .spawn();
    });
    content.append(&kde);

    let syncing = Rc::new(std::cell::Cell::new(false));
    let load: Rc<dyn Fn()> = {
        let (grid, status, device, scale, mute, row, syncing) = (
            grid.clone(),
            status.clone(),
            device.clone(),
            scale.clone(),
            mute.clone(),
            row.clone(),
            syncing.clone(),
        );
        Rc::new(move || {
            let (grid, status, device, scale, mute, row, syncing) = (
                grid.clone(),
                status.clone(),
                device.clone(),
                scale.clone(),
                mute.clone(),
                row.clone(),
                syncing.clone(),
            );
            bg::run(mados_audio::status, move |r| {
                let mut g = grid.borrow_mut();
                g.clear();
                match r {
                    Ok(st) => {
                        match st.default_output() {
                            Some(o) => {
                                device.set_label(&o.description);
                                syncing.set(true);
                                scale.set_value(f64::from(o.volume_percent.min(100)));
                                mute.set_active(o.muted);
                                mute.set_state(o.muted);
                                syncing.set(false);
                                row.set_visible(true);
                            }
                            None => {
                                device.set_label("No output device.");
                                row.set_visible(false);
                            }
                        }
                        for o in &st.outputs {
                            let mut text = format!("{}%{}", o.volume_percent, if o.muted { " · muted" } else { "" });
                            if o.is_default {
                                text.push_str(" · default");
                            }
                            g.row(&o.description, &text);
                        }
                        status.set_label(&format!("Sound server: {}", st.server));
                    }
                    Err(e) => {
                        device.set_label("Sound is unavailable.");
                        row.set_visible(false);
                        status.set_label(&e.to_string());
                    }
                }
            });
        })
    };

    {
        let (status, syncing) = (status.clone(), syncing.clone());
        let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
        scale.connect_value_changed(move |sc| {
            if syncing.get() {
                return;
            }
            if let Some(id) = pending.borrow_mut().take() {
                id.remove();
            }
            let percent = sc.value().round().clamp(0.0, 100.0) as u32;
            let (status, pending_inner) = (status.clone(), pending.clone());
            let id = glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
                pending_inner.borrow_mut().take();
                bg::run(
                    move || mados_audio::set_volume(percent),
                    move |r| {
                        if let Err(e) = r {
                            status.set_label(&format!("Could not change volume: {e}"));
                        }
                    },
                );
            });
            *pending.borrow_mut() = Some(id);
        });
    }
    {
        let (load, status, syncing) = (load.clone(), status.clone(), syncing.clone());
        mute.connect_state_set(move |sw, want| {
            if syncing.get() {
                return glib::Propagation::Proceed;
            }
            sw.set_sensitive(false);
            let (load, status, sw) = (load.clone(), status.clone(), sw.clone());
            bg::run(
                move || mados_audio::set_muted(want),
                move |r| {
                    if let Err(e) = r {
                        status.set_label(&format!("Could not change mute: {e}"));
                    }
                    sw.set_sensitive(true);
                    load();
                },
            );
            glib::Propagation::Stop
        });
    }
    load();
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
                "{title} settings are not yet implemented in {} Settings (development build).",
                product_name()
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
