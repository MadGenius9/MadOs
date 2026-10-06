//! Capability registry and policy.
//!
//! Every intent maps to exactly one capability with a fixed risk level.
//! Policy (0.1):
//!
//! * `ReadOnly`: executed immediately.
//! * `Settings`: requires explicit user confirmation.
//! * `Privileged`: requires explicit user confirmation, and is then
//!   authorized again by polkit inside mados-daemon against the user's
//!   identity. The assistant holds no privileges.
//! * `Forbidden`: always denied.
//!
//! Capabilities not yet implemented report `Unsupported` honestly.

use crate::intent::Intent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    ReadOnly,
    Settings,
    Privileged,
    Forbidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    pub id: &'static str,
    pub risk: Risk,
    pub implemented: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Execute,
    Confirm,
    Unsupported,
    Deny,
}

pub fn capability(intent: &Intent) -> Capability {
    use Risk::*;
    let (id, risk, implemented) = match intent {
        Intent::SystemInfo => ("system.info", ReadOnly, true),
        Intent::BatteryStatus => ("power.battery", ReadOnly, true),
        Intent::DiagnosePerformance => ("system.diagnose", ReadOnly, true),
        Intent::PowerOff => ("power.off", Privileged, true),
        Intent::Reboot => ("power.reboot", Privileged, true),
        Intent::SetWifi { .. } => ("network.wifi.set", Settings, true),
        Intent::SetBluetooth { .. } => ("bluetooth.set", Settings, true),
        Intent::SetBrightness { .. } => ("display.brightness.set", Settings, true),
        // Fetches metadata only; authorized by polkit updates.check.
        Intent::CheckUpdates => ("updates.check", ReadOnly, true),
        // Admin authentication again in mados-daemon (polkit updates.apply).
        Intent::InstallUpdate => ("updates.install", Privileged, true),
        Intent::ConnectDevice { .. } => ("bluetooth.connect", Settings, false),
        Intent::FindFile { .. } => ("files.search", ReadOnly, false),
        Intent::OpenApp { .. } => ("apps.open", Settings, false),
        Intent::InstallApp { .. } => ("apps.install", Privileged, false),
        Intent::RunCommand { .. } => ("system.command", Forbidden, false),
    };
    Capability { id, risk, implemented }
}

pub fn decide(intent: &Intent) -> Decision {
    let cap = capability(intent);
    match (cap.risk, cap.implemented) {
        (Risk::Forbidden, _) => Decision::Deny,
        (_, false) => Decision::Unsupported,
        (Risk::ReadOnly, true) => Decision::Execute,
        (Risk::Settings | Risk::Privileged, true) => Decision::Confirm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_table() {
        assert_eq!(decide(&Intent::BatteryStatus), Decision::Execute);
        assert_eq!(decide(&Intent::Reboot), Decision::Confirm);
        assert_eq!(decide(&Intent::SetBluetooth { enabled: true }), Decision::Confirm);
        assert_eq!(decide(&Intent::InstallApp { name: "x".into() }), Decision::Unsupported);
        assert_eq!(decide(&Intent::RunCommand { command: "ls".into() }), Decision::Deny);
        assert_eq!(decide(&Intent::InstallUpdate), Decision::Confirm);
    }

    #[test]
    fn nothing_privileged_executes_without_confirmation() {
        let all = [
            Intent::SystemInfo,
            Intent::BatteryStatus,
            Intent::DiagnosePerformance,
            Intent::PowerOff,
            Intent::Reboot,
            Intent::SetWifi { enabled: true },
            Intent::SetBluetooth { enabled: true },
            Intent::SetBrightness { percent: 50 },
            Intent::CheckUpdates,
            Intent::InstallUpdate,
            Intent::ConnectDevice { name: "x".into() },
            Intent::FindFile { query: "x".into() },
            Intent::OpenApp { name: "x".into() },
            Intent::InstallApp { name: "x".into() },
            Intent::RunCommand { command: "x".into() },
        ];
        for i in &all {
            if decide(i) == Decision::Execute {
                assert_eq!(capability(i).risk, Risk::ReadOnly, "{i:?}");
            }
        }
    }
}
