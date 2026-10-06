//! Stable technical identifiers.
//!
//! These are API identifiers, not branding: they use the codename and are kept
//! stable so that renaming the product (product/product.toml) never breaks the
//! system API. See docs/architecture/overview.md ("Naming").

/// Directory holding installed MadOS data files.
pub const DATA_DIR: &str = "/usr/lib/mados";
/// Installed product metadata.
pub const PRODUCT_FILE: &str = "/usr/lib/mados/product.toml";
/// Installed build metadata (written at image build time).
pub const BUILD_INFO_FILE: &str = "/usr/lib/mados/build-info.json";

/// System-bus service exposing controlled system operations.
pub const SYSTEM_BUS_NAME: &str = "org.mados.System1";
pub const SYSTEM_OBJECT_PATH: &str = "/org/mados/System1";
pub const SYSTEM_INTERFACE: &str = "org.mados.System1";

/// Session-bus service for the assistant.
pub const ASSISTANT_BUS_NAME: &str = "org.mados.Assistant1";
pub const ASSISTANT_OBJECT_PATH: &str = "/org/mados/Assistant1";

/// polkit action ids (see system/usr/share/polkit-1/actions/org.mados.system.policy).
pub const ACTION_POWER: &str = "org.mados.system.power";
