//! Serves mock NetworkManager, BlueZ, AccountsService and logind on the bus
//! in $DBUS_SYSTEM_BUS_ADDRESS, for developing Settings without a full system:
//!
//!   addr=$(dbus-daemon --session --print-address=1 --fork)
//!   DBUS_SYSTEM_BUS_ADDRESS=$addr cargo run -p mados-api --example mock-system-services &
//!   DBUS_SYSTEM_BUS_ADDRESS=$addr cargo run -p mados-settings -- --page=network
//!
//! MOCK_DENY=1 makes every mock refuse changes, like polkit would.

#[path = "../tests/common/mod.rs"]
mod common;

fn main() {
    let address = std::env::var("DBUS_SYSTEM_BUS_ADDRESS").expect("set DBUS_SYSTEM_BUS_ADDRESS to a private bus");
    let deny = std::env::var_os("MOCK_DENY").is_some();
    zbus::block_on(async {
        let _nm = common::mock_nm::serve(&address, deny).await;
        let _bt = common::mock_bluez::serve(&address, deny).await;
        let _acc = common::mock_accounts::serve(&address).await;
        let _logind = common::mock_logind::serve(&address, deny).await;
        eprintln!("mock system services serving on {address}");
        std::future::pending::<()>().await;
    });
}
