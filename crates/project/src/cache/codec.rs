//! Owned cache metadata encoding built on rkyv.
//!
//! Cache-native metadata structs are the artifact schema, so they archive directly. Payloads are
//! still handled separately because analysis phase data needs explicit serialization adapters.

use anyhow::Context as _;

use super::{CURRENT_PACKAGE_CACHE_SCHEMA_VERSION, PackageCacheHeader};

/// Encodes and decodes cache artifact metadata.
pub struct PackageCacheCodec;

impl PackageCacheCodec {
    pub fn encode_header(header: &PackageCacheHeader) -> anyhow::Result<Vec<u8>> {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(header)
            .map_err(|error| anyhow::anyhow!("{error}"))
            .context("while attempting to serialize package cache header")?;

        Ok(bytes.to_vec())
    }

    pub fn decode_header(bytes: &[u8]) -> anyhow::Result<PackageCacheHeader> {
        let header = rkyv::from_bytes::<PackageCacheHeader, rkyv::rancor::Error>(bytes)
            .map_err(|error| anyhow::anyhow!("{error}"))
            .context("while attempting to deserialize package cache header")?;

        if header.schema_version != CURRENT_PACKAGE_CACHE_SCHEMA_VERSION {
            anyhow::bail!(
                "unsupported package cache schema version {}, expected {}",
                header.schema_version.0,
                CURRENT_PACKAGE_CACHE_SCHEMA_VERSION.0,
            );
        }

        Ok(header)
    }
}
