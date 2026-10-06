//! Mock NetworkManager for tests and UI development: same object paths,
//! interfaces and property names as the real service.

use mados_api::network;
use std::collections::HashMap;
use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

pub fn path(p: &str) -> OwnedObjectPath {
    ObjectPath::try_from(p).unwrap().into()
}

pub struct Manager {
    pub wifi: bool,
    pub deny_wifi_change: bool,
}

#[interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        [
            "/org/freedesktop/NetworkManager/Devices/1",
            "/org/freedesktop/NetworkManager/Devices/2",
            "/org/freedesktop/NetworkManager/Devices/3",
        ]
        .into_iter()
        .map(path)
        .collect()
    }
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        self.wifi
    }
    #[zbus(property)]
    fn set_wireless_enabled(&mut self, value: bool) -> zbus::fdo::Result<()> {
        if self.deny_wifi_change {
            return Err(zbus::fdo::Error::AccessDenied("not authorized".into()));
        }
        self.wifi = value;
        Ok(())
    }
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        70
    }
    #[zbus(property)]
    fn connectivity(&self) -> u32 {
        4
    }
}

pub struct Device {
    pub iface: &'static str,
    pub kind: u32,
    pub state: u32,
    pub ip4: &'static str,
    pub active: &'static str,
}

#[interface(name = "org.freedesktop.NetworkManager.Device")]
impl Device {
    #[zbus(property)]
    fn interface(&self) -> String {
        self.iface.into()
    }
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        self.kind
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.state
    }
    #[zbus(property)]
    fn ip4_config(&self) -> OwnedObjectPath {
        path(self.ip4)
    }
    #[zbus(property)]
    fn active_connection(&self) -> OwnedObjectPath {
        path(self.active)
    }
}

pub struct Active;

#[interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn id(&self) -> String {
        "Wired connection 1".into()
    }
}

pub struct Ip4;

#[interface(name = "org.freedesktop.NetworkManager.IP4Config")]
impl Ip4 {
    #[zbus(property)]
    fn address_data(&self) -> Vec<HashMap<String, OwnedValue>> {
        let mut m = HashMap::new();
        m.insert(
            "address".to_string(),
            OwnedValue::try_from(Value::from("10.0.2.15")).unwrap(),
        );
        m.insert("prefix".to_string(), OwnedValue::from(24u32));
        vec![m]
    }
}

/// Serves a mock NetworkManager (one wired connection, one idle Wi-Fi
/// adapter, loopback) on the bus at `address`.
pub async fn serve(address: &str, deny: bool) -> zbus::Connection {
    zbus::connection::Builder::address(address)
        .unwrap()
        .name(network::NM_SERVICE)
        .unwrap()
        .serve_at(
            network::NM_PATH,
            Manager {
                wifi: true,
                deny_wifi_change: deny,
            },
        )
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager/Devices/1",
            Device {
                iface: "enp1s0",
                kind: network::DEVICE_TYPE_ETHERNET,
                state: 100,
                ip4: "/org/freedesktop/NetworkManager/IP4Config/1",
                active: "/org/freedesktop/NetworkManager/ActiveConnection/1",
            },
        )
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager/Devices/2",
            Device {
                iface: "lo",
                kind: network::DEVICE_TYPE_LOOPBACK,
                state: 100,
                ip4: "/",
                active: "/",
            },
        )
        .unwrap()
        .serve_at(
            "/org/freedesktop/NetworkManager/Devices/3",
            Device {
                iface: "wlp2s0",
                kind: network::DEVICE_TYPE_WIFI,
                state: 30,
                ip4: "/",
                active: "/",
            },
        )
        .unwrap()
        .serve_at("/org/freedesktop/NetworkManager/ActiveConnection/1", Active)
        .unwrap()
        .serve_at("/org/freedesktop/NetworkManager/IP4Config/1", Ip4)
        .unwrap()
        .build()
        .await
        .unwrap()
}
