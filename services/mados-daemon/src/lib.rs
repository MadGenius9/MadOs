//! `mados-daemon`: the MadOS system service.
//!
//! Exposes `org.mados.System1` on the system bus. Design rules:
//!
//! * No shell. External programs are run by absolute path with fixed
//!   arguments (`bootc status|upgrade [--check]|rollback`).
//! * Every privileged method authorizes the **calling** D-Bus peer with
//!   polkit before acting; the daemon's own root identity is never used as
//!   the authorization subject.
//! * Backends sit behind traits so the policy logic is testable without a
//!   real system bus, polkit or logind.

pub mod backends;

use mados_api::UpdateStatus;
use mados_core::names;
use mados_core::sysinfo::{Probe, SessionEnv};
use mados_core::{log_info, log_notice, log_warn, Product};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{fdo, interface};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Increment when methods are added to org.mados.System1.
/// 2: CheckForUpdate, StartUpdate, StartRollback, UpdateJobFinished, Busy.
pub const API_LEVEL: u32 = 2;

/// Decides whether a D-Bus peer may perform a polkit action.
pub trait Authorizer: Send + Sync {
    fn check<'a>(&'a self, sender: &'a str, action: &'a str) -> BoxFuture<'a, Result<bool, String>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    PowerOff,
    Reboot,
}

pub trait PowerBackend: Send + Sync {
    fn execute(&self, action: PowerAction) -> BoxFuture<'_, Result<(), String>>;
}

pub trait UpdateBackend: Send + Sync {
    fn status(&self) -> BoxFuture<'_, UpdateStatus>;
    /// Fetch update metadata; returns the refreshed status.
    fn check(&self) -> BoxFuture<'_, Result<UpdateStatus, String>>;
    /// Download and stage the update for the next boot.
    fn stage(&self) -> BoxFuture<'_, Result<String, String>>;
    /// Make the rollback deployment the default for the next boot.
    fn rollback(&self) -> BoxFuture<'_, Result<String, String>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateJob {
    Update,
    Rollback,
}

impl UpdateJob {
    fn name(self) -> &'static str {
        match self {
            UpdateJob::Update => "update",
            UpdateJob::Rollback => "rollback",
        }
    }
}

/// Authorizer that denies everything. Used in `--session` development mode,
/// where there is no system polkit subject to check against.
pub struct DenyAll;

impl Authorizer for DenyAll {
    fn check<'a>(&'a self, _: &'a str, _: &'a str) -> BoxFuture<'a, Result<bool, String>> {
        Box::pin(async { Ok(false) })
    }
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.mados.System1.Error")]
pub enum SystemError {
    #[zbus(error)]
    ZBus(zbus::Error),
    /// The caller is not authorized for this action.
    NotAuthorized(String),
    /// The operation was attempted and failed.
    Failed(String),
    /// Another update or rollback job is running.
    Busy(String),
}

pub struct SystemService {
    product: Product,
    probe: Probe,
    authorizer: Arc<dyn Authorizer>,
    power: Arc<dyn PowerBackend>,
    updates: Arc<dyn UpdateBackend>,
    /// One update/rollback job at a time.
    busy: Arc<AtomicBool>,
}

impl SystemService {
    pub fn new(authorizer: Arc<dyn Authorizer>, power: Arc<dyn PowerBackend>, updates: Arc<dyn UpdateBackend>) -> Self {
        Self {
            product: Product::load(),
            probe: Probe::default(),
            authorizer,
            power,
            updates,
            busy: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn authorize(&self, hdr: &Header<'_>, action: &str) -> Result<String, SystemError> {
        let sender = hdr
            .sender()
            .map(|s| s.to_string())
            .ok_or_else(|| SystemError::NotAuthorized("message has no sender".into()))?;
        match self.authorizer.check(&sender, action).await {
            Ok(true) => Ok(sender),
            Ok(false) => {
                log_notice!("denied {action} for {sender}");
                Err(SystemError::NotAuthorized(format!("not authorized for {action}")))
            }
            Err(e) => {
                log_warn!("authorization check for {action} by {sender} failed: {e}");
                Err(SystemError::NotAuthorized(format!("authorization check failed: {e}")))
            }
        }
    }

    async fn power(&self, hdr: &Header<'_>, action: PowerAction) -> Result<(), SystemError> {
        let sender = self.authorize(hdr, names::ACTION_POWER).await?;
        log_info!("{action:?} requested by {sender}");
        self.power.execute(action).await.map_err(SystemError::Failed)
    }

    /// Authorizes, claims the job slot and runs `job` in the background; the
    /// result is announced with UpdateJobFinished and the Busy property.
    async fn start_job(&self, hdr: &Header<'_>, conn: &zbus::Connection, job: UpdateJob) -> Result<(), SystemError> {
        let sender = self.authorize(hdr, names::ACTION_UPDATES_APPLY).await?;
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(SystemError::Busy("an update or rollback is already running".into()));
        }
        log_info!("{} started by {sender}", job.name());
        let (updates, busy, task_conn) = (self.updates.clone(), self.busy.clone(), conn.clone());
        let emitter = SignalEmitter::new(conn, names::SYSTEM_OBJECT_PATH)?.into_owned();
        emit_busy_changed(conn, &emitter).await;
        conn.executor()
            .spawn(
                async move {
                    let result = match job {
                        UpdateJob::Update => updates.stage().await,
                        UpdateJob::Rollback => updates.rollback().await,
                    };
                    busy.store(false, Ordering::SeqCst);
                    let (ok, message) = match result {
                        Ok(m) => {
                            log_notice!("{} finished: {m}", job.name());
                            (true, m)
                        }
                        Err(e) => {
                            log_warn!("{} failed: {e}", job.name());
                            (false, e)
                        }
                    };
                    emit_busy_changed(&task_conn, &emitter).await;
                    if let Err(e) = SystemService::update_job_finished(&emitter, job.name(), ok, &message).await {
                        log_warn!("cannot emit UpdateJobFinished: {e}");
                    }
                },
                "mados-update-job",
            )
            .detach();
        Ok(())
    }
}

async fn emit_busy_changed(conn: &zbus::Connection, emitter: &SignalEmitter<'_>) {
    if let Ok(iface) = conn
        .object_server()
        .interface::<_, SystemService>(names::SYSTEM_OBJECT_PATH)
        .await
    {
        let _ = iface.get().await.busy_changed(emitter).await;
    }
}

#[interface(name = "org.mados.System1")]
impl SystemService {
    async fn get_system_info(&self) -> fdo::Result<String> {
        let probe = self.probe.clone();
        let product = self.product.clone();
        // Filesystem probing is blocking I/O; keep it off the bus executor.
        let info = blocking::unblock(move || probe.collect(&product, &SessionEnv::default())).await;
        serde_json::to_string(&info).map_err(|e| fdo::Error::Failed(e.to_string()))
    }

    async fn get_update_status(&self) -> fdo::Result<String> {
        let status = self.updates.status().await;
        serde_json::to_string(&status).map_err(|e| fdo::Error::Failed(e.to_string()))
    }

    async fn check_for_update(&self, #[zbus(header)] hdr: Header<'_>) -> Result<String, SystemError> {
        let sender = self.authorize(&hdr, names::ACTION_UPDATES_CHECK).await?;
        log_info!("update check requested by {sender}");
        let status = self.updates.check().await.map_err(SystemError::Failed)?;
        serde_json::to_string(&status).map_err(|e| SystemError::Failed(e.to_string()))
    }

    async fn start_update(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &zbus::Connection,
    ) -> Result<(), SystemError> {
        self.start_job(&hdr, conn, UpdateJob::Update).await
    }

    async fn start_rollback(
        &self,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &zbus::Connection,
    ) -> Result<(), SystemError> {
        self.start_job(&hdr, conn, UpdateJob::Rollback).await
    }

    #[zbus(signal)]
    async fn update_job_finished(
        emitter: &SignalEmitter<'_>,
        operation: &str,
        success: bool,
        message: &str,
    ) -> zbus::Result<()>;

    #[zbus(property)]
    fn busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    async fn power_off(&self, #[zbus(header)] hdr: Header<'_>) -> Result<(), SystemError> {
        self.power(&hdr, PowerAction::PowerOff).await
    }

    async fn reboot(&self, #[zbus(header)] hdr: Header<'_>) -> Result<(), SystemError> {
        self.power(&hdr, PowerAction::Reboot).await
    }

    #[zbus(property)]
    fn version(&self) -> String {
        self.product.version.full()
    }

    #[zbus(property)]
    fn api_level(&self) -> u32 {
        API_LEVEL
    }
}
