//! MadOS core library.
//!
//! * [`product`] — product metadata (name, version, vendor, branding) from the
//!   single source of truth, `product/product.toml`.
//! * [`sysinfo`] — real system information gathered from Linux interfaces
//!   (`/proc`, `/sys`, `statvfs`, `os-release`), with graceful fallbacks.
//! * [`buildinfo`] — image build metadata written at image build time.
//! * [`names`] — stable technical identifiers (D-Bus names, paths).

pub mod apps;
pub mod buildinfo;
pub mod log;
pub mod names;
pub mod product;
pub mod sysinfo;

pub use buildinfo::BuildInfo;
pub use product::Product;
pub use sysinfo::SystemInfo;
