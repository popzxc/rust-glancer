use std::path::Path;

use anyhow::Context as _;

use crate::{
    parse::{FileId, ParsedFile, Target, TargetId, file::FileDb},
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
    files: FileDb,
    /// Parsed targets rooted in this package.
    targets: Vec<Target>,
}

impl Package {
    /// Parses a package-local source file, or returns its existing file id if it is already cached.
    pub fn parse_file(&mut self, file_path: &Path) -> anyhow::Result<FileId> {
        self.files.get_or_parse_file(file_path)
    }

    /// Returns the cached parsed file for a previously known `FileId`.
    pub fn parsed_file(&self, file_id: FileId) -> Option<ParsedFile<'_>> {
        self.files.parsed_file(file_id)
    }

    /// Iterates over all files parsed for this package.
    pub fn parsed_files(&self) -> impl Iterator<Item = ParsedFile<'_>> {
        self.files.parsed_files()
    }

    /// Returns the path associated with a file id, if the id is valid.
    pub fn file_path(&self, file_id: FileId) -> Option<&Path> {
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
    pub fn is_workspace_member(&self) -> bool {
        self.is_workspace_member
    }

    /// Returns all parsed targets for this package.
    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    /// Returns one parsed target by stable id.
    pub fn target(&self, target_id: TargetId) -> Option<&Target> {
        self.targets.iter().find(|target| target.id == target_id)
    }

    /// Parses package targets and their root files.
    pub fn build(package: &crate::workspace_metadata::Package) -> anyhow::Result<Self> {
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
