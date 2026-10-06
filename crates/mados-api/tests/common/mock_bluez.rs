//! Mock BlueZ for tests and UI development: one adapter (hci0) with a
//! connected headset, a paired-but-disconnected mouse and an unpaired
//! nearby device, published through an ObjectManager at "/" like bluetoothd.

use zbus::interface;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

pub struct Adapter {
    pub powered: bool,
    pub deny_power_change: bool,
}

#[interface(name = "org.bluez.Adapter1")]
impl Adapter {
    #[zbus(property)]
    fn address(&self) -> String {
        "00:1A:7D:DA:71:13".into()
    }
    #[zbus(property)]
    fn alias(&self) -> String {
        "mados-vm".into()
    }
    #[zbus(property)]
    fn powered(&self) -> bool {
        self.powered
    }
    #[zbus(property)]
    fn set_powered(&mut self, value: bool) -> zbus::fdo::Result<()> {
        if self.deny_power_change {
            return Err(zbus::fdo::Error::AccessDenied("not authorized".into()));
        }
        self.powered = value;
        Ok(())
    }
}

pub struct Device {
    pub alias: &'static str,
    pub address: &'static str,
    pub paired: bool,
    pub connected: bool,
    pub icon: &'static str,
}

#[interface(name = "org.bluez.Device1")]
impl Device {
    #[zbus(property)]
    fn alias(&self) -> String {
        self.alias.into()
    }
    #[zbus(property)]
    fn address(&self) -> String {
        self.address.into()
    }
    #[zbus(property)]
    fn paired(&self) -> bool {
        self.paired
    }
    #[zbus(property)]
    fn connected(&self) -> bool {
        self.connected
    }
    #[zbus(property)]
    fn icon(&self) -> String {
        self.icon.into()
    }
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        ObjectPath::try_from("/org/bluez/hci0").unwrap().into()
    }
}

/// Serves the mock on the bus at `address` under the name `org.bluez`.
pub async fn serve(address: &str, deny: bool) -> zbus::Connection {
    let dev = |alias, address, paired, connected, icon| Device {
        alias,
        address,
        paired,
        connected,
        icon,
    };
    zbus::connection::Builder::address(address)
        .unwrap()
        .name(mados_api::bluetooth::BLUEZ_SERVICE)
        .unwrap()
        .serve_at("/", zbus::fdo::ObjectManager)
        .unwrap()
        .serve_at(
            "/org/bluez/hci0",
            Adapter {
                powered: true,
                deny_power_change: deny,
            },
        )
        .unwrap()
        .serve_at(
            "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_01",
            dev("Headphones", "AA:BB:CC:DD:EE:01", true, true, "audio-headset"),
        )
        .unwrap()
        .serve_at(
            "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_02",
            dev("Mouse", "AA:BB:CC:DD:EE:02", true, false, "input-mouse"),
        )
        .unwrap()
        .serve_at(
            "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_03",
            dev("Neighbour TV", "AA:BB:CC:DD:EE:03", false, false, "video-display"),
        )
        .unwrap()
        .build()
        .await
        .unwrap()
}
