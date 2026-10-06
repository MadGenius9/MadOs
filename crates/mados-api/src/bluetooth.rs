//! BlueZ client (upstream `org.bluez`, system bus).
//!
//! Like NetworkManager, BlueZ is used directly: its D-Bus policy and agent
//! model govern what callers may do. This is the single MadOS client for it,
//! shared by Settings and the assistant.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::zvariant::{OwnedValue, Value};
use zbus::Connection;

pub const BLUEZ_SERVICE: &str = "org.bluez";
pub const ADAPTER_INTERFACE: &str = "org.bluez.Adapter1";
pub const DEVICE_INTERFACE: &str = "org.bluez.Device1";

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Adapter {
    pub path: String,
    pub name: String,
    pub address: String,
    pub powered: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct BtDevice {
    pub name: String,
    pub address: String,
    pub paired: bool,
    pub connected: bool,
    /// freedesktop icon name BlueZ reports (e.g. `audio-headset`), if any.
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct BluetoothStatus {
    pub schema: u32,
    /// First adapter (sorted by object path); `None` when there is none.
    pub adapter: Option<Adapter>,
    /// Paired or connected devices of that adapter.
    pub devices: Vec<BtDevice>,
}

type Props = HashMap<String, OwnedValue>;

fn prop_str(p: &Props, key: &str) -> Option<String> {
    match p.get(key).map(|v| &**v) {
        Some(Value::Str(s)) => Some(s.to_string()),
        Some(Value::ObjectPath(o)) => Some(o.to_string()),
        _ => None,
    }
}

fn prop_bool(p: &Props, key: &str) -> bool {
    matches!(p.get(key).map(|v| &**v), Some(Value::Bool(true)))
}

/// Builds the status from BlueZ's `GetManagedObjects` result.
pub fn status_from_objects(objects: &HashMap<String, HashMap<String, Props>>) -> BluetoothStatus {
    let mut adapters: Vec<(&String, &Props)> = objects
        .iter()
        .filter_map(|(path, ifaces)| ifaces.get(ADAPTER_INTERFACE).map(|p| (path, p)))
        .collect();
    adapters.sort_by(|a, b| a.0.cmp(b.0));
    let Some((apath, aprops)) = adapters.first() else {
        return BluetoothStatus {
            schema: 1,
            ..Default::default()
        };
    };
    let mut devices: Vec<BtDevice> = objects
        .values()
        .filter_map(|ifaces| ifaces.get(DEVICE_INTERFACE))
        .filter(|p| prop_str(p, "Adapter").as_deref() == Some(apath.as_str()))
        .filter(|p| prop_bool(p, "Paired") || prop_bool(p, "Connected"))
        .map(|p| {
            let address = prop_str(p, "Address").unwrap_or_default();
            BtDevice {
                name: prop_str(p, "Alias")
                    .or_else(|| prop_str(p, "Name"))
                    .unwrap_or_else(|| address.clone()),
                address,
                paired: prop_bool(p, "Paired"),
                connected: prop_bool(p, "Connected"),
                icon: prop_str(p, "Icon"),
            }
        })
        .collect();
    devices.sort_by(|a, b| (!a.connected, &a.name).cmp(&(!b.connected, &b.name)));
    BluetoothStatus {
        schema: 1,
        adapter: Some(Adapter {
            path: apath.to_string(),
            name: prop_str(aprops, "Alias")
                .or_else(|| prop_str(aprops, "Name"))
                .unwrap_or_default(),
            address: prop_str(aprops, "Address").unwrap_or_default(),
            powered: prop_bool(aprops, "Powered"),
        }),
        devices,
    }
}

async fn managed_objects(conn: &Connection) -> zbus::Result<HashMap<String, HashMap<String, Props>>> {
    let om = zbus::fdo::ObjectManagerProxy::builder(conn)
        .destination(BLUEZ_SERVICE)?
        .path("/")?
        .build()
        .await?;
    Ok(om
        .get_managed_objects()
        .await?
        .into_iter()
        .map(|(path, ifaces)| {
            (
                path.to_string(),
                ifaces.into_iter().map(|(i, p)| (i.to_string(), p)).collect(),
            )
        })
        .collect())
}

/// Reads adapter and device state. Errors when BlueZ is not running.
pub async fn status(conn: &Connection) -> zbus::Result<BluetoothStatus> {
    Ok(status_from_objects(&managed_objects(conn).await?))
}

/// Powers the first adapter on or off. Returns `Ok(false)` when there is no adapter.
pub async fn set_powered(conn: &Connection, enabled: bool) -> zbus::Result<bool> {
    let Some(adapter) = status(conn).await?.adapter else {
        return Ok(false);
    };
    let props = zbus::fdo::PropertiesProxy::builder(conn)
        .destination(BLUEZ_SERVICE)?
        .path(adapter.path.as_str())?
        .build()
        .await?;
    props
        .set(
            ADAPTER_INTERFACE.try_into().expect("valid interface name"),
            "Powered",
            Value::from(enabled),
        )
        .await?;
    Ok(true)
}
