//! Language providers turn free text into a structured [`Intent`].
//!
//! Providers are interchangeable (rules, local model, cloud model). They are
//! *interpreters only*: their output passes through strict deserialization
//! ([`Intent::from_provider_json`]) and the policy in [`crate::policy`]; a
//! provider can never cause execution directly.
//!
//! 0.1 ships only the offline `rules` provider. Model providers (OpenAI,
//! Anthropic, Gemini, local) are future work; credentials must come from the
//! user's keyring at runtime and are never stored in this repository or in
//! plain-text config.

use crate::intent::Intent;
use serde::Deserialize;
use std::path::Path;

pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    /// True when requests are interpreted on this device (nothing is sent
    /// over the network). Shown to the user in Settings → Privacy.
    fn is_local(&self) -> bool;
    fn interpret(&self, text: &str) -> Option<Intent>;
}

pub struct RulesProvider;

impl Provider for RulesProvider {
    fn name(&self) -> &str {
        "rules"
    }
    fn is_local(&self) -> bool {
        true
    }
    fn interpret(&self, text: &str) -> Option<Intent> {
        crate::rules::parse(text)
    }
}

/// `~/.config/mados/assistant.toml`
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub provider: Option<String>,
}

impl Config {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn default_path() -> Option<std::path::PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))?;
        Some(base.join("mados/assistant.toml"))
    }
}

/// Builds the configured provider. Unknown or unavailable providers fall back
/// to `rules` and say so, rather than failing.
pub fn from_config(cfg: &Config) -> (Box<dyn Provider>, Option<String>) {
    match cfg.provider.as_deref() {
        None | Some("rules") => (Box::new(RulesProvider), None),
        Some(other) => (
            Box::new(RulesProvider),
            Some(format!(
                "provider {other:?} is not available in this build; using \"rules\""
            )),
        ),
    }
}
