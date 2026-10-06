//! Mock AccountsService: two cached users (an administrator and a standard user).

use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

pub struct Accounts;

#[interface(name = "org.freedesktop.Accounts")]
impl Accounts {
    fn list_cached_users(&self) -> Vec<OwnedObjectPath> {
        [
            "/org/freedesktop/Accounts/User1001",
            "/org/freedesktop/Accounts/User1000",
        ]
        .into_iter()
        .map(|p| ObjectPath::try_from(p).unwrap().into())
        .collect()
    }
}

pub struct User {
    pub name: &'static str,
    pub real: &'static str,
    pub uid: u64,
    pub admin: bool,
}

#[interface(name = "org.freedesktop.Accounts.User")]
impl User {
    #[zbus(property)]
    fn user_name(&self) -> String {
        self.name.into()
    }
    #[zbus(property)]
    fn real_name(&self) -> String {
        self.real.into()
    }
    #[zbus(property)]
    fn account_type(&self) -> i32 {
        i32::from(self.admin)
    }
    #[zbus(property)]
    fn uid(&self) -> u64 {
        self.uid
    }
    #[zbus(property)]
    fn locked(&self) -> bool {
        false
    }
}

pub async fn serve(address: &str) -> zbus::Connection {
    zbus::connection::Builder::address(address)
        .unwrap()
        .name("org.freedesktop.Accounts")
        .unwrap()
        .serve_at("/org/freedesktop/Accounts", Accounts)
        .unwrap()
        .serve_at(
            "/org/freedesktop/Accounts/User1000",
            User {
                name: "mados",
                real: "Dev User",
                uid: 1000,
                admin: true,
            },
        )
        .unwrap()
        .serve_at(
            "/org/freedesktop/Accounts/User1001",
            User {
                name: "guest",
                real: "",
                uid: 1001,
                admin: false,
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap()
}
