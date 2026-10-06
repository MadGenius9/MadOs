//! network::status / set_wifi_enabled against a mock NetworkManager on a
//! private bus (same object paths, interfaces and property names as the real one).

use mados_api::network;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

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

mod common;
use common::mock_nm;

async fn client(bus: &PrivateBus) -> zbus::Connection {
    zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .build()
        .await
        .unwrap()
}

#[test]
fn reads_status_and_toggles_wifi() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("SKIP: dbus-daemon not available");
        return;
    };
    zbus::block_on(async {
        let _server = mock_nm::serve(&bus.1, false).await;
        let conn = client(&bus).await;
        let s = network::status(&conn).await.unwrap();
        assert_eq!(s.state, "connected");
        assert_eq!(s.connectivity, "full");
        assert!(s.wifi_enabled && s.wifi_hardware_enabled);
        assert_eq!(s.devices.len(), 2, "loopback must be hidden: {:?}", s.devices);
        let eth = &s.devices[0];
        assert_eq!(eth.interface, "enp1s0");
        assert_eq!(eth.kind, "ethernet");
        assert!(eth.connected);
        assert_eq!(eth.connection.as_deref(), Some("Wired connection 1"));
        assert_eq!(eth.ipv4, vec!["10.0.2.15/24".to_string()]);
        let wifi = &s.devices[1];
        assert_eq!(
            (wifi.kind.as_str(), wifi.state.as_str(), wifi.connected),
            ("wifi", "disconnected", false)
        );
        assert!(wifi.ipv4.is_empty() && wifi.connection.is_none());
        assert!(s.has_wifi_device());

        network::set_wifi_enabled(&conn, false).await.unwrap();
        // Fresh connection: no cached property values.
        let conn2 = client(&bus).await;
        assert!(!network::status(&conn2).await.unwrap().wifi_enabled);
    });
}

#[test]
fn wifi_change_refusal_is_reported() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("SKIP: dbus-daemon not available");
        return;
    };
    zbus::block_on(async {
        let _server = mock_nm::serve(&bus.1, true).await;
        let conn = client(&bus).await;
        let err = network::set_wifi_enabled(&conn, false)
            .await
            .expect_err("must be refused");
        assert!(err.to_string().contains("not authorized"), "{err}");
        assert!(network::status(&conn).await.unwrap().wifi_enabled);
    });
}
