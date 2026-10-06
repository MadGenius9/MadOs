//! MadOS system API contracts.
//!
//! GUI applications and the assistant talk to the system **only** through
//! these D-Bus interfaces. They never run privileged commands themselves.
//!
//! * `org.mados.System1` (system bus, `mados-daemon`): system information,
//!   update status, power actions. Privileged methods are authorized with
//!   polkit against the *calling* process.
//! * `org.mados.Assistant1` (session bus, `mados-ai`): natural-language
//!   requests → structured intents → policy → confirmation → execution.
//!
//! Complex results are JSON strings with an explicit `schema` field, so the
//! wire format can evolve without breaking D-Bus signatures.

use serde::{Deserialize, Serialize};
use zbus::proxy;

pub mod bluetooth;
pub mod display;
pub mod network;

pub use mados_core::names;

/// Client proxy for `org.mados.System1`.
#[proxy(
    interface = "org.mados.System1",
    default_service = "org.mados.System1",
    default_path = "/org/mados/System1"
)]
pub trait System {
    /// JSON-encoded [`mados_core::SystemInfo`] (session fields empty: the
    /// daemon has no session; clients fill them from their environment).
    fn get_system_info(&self) -> zbus::Result<String>;

    /// JSON-encoded [`UpdateStatus`].
    fn get_update_status(&self) -> zbus::Result<String>;

    /// Asks the update source whether a newer image exists, then returns the
    /// JSON [`UpdateStatus`] (`cached_update` set when one is available).
    /// polkit `org.mados.system.updates.check`.
    fn check_for_update(&self) -> zbus::Result<String>;

    /// Starts downloading and staging the update as a new deployment for the
    /// next boot (never reboots). Completion: `UpdateJobFinished("update", …)`.
    /// polkit `org.mados.system.updates.apply`.
    fn start_update(&self) -> zbus::Result<()>;

    /// Makes the rollback deployment the default for the next boot.
    /// Completion: `UpdateJobFinished("rollback", …)`.
    /// polkit `org.mados.system.updates.apply`.
    fn start_rollback(&self) -> zbus::Result<()>;

    /// Emitted when a job started by StartUpdate/StartRollback ends.
    #[zbus(signal)]
    fn update_job_finished(&self, operation: &str, success: bool, message: &str) -> zbus::Result<()>;

    /// True while an update or rollback job runs.
    #[zbus(property)]
    fn busy(&self) -> zbus::Result<bool>;

    /// Power off via logind. Requires polkit `org.mados.system.power`.
    fn power_off(&self) -> zbus::Result<()>;

    /// Reboot via logind. Requires polkit `org.mados.system.power`.
    fn reboot(&self) -> zbus::Result<()>;

    /// Product version (e.g. `0.1.0-dev`).
    #[zbus(property)]
    fn version(&self) -> zbus::Result<String>;

    /// API level, incremented when methods are added.
    #[zbus(property)]
    fn api_level(&self) -> zbus::Result<u32>;
}

/// Client proxy for `org.mados.Assistant1`.
#[proxy(
    interface = "org.mados.Assistant1",
    default_service = "org.mados.Assistant1",
    default_path = "/org/mados/Assistant1"
)]
pub trait Assistant {
    /// Interpret a request. Returns JSON [`AssistantReply`].
    fn ask(&self, text: &str) -> zbus::Result<String>;

    /// Confirm a pending request by id. Returns JSON [`AssistantReply`].
    fn confirm(&self, request_id: &str) -> zbus::Result<String>;

    /// Cancel a pending request.
    fn cancel(&self, request_id: &str) -> zbus::Result<()>;

    /// Name of the active language provider (e.g. `rules`).
    #[zbus(property)]
    fn provider(&self) -> zbus::Result<String>;

    /// True when requests are processed on this device only.
    #[zbus(property)]
    fn local(&self) -> zbus::Result<bool>;
}

/// Deployment state reported by bootc (subset we display).
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Deployment {
    pub image: Option<String>,
    pub version: Option<String>,
    pub timestamp: Option<String>,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct UpdateStatus {
    pub schema: u32,
    /// False when the system is not bootc-managed or bootc is unavailable.
    pub available: bool,
    pub booted: Option<Deployment>,
    pub staged: Option<Deployment>,
    pub rollback: Option<Deployment>,
    /// Newer image found by the last update check, if any.
    #[serde(default)]
    pub cached_update: Option<Deployment>,
    /// Human-readable reason when `available` is false.
    pub message: Option<String>,
}

/// Outcome of an assistant request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyStatus {
    /// Executed; `message` holds the answer.
    Done,
    /// Recognised and permitted, but needs explicit user confirmation.
    NeedsConfirmation,
    /// Recognised but not implemented in this build.
    Unsupported,
    /// Not understood.
    NotUnderstood,
    /// Refused by policy.
    Denied,
    /// Execution failed.
    Error,
    /// Cancelled by the user.
    Cancelled,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AssistantReply {
    pub schema: u32,
    pub status: ReplyStatus,
    /// Stable capability id, e.g. `power.reboot`.
    pub capability: Option<String>,
    /// Present when `status == NeedsConfirmation`.
    pub request_id: Option<String>,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_status_wire_names() {
        let r = AssistantReply {
            schema: 1,
            status: ReplyStatus::NeedsConfirmation,
            capability: Some("power.reboot".into()),
            request_id: Some("1".into()),
            message: "Reboot now?".into(),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"needs_confirmation\""), "{j}");
        assert_eq!(serde_json::from_str::<AssistantReply>(&j).unwrap(), r);
    }
}
