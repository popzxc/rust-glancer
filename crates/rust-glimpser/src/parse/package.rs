use std::path::Path;

use anyhow::Context as _;

use crate::{
    parse::{FileDb, FileId, Target, TargetId},
    workspace_metadata::{PackageId, TargetKind},
};

/// Parsed package, including package-local files and target entrypoints.
#[derive(Debug, Clone)]
pub struct Package {
    /// Stable package id from workspace metadata.
    id: PackageId,
    /// Package name from `Cargo.toml`.
    package_name: String,
    /// Whether this package belongs to the analyzed workspace.
    is_workspace_member: bool,
    /// All parsed files known to this package.
    pub(crate) files: FileDb,
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

    /// Returns the stable package id.
    pub fn id(&self) -> &PackageId {
        &self.id
    }

    /// Returns whether this package belongs to the analyzed workspace.
    pub(crate) fn is_workspace_member(&self) -> bool {
        self.is_workspace_member
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
    pub(crate) fn build(package: &crate::workspace_metadata::Package) -> anyhow::Result<Self> {
        // Outside of the workspace being analyzed, we only keep the library target.
        let targets = if package.is_workspace_member {
            package.targets.clone()
        } else {
            package
                .targets
                .iter()
                .filter(|target| matches!(target.kind, TargetKind::Lib))
                .cloned()
                .collect()
        };

        let mut files = FileDb::default();
        let mut parsed_targets = Vec::new();

        for (idx, target) in targets.into_iter().enumerate() {
            let target_id = TargetId(idx);
            let root_file = files.get_or_parse_file(&target.src_path).with_context(|| {
                format!(
                    "while attempting to parse target root {}",
                    target.src_path.display()
                )
            })?;

            parsed_targets.push(Target {
                id: target_id,
                name: target.name,
                kind: target.kind,
                src_path: target.src_path,
                root_file,
            });
        }

        Ok(Self {
            id: package.id.clone(),
            package_name: package.name.clone(),
            is_workspace_member: package.is_workspace_member,
            files,
            targets: parsed_targets,
        })
    }
}
