//! Serves a mock BlueZ on the bus in $DBUS_SYSTEM_BUS_ADDRESS (see
//! examples/mock-networkmanager.rs for usage). MOCK_BLUEZ_DENY=1 refuses
//! power changes.

#[path = "../tests/common/mock_bluez.rs"]
mod mock_bluez;

fn main() {
    let address = std::env::var("DBUS_SYSTEM_BUS_ADDRESS").expect("set DBUS_SYSTEM_BUS_ADDRESS to a private bus");
    zbus::block_on(async {
        let _conn = mock_bluez::serve(&address, std::env::var_os("MOCK_BLUEZ_DENY").is_some()).await;
        eprintln!("mock BlueZ serving on {address}");
        std::future::pending::<()>().await;
    });
}
