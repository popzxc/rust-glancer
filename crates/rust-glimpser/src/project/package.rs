use anyhow::Context as _;

use crate::item_tree::{package::PackageIndex, target::TargetInput};

/// Parsed package view enriched with graph metadata from Cargo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageAnalysis {
    /// Stable package id from Cargo metadata.
    pub package_id: cargo_metadata::PackageId,
    /// Parsed item tree index for this package.
    pub package_index: PackageIndex,
}

impl PackageAnalysis {
    /// Builds one package analysis from Cargo package metadata.
    pub(super) fn build(package: &cargo_metadata::Package) -> anyhow::Result<Self> {
        let package_name = package.name.to_string();
        let package_id = package.id.clone();
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
            package_index,
        })
    }

    /// Returns the logical package name from the parsed package index.
    pub fn package_name(&self) -> &str {
        &self.package_index.package_name
    }
}
