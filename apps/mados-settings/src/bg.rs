//! Run blocking work (D-Bus calls, filesystem probing) off the UI thread.

use gtk::glib;
use std::sync::OnceLock;

/// Runs `work` on a worker thread and `done` with its result on the UI thread.
pub fn run<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let _ = tx.send_blocking(work());
    });
    glib::spawn_future_local(async move {
        if let Ok(v) = rx.recv().await {
            done(v);
        }
    });
}

/// One session-bus connection for the app's lifetime, so the assistant sees a
/// stable caller identity across Ask and Confirm.
pub fn session_bus() -> zbus::Result<zbus::blocking::Connection> {
    static CONN: OnceLock<zbus::blocking::Connection> = OnceLock::new();
    if let Some(c) = CONN.get() {
        return Ok(c.clone());
    }
    let c = zbus::blocking::Connection::session()?;
    Ok(CONN.get_or_init(|| c).clone())
}

pub fn system_bus() -> zbus::Result<zbus::blocking::Connection> {
    static CONN: OnceLock<zbus::blocking::Connection> = OnceLock::new();
    if let Some(c) = CONN.get() {
        return Ok(c.clone());
    }
    let c = zbus::blocking::Connection::system()?;
    Ok(CONN.get_or_init(|| c).clone())
}

/// Maps a D-Bus error to a short user-facing sentence.
pub fn describe(e: &zbus::Error) -> String {
    match e {
        zbus::Error::MethodError(name, msg, _) if name.as_str().ends_with("NotAuthorized") => {
            format!(
                "Not authorized{}",
                msg.as_deref().map(|m| format!(": {m}")).unwrap_or_default()
            )
        }
        zbus::Error::MethodError(name, _, _) if name.as_str() == "org.freedesktop.DBus.Error.ServiceUnknown" => {
            "The system service is not running.".into()
        }
        zbus::Error::InputOutput(_) => "The message bus is not available.".into(),
        // Upstream services (e.g. NetworkManager) refuse with the standard
        // AccessDenied error, which zbus may surface as a typed FDO error.
        other if other.to_string().contains("AccessDenied") => "Not authorized.".into(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friendly_errors() {
        let denied = zbus::Error::FDO(Box::new(zbus::fdo::Error::AccessDenied("not authorized".into())));
        assert_eq!(describe(&denied), "Not authorized.");
        assert_eq!(
            describe(&zbus::Error::Failure("boom".into())),
            zbus::Error::Failure("boom".into()).to_string()
        );
    }
}
