//! AccountsService client (upstream `org.freedesktop.Accounts`, system bus):
//! read-only list of human user accounts. Account changes are not done here;
//! AccountsService authorizes those itself with polkit when they are added.

use serde::{Deserialize, Serialize};
use zbus::zvariant::OwnedObjectPath;
use zbus::{proxy, Connection};

#[proxy(
    interface = "org.freedesktop.Accounts",
    default_service = "org.freedesktop.Accounts",
    default_path = "/org/freedesktop/Accounts"
)]
pub trait Accounts {
    fn list_cached_users(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.Accounts.User",
    default_service = "org.freedesktop.Accounts"
)]
pub trait AccountsUser {
    #[zbus(property)]
    fn user_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn real_name(&self) -> zbus::Result<String>;
    /// 0 = standard, 1 = administrator.
    #[zbus(property)]
    fn account_type(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn uid(&self) -> zbus::Result<u64>;
    #[zbus(property)]
    fn locked(&self) -> zbus::Result<bool>;
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Account {
    pub user_name: String,
    pub real_name: String,
    pub uid: u64,
    pub administrator: bool,
    pub locked: bool,
}

/// Human accounts known to AccountsService, sorted by uid.
pub async fn list(conn: &Connection) -> zbus::Result<Vec<Account>> {
    let accounts = AccountsProxy::new(conn).await?;
    let mut out = Vec::new();
    for path in accounts.list_cached_users().await? {
        let u = AccountsUserProxy::builder(conn).path(path)?.build().await?;
        out.push(Account {
            user_name: u.user_name().await?,
            real_name: u.real_name().await.unwrap_or_default(),
            uid: u.uid().await.unwrap_or(0),
            administrator: u.account_type().await.unwrap_or(0) == 1,
            locked: u.locked().await.unwrap_or(false),
        });
    }
    out.sort_by_key(|a| a.uid);
    Ok(out)
}
