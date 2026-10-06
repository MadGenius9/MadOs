//! Mock systemd-logind: the calling user's graphical session and
//! Session.SetBrightness, which records requests.

use std::sync::{Arc, Mutex};
use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

pub const SESSION_PATH: &str = "/org/freedesktop/login1/session/_32";

pub struct User;

#[interface(name = "org.freedesktop.login1.User")]
impl User {
    #[zbus(property)]
    fn display(&self) -> (String, OwnedObjectPath) {
        ("2".into(), ObjectPath::try_from(SESSION_PATH).unwrap().into())
    }
}

pub struct Session {
    pub calls: Arc<Mutex<Vec<(String, String, u32)>>>,
    pub deny: bool,
}

#[interface(name = "org.freedesktop.login1.Session")]
impl Session {
    fn set_brightness(&self, subsystem: String, name: String, brightness: u32) -> zbus::fdo::Result<()> {
        if self.deny {
            return Err(zbus::fdo::Error::AccessDenied("not the active session".into()));
        }
        self.calls.lock().unwrap().push((subsystem, name, brightness));
        Ok(())
    }
}

/// Serves the mock as `org.freedesktop.login1`; returns the recorded calls.
pub async fn serve(address: &str, deny: bool) -> (zbus::Connection, Arc<Mutex<Vec<(String, String, u32)>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let conn = zbus::connection::Builder::address(address)
        .unwrap()
        .name("org.freedesktop.login1")
        .unwrap()
        .serve_at("/org/freedesktop/login1/user/self", User)
        .unwrap()
        .serve_at(
            SESSION_PATH,
            Session {
                calls: calls.clone(),
                deny,
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    (conn, calls)
}
