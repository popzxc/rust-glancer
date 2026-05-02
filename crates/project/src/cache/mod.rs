//! Package cache artifact model.
//!
//! Project-level code owns invalidation because only it can see Cargo metadata, workspace graph
//! changes, and the selected residency policy. Lower storage layers should receive already-vetted
//! artifact handles and package payloads.

mod cached;
mod codec;
mod fingerprint;
mod header;
mod payload;
mod plan;
mod store;

pub use self::{
    cached::{
        CachedDependency, CachedPackage, CachedPackageId, CachedPackageSlot, CachedPackageSource,
        CachedPath, CachedRustEdition, CachedTarget, CachedTargetKind,
    },
    codec::PackageCacheCodec,
    fingerprint::Fingerprint,
    header::{CURRENT_PACKAGE_CACHE_SCHEMA_VERSION, PackageCacheHeader, PackageCacheSchemaVersion},
    payload::{PackageCacheArtifact, PackageCacheBodyIrState, PackageCachePayload},
    plan::CachedWorkspace,
    store::PackageCacheStore,
};

#[cfg(test)]
mod tests;
