use anyhow::Context as _;

use self::index::PackageIndex;

pub(crate) mod index;

/// Parsed package view enriched with graph metadata from Cargo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageAnalysis {
    /// Stable package id from Cargo metadata.
    pub package_id: cargo_metadata::PackageId,
    /// Whether the package is a part of workspace being analyzed
    pub is_workspace: bool,
    /// Metadata package
    metadata: cargo_metadata::Package,
    /// Parsed item tree index for this package.
    pub package_index: PackageIndex,
}

impl PackageAnalysis {
    /// Builds one package analysis from Cargo package metadata.
    pub(super) fn build(
        package: cargo_metadata::Package,
        is_workspace: bool,
    ) -> anyhow::Result<Self> {
        let package_name = package.name.to_string();
        let package_id = package.id.clone();

        // Outside of the workspace we're working with, we don't want to analyze any tests/examples/binaries/etc.
        let targets = if is_workspace {
            package.targets.clone()
        } else {
            package
                .targets
                .iter()
                .filter(|t| t.is_kind(cargo_metadata::TargetKind::Lib))
                .cloned()
                .collect()
        };

        let package_index =
            PackageIndex::build(package_name.clone(), targets).with_context(|| {
                format!(
                    "while attempting to build package index for {}",
                    package_name
                )
            })?;

        Ok(Self {
            package_id,
            is_workspace,
            metadata: package,
            package_index,
        })
    }

    /// Returns the logical package name from the parsed package index.
    pub fn package_name(&self) -> &str {
        &self.package_index.package_name
    }
}
