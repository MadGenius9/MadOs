//! Structured intents: the ONLY things the assistant can ever do.
//!
//! Any language provider (rule-based today; local or cloud models later)
//! must produce one of these values. Provider output is deserialized
//! strictly into [`Intent`]; anything else is rejected. There is no
//! "run this command" intent that executes — [`Intent::RunCommand`] exists
//! only so such requests can be recognised and refused explicitly.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "intent", rename_all = "snake_case", deny_unknown_fields)]
pub enum Intent {
    SystemInfo,
    BatteryStatus,
    DiagnosePerformance,
    PowerOff,
    Reboot,
    SetWifi {
        enabled: bool,
    },
    SetBluetooth {
        enabled: bool,
    },
    SetBrightness {
        percent: u8,
    },
    CheckUpdates,
    InstallUpdate,
    ConnectDevice {
        name: String,
    },
    FindFile {
        query: String,
    },
    OpenApp {
        name: String,
    },
    InstallApp {
        name: String,
    },
    /// A request to run arbitrary commands. Always denied.
    RunCommand {
        command: String,
    },
}

impl Intent {
    /// Parses strict JSON produced by a model provider.
    pub fn from_provider_json(json: &str) -> Result<Self, String> {
        let raw: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let intent: Intent = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
        // serde's deny_unknown_fields is not enforced for unit variants of
        // internally tagged enums, so require the input to have exactly the
        // keys of the canonical encoding.
        let canonical = serde_json::to_value(&intent).map_err(|e| e.to_string())?;
        let keys = |v: &serde_json::Value| {
            let mut k: Vec<String> = v.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
            k.sort();
            k
        };
        if keys(&raw) != keys(&canonical) {
            return Err("unexpected fields in intent".into());
        }
        intent.validate()?;
        Ok(intent)
    }

    /// Value-level validation (serde only checks shape).
    pub fn validate(&self) -> Result<(), String> {
        let check = |s: &str, what: &str| {
            if s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control) {
                Err(format!("invalid {what}"))
            } else {
                Ok(())
            }
        };
        match self {
            Intent::SetBrightness { percent } if *percent > 100 => Err("brightness must be 0-100".into()),
            Intent::ConnectDevice { name } | Intent::OpenApp { name } | Intent::InstallApp { name } => {
                check(name, "name")
            }
            Intent::FindFile { query } => check(query, "query"),
            _ => Ok(()),
        }
    }
}
