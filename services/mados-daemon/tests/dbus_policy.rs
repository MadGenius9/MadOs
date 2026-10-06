//! End-to-end D-Bus test on a private bus: authorization gates power actions.

use mados_api::{SystemProxy, UpdateStatus};
use mados_core::names;
use mados_daemon::{Authorizer, BoxFuture, PowerAction, PowerBackend, SystemService, UpdateBackend};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

/// A private dbus-daemon, killed on drop.
struct PrivateBus {
    child: Child,
    address: String,
}

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
        Some(Self {
            child,
            address: line.trim().to_string(),
        })
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct FixedAuth {
    allow: bool,
    seen: Mutex<Vec<String>>,
}

impl Authorizer for FixedAuth {
    fn check<'a>(&'a self, sender: &'a str, action: &'a str) -> BoxFuture<'a, Result<bool, String>> {
        self.seen.lock().unwrap().push(format!("{sender} {action}"));
        let allow = self.allow;
        Box::pin(async move { Ok(allow) })
    }
}

#[derive(Default)]
struct RecordingPower(Mutex<Vec<PowerAction>>);

impl PowerBackend for RecordingPower {
    fn execute(&self, action: PowerAction) -> BoxFuture<'_, Result<(), String>> {
        self.0.lock().unwrap().push(action);
        Box::pin(async { Ok(()) })
    }
}

struct NoUpdates;

impl UpdateBackend for NoUpdates {
    fn status(&self) -> BoxFuture<'_, UpdateStatus> {
        Box::pin(async {
            UpdateStatus {
                schema: 1,
                available: false,
                message: Some("test".into()),
                ..Default::default()
            }
        })
    }
}

async fn serve(
    bus: &PrivateBus,
    auth: Arc<FixedAuth>,
    power: Arc<RecordingPower>,
) -> (zbus::Connection, zbus::Connection) {
    let service = SystemService::new(auth, power, Arc::new(NoUpdates));
    let server = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name(names::SYSTEM_BUS_NAME)
        .unwrap()
        .serve_at(names::SYSTEM_OBJECT_PATH, service)
        .unwrap()
        .build()
        .await
        .unwrap();
    let client = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    (server, client)
}

#[test]
fn power_requires_authorization() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("SKIP: dbus-daemon not available");
        return;
    };
    zbus::block_on(async {
        let auth = Arc::new(FixedAuth {
            allow: false,
            seen: Mutex::default(),
        });
        let power = Arc::new(RecordingPower::default());
        let (_server, client) = serve(&bus, auth.clone(), power.clone()).await;
        let proxy = SystemProxy::new(&client).await.unwrap();

        let err = proxy.reboot().await.expect_err("reboot must be denied");
        match err {
            zbus::Error::MethodError(name, _, _) => {
                assert_eq!(name.as_str(), "org.mados.System1.Error.NotAuthorized")
            }
            other => panic!("unexpected error {other:?}"),
        }
        assert!(power.0.lock().unwrap().is_empty(), "denied call must not reach logind");
        let seen = auth.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 1);
        // The subject is the caller's unique name, not the service's.
        assert!(seen[0].starts_with(client.unique_name().unwrap().as_str()), "{seen:?}");
        assert!(seen[0].ends_with(names::ACTION_POWER));
    });
}

#[test]
fn authorized_power_and_read_methods() {
    let Some(bus) = PrivateBus::start() else {
        eprintln!("SKIP: dbus-daemon not available");
        return;
    };
    zbus::block_on(async {
        let auth = Arc::new(FixedAuth {
            allow: true,
            seen: Mutex::default(),
        });
        let power = Arc::new(RecordingPower::default());
        let (_server, client) = serve(&bus, auth, power.clone()).await;
        let proxy = SystemProxy::new(&client).await.unwrap();

        proxy.power_off().await.unwrap();
        proxy.reboot().await.unwrap();
        assert_eq!(
            *power.0.lock().unwrap(),
            vec![PowerAction::PowerOff, PowerAction::Reboot]
        );

        let info: mados_core::SystemInfo = serde_json::from_str(&proxy.get_system_info().await.unwrap()).unwrap();
        assert_eq!(info.product_version, mados_core::Product::load().version.full());
        let upd: UpdateStatus = serde_json::from_str(&proxy.get_update_status().await.unwrap()).unwrap();
        assert!(!upd.available);
        assert_eq!(
            proxy.version().await.unwrap(),
            mados_core::Product::load().version.full()
        );
        assert_eq!(proxy.api_level().await.unwrap(), mados_daemon::API_LEVEL);
    });
}
