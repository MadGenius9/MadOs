//! MadOS assistant (`mados-ai`).
//!
//! Pipeline: request text → [`provider`] → structured [`intent::Intent`] →
//! [`policy`] (execute / confirm / unsupported / deny) → [`ops`] (system
//! D-Bus APIs) → reply. The assistant runs unprivileged in the user session
//! and has no way to execute arbitrary commands.
//! See docs/architecture/ADR-003-mados-ai.md.

pub mod assistant;
pub mod intent;
pub mod ops;
pub mod policy;
pub mod provider;
pub mod rules;
pub mod service;
