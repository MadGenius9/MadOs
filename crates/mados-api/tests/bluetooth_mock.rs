//! bluetooth::status / set_powered against a mock BlueZ on a private bus.

use mados_api::bluetooth;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

mod common;
use common::mock_bluez;

struct PrivateBus(Child, String);

impl PrivateBus {
    fn start() -> Option<Self> {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut line = String::new();
        BufReader::new(child.stdout.take()?).read_line(&mut line).ok()?;
        Some(Self(child, line.trim().to_string()))
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn client(bus: &PrivateBus) -> zbus::Connection {
    zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .build()
        .await
        .unwrap()
}

#[test]
fn reads_adapter_and_devices_and_powers_off() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("SKIP: dbus-daemon not available");
        return;
    };
    zbus::block_on(async {
        let _server = mock_bluez::serve(&bus.1, false).await;
        let conn = client(&bus).await;
        let s = bluetooth::status(&conn).await.unwrap();
        let a = s.adapter.clone().expect("adapter");
        assert_eq!(
            (a.name.as_str(), a.address.as_str(), a.powered),
            ("mados-vm", "00:1A:7D:DA:71:13", true)
        );
        let names: Vec<&str> = s.devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            ["Headphones", "Mouse"],
            "unpaired devices hidden; connected first"
        );
        assert!(s.devices[0].connected && s.devices[0].paired);
        assert_eq!(s.devices[0].icon.as_deref(), Some("audio-headset"));
        assert!(!s.devices[1].connected);

        assert!(bluetooth::set_powered(&conn, false).await.unwrap());
        let conn2 = client(&bus).await;
        assert!(!bluetooth::status(&conn2).await.unwrap().adapter.unwrap().powered);
    });
}

#[test]
fn refusal_and_missing_service() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("SKIP: dbus-daemon not available");
        return;
    };
    zbus::block_on(async {
        let conn = client(&bus).await;
        assert!(
            bluetooth::status(&conn).await.is_err(),
            "no bluetoothd -> error, not a fake status"
        );
        let _server = mock_bluez::serve(&bus.1, true).await;
        let err = bluetooth::set_powered(&conn, false).await.expect_err("refused");
        assert!(err.to_string().contains("not authorized"), "{err}");
    });
}
