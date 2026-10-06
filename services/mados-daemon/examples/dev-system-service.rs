//! DEVELOPMENT ONLY: serves the real `SystemService` on the bus in
//! $DBUS_SYSTEM_BUS_ADDRESS with an allow-all authorizer, no-op power
//! actions and a simulated bootc, so the Settings UI can be exercised
//! without a bootc system. Not installed in any image (examples are never
//! staged; see scripts/stage-system.py BINARIES).
//!
//!   addr=$(dbus-daemon --session --print-address=1 --fork)
//!   DBUS_SYSTEM_BUS_ADDRESS=$addr cargo run -p mados-daemon --example dev-system-service &
//!   DBUS_SYSTEM_BUS_ADDRESS=$addr cargo run -p mados-settings -- --page=updates

use mados_api::{Deployment, UpdateStatus};
use mados_core::names;
use mados_daemon::{Authorizer, BoxFuture, PowerAction, PowerBackend, SystemService, UpdateBackend};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct AllowAll;

impl Authorizer for AllowAll {
    fn check<'a>(&'a self, _: &'a str, _: &'a str) -> BoxFuture<'a, Result<bool, String>> {
        Box::pin(async { Ok(true) })
    }
}

struct LogPower;

impl PowerBackend for LogPower {
    fn execute(&self, action: PowerAction) -> BoxFuture<'_, Result<(), String>> {
        eprintln!("dev-system-service: would {action:?}");
        Box::pin(async { Ok(()) })
    }
}

fn dep(version: &str, digest: &str) -> Deployment {
    Deployment {
        image: Some("ghcr.io/madgenius9/mados:dev".into()),
        version: Some(version.into()),
        timestamp: Some("2026-10-06T00:00:00Z".into()),
        digest: Some(digest.into()),
    }
}

/// In-memory stand-in for bootc's deployment state machine.
struct SimulatedBootc(Mutex<UpdateStatus>);

impl UpdateBackend for SimulatedBootc {
    fn status(&self) -> BoxFuture<'_, UpdateStatus> {
        let s = self.0.lock().unwrap().clone();
        Box::pin(async move { s })
    }
    fn check(&self) -> BoxFuture<'_, Result<UpdateStatus, String>> {
        let mut s = self.0.lock().unwrap();
        s.cached_update = Some(dep("0.1.1-dev", "sha256:new"));
        let s = s.clone();
        Box::pin(async move { Ok(s) })
    }
    fn stage(&self) -> BoxFuture<'_, Result<String, String>> {
        Box::pin(async move {
            blocking::unblock(|| std::thread::sleep(Duration::from_secs(2))).await;
            let mut s = self.0.lock().unwrap();
            s.staged = s.cached_update.take();
            Ok("Update installed. Restart to use it; the current version stays available for rollback.".into())
        })
    }
    fn rollback(&self) -> BoxFuture<'_, Result<String, String>> {
        Box::pin(async { Ok("Rollback prepared. Restart to boot the previous version.".into()) })
    }
}

fn main() {
    let address = std::env::var("DBUS_SYSTEM_BUS_ADDRESS").expect("set DBUS_SYSTEM_BUS_ADDRESS to a private bus");
    let updates = SimulatedBootc(Mutex::new(UpdateStatus {
        schema: 1,
        available: true,
        booted: Some(dep("0.1.0-dev", "sha256:cur")),
        rollback: Some(dep("0.0.9-dev", "sha256:old")),
        ..Default::default()
    }));
    zbus::block_on(async {
        let service = SystemService::new(Arc::new(AllowAll), Arc::new(LogPower), Arc::new(updates));
        let _conn = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .name(names::SYSTEM_BUS_NAME)
            .unwrap()
            .serve_at(names::SYSTEM_OBJECT_PATH, service)
            .unwrap()
            .build()
            .await
            .unwrap();
        eprintln!("dev-system-service: serving {} on {address}", names::SYSTEM_BUS_NAME);
        std::future::pending::<()>().await;
    });
}
