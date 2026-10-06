//! Serves a mock NetworkManager on the bus in $DBUS_SYSTEM_BUS_ADDRESS, for
//! developing Settings without a real NetworkManager:
//!
//!   dbus-daemon --session --print-address --fork   # a private bus
//!   DBUS_SYSTEM_BUS_ADDRESS=<addr> cargo run -p mados-api --example mock-networkmanager &
//!   DBUS_SYSTEM_BUS_ADDRESS=<addr> cargo run -p mados-settings -- --page=network

#[path = "../tests/common/mock_nm.rs"]
mod mock_nm;

fn main() {
    let address = std::env::var("DBUS_SYSTEM_BUS_ADDRESS").expect("set DBUS_SYSTEM_BUS_ADDRESS to a private bus");
    zbus::block_on(async {
        let _conn = mock_nm::serve(&address, std::env::var_os("MOCK_NM_DENY").is_some()).await;
        eprintln!("mock NetworkManager serving on {address}");
        std::future::pending::<()>().await;
    });
}
