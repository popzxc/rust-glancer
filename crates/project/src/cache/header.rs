//! Versioned package artifact headers.
//!
//! The header is the first data read from an artifact. It keeps the schema version next to the
//! cached package metadata so stale or mismatched files can be rejected before loading analysis
//! payloads.

use rkyv::{Archive, Deserialize, Serialize};

use super::cached::CachedPackage;

/// Current on-disk package artifact schema.
pub const CURRENT_PACKAGE_CACHE_SCHEMA_VERSION: PackageCacheSchemaVersion =
    PackageCacheSchemaVersion(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Archive, Serialize, Deserialize)]
pub struct PackageCacheSchemaVersion(pub u32);

/// Header shared by future package cache artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Archive, Serialize, Deserialize)]
pub struct PackageCacheHeader {
    pub schema_version: PackageCacheSchemaVersion,
    pub package: CachedPackage,
}

impl PackageCacheHeader {
    pub fn new(package: CachedPackage) -> Self {
        Self {
            schema_version: CURRENT_PACKAGE_CACHE_SCHEMA_VERSION,
            package,
        }
    }
}
