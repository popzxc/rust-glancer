use std::path::Path;

use anyhow::Context as _;

use crate::parse::{FileDb, FileId, Target, TargetId};

/// Parsed package, including package-local files and target entrypoints.
#[derive(Debug, Clone)]
pub struct Package {
    /// Package name from `Cargo.toml`.
    package_name: String,
    /// All parsed files known to this package.
    pub(crate) files: FileDb,
    /// Cargo metadata for the package.
    metadata: cargo_metadata::Package,
    /// Parsed targets rooted in this package.
    pub(crate) targets: Vec<Target>,
}

impl Package {
    /// Returns the path associated with a file id, if the id is valid.
    pub(crate) fn file_path(&self, file_id: FileId) -> Option<&Path> {
        self.files.file_path(file_id)
    }

    /// Returns the logical package name from the parsed package.
    pub fn package_name(&self) -> &str {
        &self.package_name
    }

    /// Returns the package id from Cargo metadata.
    pub fn id(&self) -> &cargo_metadata::PackageId {
        &self.metadata.id
    }

    /// Returns all parsed targets for this package.
    #[cfg(test)]
    pub(crate) fn targets(&self) -> &[Target] {
        &self.targets
    }

    /// Returns one parsed target by stable id.
    pub(crate) fn target(&self, target_id: TargetId) -> Option<&Target> {
        self.targets.iter().find(|target| target.id == target_id)
    }

    /// Parses package targets and their root files.
    pub(crate) fn build(
        package: cargo_metadata::Package,
        is_workspace: bool,
    ) -> anyhow::Result<Self> {
        let package_name = package.name.to_string();

        // Outside of the workspace being analyzed, we only keep the library target.
        let targets = if is_workspace {
            package.targets.clone()
        } else {
            package
                .targets
                .iter()
                .filter(|target| target.is_kind(cargo_metadata::TargetKind::Lib))
                .cloned()
                .collect()
        };

        let mut files = FileDb::default();
        let mut parsed_targets = Vec::new();

        for (idx, cargo_target) in targets.into_iter().enumerate() {
            let target_id = TargetId(idx);
            let root_path = cargo_target.src_path.as_path().as_std_path();
            let root_file = files.get_or_parse_file(&root_path).with_context(|| {
                format!(
                    "while attempting to parse target root {}",
                    root_path.display()
                )
            })?;

            parsed_targets.push(Target {
                id: target_id,
                cargo_target,
                root_file,
            });
        }

        Ok(Self {
            metadata: package,
            package_name,
            files,
            targets: parsed_targets,
        })
    }
}
