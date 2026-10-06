//! mados-daemon entry point.
//!
//! Usage: mados-daemon [--session]
//!
//! `--session` serves on the session bus for development. In that mode every
//! privileged method is denied (there is no system polkit subject), so it is
//! safe to run unprivileged.

use mados_core::names;
use mados_core::{log_error, log_info, Product};
use mados_daemon::backends::{BootcUpdates, LogindPower, PolkitAuthorizer};
use mados_daemon::{DenyAll, SystemService};
use std::sync::Arc;

fn main() {
    if let Err(e) = run() {
        log_error!("fatal: {e}");
        std::process::exit(1);
    }
}

fn run() -> zbus::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let session = match args.as_slice() {
        [] => false,
        [a] if a == "--session" => true,
        [a] if a == "--version" => {
            println!("mados-daemon {}", Product::load().version.full());
            return Ok(());
        }
        _ => {
            eprintln!("usage: mados-daemon [--session|--version]");
            std::process::exit(2);
        }
    };

    zbus::block_on(async move {
        let system = zbus::Connection::system();
        let service = if session {
            // Development mode: power/update backends are never reachable
            // because DenyAll rejects every privileged call.
            let sys = system.await.ok();
            let power: Arc<dyn mados_daemon::PowerBackend> = match sys {
                Some(c) => Arc::new(LogindPower::new(c)),
                None => Arc::new(NoPower),
            };
            SystemService::new(Arc::new(DenyAll), power, Arc::new(BootcUpdates::default()))
        } else {
            let sys = system.await?;
            SystemService::new(
                Arc::new(PolkitAuthorizer::new(sys.clone())),
                Arc::new(LogindPower::new(sys)),
                Arc::new(BootcUpdates::default()),
            )
        };
        let builder = if session {
            zbus::connection::Builder::session()?
        } else {
            zbus::connection::Builder::system()?
        };
        let _conn = builder
            .name(names::SYSTEM_BUS_NAME)?
            .serve_at(names::SYSTEM_OBJECT_PATH, service)?
            .build()
            .await?;
        log_info!(
            "serving {} on the {} bus (version {})",
            names::SYSTEM_BUS_NAME,
            if session { "session" } else { "system" },
            Product::load().version.full()
        );
        std::future::pending::<()>().await;
        Ok(())
    })
}

struct NoPower;

impl mados_daemon::PowerBackend for NoPower {
    fn execute(&self, _: mados_daemon::PowerAction) -> mados_daemon::BoxFuture<'_, Result<(), String>> {
        Box::pin(async { Err("no system bus".into()) })
    }
}
