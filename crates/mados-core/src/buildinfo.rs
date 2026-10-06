//! Image build metadata, written by the image build to
//! `/usr/lib/mados/build-info.json` (see scripts/gen-build-info.py).

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct BuildInfo {
    /// Unique build identifier, e.g. `20261006.1-3f2a9c1`.
    pub build_id: String,
    /// Product version at build time.
    pub version: String,
    /// Git commit of the MadOS source tree ("unknown" if unavailable).
    #[serde(default)]
    pub git_commit: String,
    /// RFC 3339 UTC build timestamp.
    #[serde(default)]
    pub build_time: String,
    /// Base image reference used for the build.
    #[serde(default)]
    pub base_image: String,
    /// Image variant ("dev" or "release").
    #[serde(default)]
    pub variant: String,
}

impl BuildInfo {
    pub fn from_file(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Installed build info; `None` when not running on a MadOS image.
    pub fn load() -> Option<Self> {
        Self::from_file(Path::new(crate::names::BUILD_INFO_FILE))
    }
}
