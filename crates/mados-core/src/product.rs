//! Product metadata.
//!
//! The repository's `product/product.toml` is embedded at compile time as a
//! fallback; on an installed system `/usr/lib/mados/product.toml` (same file,
//! copied at image build time) takes precedence.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Compile-time copy of product/product.toml.
pub const EMBEDDED_PRODUCT_TOML: &str = include_str!("../../../product/product.toml");

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Product {
    pub schema: u32,
    pub product: ProductNames,
    pub version: Version,
    pub vendor: Vendor,
    pub urls: Urls,
    pub branding: Branding,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ProductNames {
    pub name: String,
    pub full_name: String,
    pub id: String,
    pub tagline: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub pre: String,
    pub codename: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Vendor {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Urls {
    pub home: String,
    pub bugs: String,
    pub docs: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Branding {
    pub logo: String,
    pub wallpaper: String,
    pub accent: String,
    pub accent_secondary: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ProductError {
    #[error("cannot read {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("invalid product metadata: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("unsupported product schema {0}")]
    Schema(u32),
}

impl Version {
    /// Full semver-style version, e.g. `0.1.0-dev`.
    pub fn full(&self) -> String {
        let base = format!("{}.{}.{}", self.major, self.minor, self.patch);
        if self.pre.is_empty() {
            base
        } else {
            format!("{base}-{}", self.pre)
        }
    }
}

impl Product {
    pub fn parse(text: &str) -> Result<Self, ProductError> {
        let p: Product = toml::from_str(text)?;
        if p.schema != 1 {
            return Err(ProductError::Schema(p.schema));
        }
        Ok(p)
    }

    pub fn from_file(path: &Path) -> Result<Self, ProductError> {
        let text = std::fs::read_to_string(path).map_err(|source| ProductError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::parse(&text)
    }

    /// The compiled-in metadata. Always valid (checked by unit tests).
    pub fn embedded() -> Self {
        Self::parse(EMBEDDED_PRODUCT_TOML).expect("embedded product.toml is valid")
    }

    /// Installed metadata if present and valid, otherwise the embedded copy.
    pub fn load() -> Self {
        Self::from_file(Path::new(crate::names::PRODUCT_FILE)).unwrap_or_else(|_| Self::embedded())
    }

    /// "MadOS 0.1.0-dev"
    pub fn display_name(&self) -> String {
        format!("{} {}", self.product.name, self.version.full())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_metadata_is_valid() {
        let p = Product::embedded();
        assert_eq!(p.schema, 1);
        assert!(!p.product.name.is_empty());
        assert!(p.branding.accent.starts_with('#') && p.branding.accent.len() == 7);
    }

    #[test]
    fn version_formatting() {
        let mut v = Product::embedded().version;
        v.major = 1;
        v.minor = 2;
        v.patch = 3;
        v.pre = "beta.1".into();
        assert_eq!(v.full(), "1.2.3-beta.1");
        v.pre.clear();
        assert_eq!(v.full(), "1.2.3");
    }

    #[test]
    fn rejects_unknown_schema() {
        let text = EMBEDDED_PRODUCT_TOML.replace("schema = 1", "schema = 7");
        assert!(matches!(Product::parse(&text), Err(ProductError::Schema(7))));
    }

    #[test]
    fn missing_file_falls_back() {
        assert!(Product::from_file(Path::new("/nonexistent/product.toml")).is_err());
    }
}
