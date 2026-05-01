//! Finalizes collected def-map target state into immutable query data.
//!
//! Target collection intentionally stops before cross-target facts such as implicit roots,
//! preludes, and imports are fully known. Clean builds and package rebuilds now share one
//! finalization model:
//! - packages with fresh `TargetState`s are "dirty" and receive fixed-point import resolution;
//! - packages without fresh states are read from an optional frozen baseline;
//! - a clean build has no baseline and marks every package dirty;
//! - a package rebuild has an old baseline and marks only affected packages dirty.

mod build;
mod implicit_roots;
mod imports;
mod rebuild;
mod scope;

pub(super) use self::{build::build_db, rebuild::rebuild_packages};
