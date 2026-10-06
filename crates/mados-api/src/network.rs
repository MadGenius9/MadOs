//! NetworkManager client (upstream `org.freedesktop.NetworkManager`, system bus).
//!
//! MadOS does not wrap NetworkManager in its own service: NetworkManager
//! already authorizes callers with polkit, so Settings and the assistant use
//! its API directly as the user. This module is the single MadOS client for it.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{proxy, Connection};

pub const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
pub const NM_PATH: &str = "/org/freedesktop/NetworkManager";

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
pub trait NetworkManager {
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    #[zbus(property)]
    fn wireless_enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_wireless_enabled(&self, value: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn connectivity(&self) -> zbus::Result<u32>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait NmDevice {
    #[zbus(property)]
    fn interface(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn device_type(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn ip4_config(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn active_connection(&self) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait NmActiveConnection {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.IP4Config",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait NmIp4Config {
    #[zbus(property)]
    fn address_data(&self) -> zbus::Result<Vec<HashMap<String, OwnedValue>>>;
}

// NetworkManager enum values (NMDeviceType, NMDeviceState, NMState, NMConnectivityState).
pub const DEVICE_TYPE_ETHERNET: u32 = 1;
pub const DEVICE_TYPE_WIFI: u32 = 2;
pub const DEVICE_TYPE_LOOPBACK: u32 = 32;
pub const DEVICE_STATE_UNMANAGED: u32 = 10;
pub const DEVICE_STATE_ACTIVATED: u32 = 100;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct NetDevice {
    pub interface: String,
    /// "ethernet", "wifi", "loopback" or "other".
    pub kind: String,
    /// Human-readable device state.
    pub state: String,
    pub connected: bool,
    /// Active connection profile name.
    pub connection: Option<String>,
    /// IPv4 addresses in CIDR notation.
    pub ipv4: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct NetworkStatus {
    pub schema: u32,
    pub state: String,
    pub connectivity: String,
    pub wifi_enabled: bool,
    pub wifi_hardware_enabled: bool,
    pub devices: Vec<NetDevice>,
}

impl NetworkStatus {
    pub fn has_wifi_device(&self) -> bool {
        self.devices.iter().any(|d| d.kind == "wifi")
    }
}

pub fn device_kind(t: u32) -> &'static str {
    match t {
        DEVICE_TYPE_ETHERNET => "ethernet",
        DEVICE_TYPE_WIFI => "wifi",
        DEVICE_TYPE_LOOPBACK => "loopback",
        _ => "other",
    }
}

pub fn device_state(s: u32) -> &'static str {
    match s {
        10 => "unmanaged",
        20 => "unavailable",
        30 => "disconnected",
        40..=90 => "connecting",
        100 => "connected",
        110 => "disconnecting",
        120 => "failed",
        _ => "unknown",
    }
}

pub fn manager_state(s: u32) -> &'static str {
    match s {
        10 => "asleep",
        20 => "disconnected",
        30 => "disconnecting",
        40 => "connecting",
        50 => "connected (local only)",
        60 => "connected (site only)",
        70 => "connected",
        _ => "unknown",
    }
}

pub fn connectivity(c: u32) -> &'static str {
    match c {
        1 => "none",
        2 => "captive portal",
        3 => "limited",
        4 => "full",
        _ => "unknown",
    }
}

fn is_null_path(p: &OwnedObjectPath) -> bool {
    p.as_str() == "/"
}

async fn ipv4_addresses(conn: &Connection, path: &OwnedObjectPath) -> Vec<String> {
    if is_null_path(path) {
        return Vec::new();
    }
    let Ok(builder) = NmIp4ConfigProxy::builder(conn).path(path.clone()) else {
        return Vec::new();
    };
    let Ok(proxy) = builder.build().await else {
        return Vec::new();
    };
    let Ok(data) = proxy.address_data().await else {
        return Vec::new();
    };
    data.iter()
        .filter_map(|entry| {
            let addr = match &**entry.get("address")? {
                Value::Str(s) => s.to_string(),
                _ => return None,
            };
            let prefix = match entry.get("prefix").map(|v| &**v) {
                Some(Value::U32(p)) => *p,
                _ => 32,
            };
            Some(format!("{addr}/{prefix}"))
        })
        .collect()
}

async fn active_connection_id(conn: &Connection, path: &OwnedObjectPath) -> Option<String> {
    if is_null_path(path) {
        return None;
    }
    let proxy = NmActiveConnectionProxy::builder(conn)
        .path(path.clone())
        .ok()?
        .build()
        .await
        .ok()?;
    proxy.id().await.ok()
}

/// Reads the current network status. Loopback and unmanaged devices are omitted.
pub async fn status(conn: &Connection) -> zbus::Result<NetworkStatus> {
    let nm = NetworkManagerProxy::new(conn).await?;
    let mut devices = Vec::new();
    for path in nm.devices().await? {
        let dev = NmDeviceProxy::builder(conn).path(path)?.build().await?;
        let kind = device_kind(dev.device_type().await.unwrap_or(0));
        let state = dev.state().await.unwrap_or(0);
        if kind == "loopback" || state == DEVICE_STATE_UNMANAGED {
            continue;
        }
        let connected = state == DEVICE_STATE_ACTIVATED;
        let (connection, ipv4) = if connected {
            let ac = dev.active_connection().await.ok();
            let ip = dev.ip4_config().await.ok();
            (
                match ac {
                    Some(p) => active_connection_id(conn, &p).await,
                    None => None,
                },
                match ip {
                    Some(p) => ipv4_addresses(conn, &p).await,
                    None => Vec::new(),
                },
            )
        } else {
            (None, Vec::new())
        };
        devices.push(NetDevice {
            interface: dev.interface().await.unwrap_or_default(),
            kind: kind.to_string(),
            state: device_state(state).to_string(),
            connected,
            connection,
            ipv4,
        });
    }
    devices.sort_by(|a, b| a.interface.cmp(&b.interface));
    Ok(NetworkStatus {
        schema: 1,
        state: manager_state(nm.state().await.unwrap_or(0)).to_string(),
        connectivity: connectivity(nm.connectivity().await.unwrap_or(0)).to_string(),
        wifi_enabled: nm.wireless_enabled().await.unwrap_or(false),
        wifi_hardware_enabled: nm.wireless_hardware_enabled().await.unwrap_or(false),
        devices,
    })
}

/// Turns the Wi-Fi radio on or off. NetworkManager authorizes the caller
/// (polkit `org.freedesktop.NetworkManager.enable-disable-wifi`).
pub async fn set_wifi_enabled(conn: &Connection, enabled: bool) -> zbus::Result<()> {
    NetworkManagerProxy::new(conn)
        .await?
        .set_wireless_enabled(enabled)
        .await
}
