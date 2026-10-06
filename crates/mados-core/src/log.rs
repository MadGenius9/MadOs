//! Minimal journald-friendly logging.
//!
//! MadOS services log to stderr with `sd-daemon(3)` priority prefixes
//! (`<3>` error … `<7>` debug) when stderr is the journal. Under systemd, journald parses the prefix
//! into the PRIORITY field and the unit's SyslogIdentifier names the
//! component, so no logging framework or journald library is required.
//! Debug messages are emitted only when `MADOS_DEBUG` is set.

use std::io::Write;

fn emit(prio: u8, msg: std::fmt::Arguments<'_>) {
    let mut err = std::io::stderr().lock();
    // systemd sets JOURNAL_STREAM when stderr is connected to journald.
    if std::env::var_os("JOURNAL_STREAM").is_some() {
        let _ = writeln!(err, "<{prio}>{msg}");
    } else {
        let level = match prio {
            0..=3 => "error",
            4 => "warning",
            5 => "notice",
            6 => "info",
            _ => "debug",
        };
        let _ = writeln!(err, "{level}: {msg}");
    }
}

pub fn error(msg: std::fmt::Arguments<'_>) {
    emit(3, msg)
}
pub fn warn(msg: std::fmt::Arguments<'_>) {
    emit(4, msg)
}
pub fn notice(msg: std::fmt::Arguments<'_>) {
    emit(5, msg)
}
pub fn info(msg: std::fmt::Arguments<'_>) {
    emit(6, msg)
}
pub fn debug(msg: std::fmt::Arguments<'_>) {
    if std::env::var_os("MADOS_DEBUG").is_some() {
        emit(7, msg)
    }
}

#[macro_export]
macro_rules! log_error { ($($t:tt)*) => { $crate::log::error(format_args!($($t)*)) } }
#[macro_export]
macro_rules! log_warn { ($($t:tt)*) => { $crate::log::warn(format_args!($($t)*)) } }
#[macro_export]
macro_rules! log_notice { ($($t:tt)*) => { $crate::log::notice(format_args!($($t)*)) } }
#[macro_export]
macro_rules! log_info { ($($t:tt)*) => { $crate::log::info(format_args!($($t)*)) } }
#[macro_export]
macro_rules! log_debug { ($($t:tt)*) => { $crate::log::debug(format_args!($($t)*)) } }
