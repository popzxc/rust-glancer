use anyhow::Context as _;
use std::path::PathBuf;

use crate::item_tree::{package::PackageIndex, target::TargetInput};

/// Parsed package view enriched with graph metadata from Cargo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageAnalysis {
    /// Stable package id from Cargo metadata.
    pub package_id: cargo_metadata::PackageId,
    /// Canonical path to the package `Cargo.toml`.
    pub manifest_path: PathBuf,
    /// Marks whether this package belongs to the workspace itself.
    pub is_workspace_member: bool,
    /// Direct dependency package ids from Cargo resolve graph.
    pub dependency_ids: Vec<cargo_metadata::PackageId>,
    /// Parsed item tree index for this package.
    pub package_index: PackageIndex,
}

impl PackageAnalysis {
    /// Builds one package analysis from Cargo package metadata.
    pub(super) fn build(
        package: &cargo_metadata::Package,
        is_workspace_member: bool,
        mut dependency_ids: Vec<cargo_metadata::PackageId>,
    ) -> anyhow::Result<Self> {
        dependency_ids.sort_by_key(|package_id| package_id.to_string());

        let package_name = package.name.to_string();
        let package_id = package.id.clone();
        let manifest_path = package.manifest_path.clone().into_std_path_buf();
        let target_inputs = package
            .targets
            .iter()
            .map(|target| TargetInput {
                name: target.name.clone(),
                kinds: target.kind.iter().map(|kind| kind.to_string()).collect(),
                root_file: target.src_path.clone().into_std_path_buf(),
            })
            .collect::<Vec<_>>();

        let package_index =
            PackageIndex::build(package_name.clone(), target_inputs).with_context(|| {
                format!(
                    "while attempting to build package index for {}",
                    package_name
                )
            })?;

        Ok(Self {
            package_id,
            manifest_path,
            is_workspace_member,
            dependency_ids,
            package_index,
        })
    }

    /// Returns the logical package name from the parsed package index.
    pub fn package_name(&self) -> &str {
        &self.package_index.package_name
    }
}
