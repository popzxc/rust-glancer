use std::{
    fmt,
    path::{Path, PathBuf},
};

use anyhow::Context as _;

mod error;
mod file;
mod package;
mod span;
mod target;

#[cfg(test)]
mod tests;

pub use self::{
    error::ParseError,
    file::{FileId, ParsedFile},
    package::Package,
    span::{LineColumnSpan, LineIndex, Position, Span, TextSpan},
    target::{Target, TargetId},
};

/// Parsed project metadata, packages, and source files.
#[derive(Debug, Clone)]
pub struct ParseDb {
    workspace_root: PathBuf,
    packages: Vec<Package>,
}

/// One package-local file touched by an editor/source update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackageFileRef {
    pub package: usize,
    pub file: FileId,
}

impl ParseDb {
    /// Builds parsed packages for one normalized workspace metadata graph.
    pub fn build(workspace: &rg_workspace::WorkspaceMetadata) -> anyhow::Result<Self> {
        let packages = workspace
            .packages()
            .iter()
            .map(|package| {
                Package::build(package).with_context(|| {
                    format!(
                        "while attempting to build parsed package for {}",
                        package.id
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            workspace_root: workspace.workspace_root().to_path_buf(),
            packages,
        })
    }

    /// Iterates over parsed packages that belong to the workspace members set.
    pub fn workspace_packages(&self) -> impl Iterator<Item = &Package> + '_ {
        self.packages
            .iter()
            .filter(|package| package.is_workspace_member())
    }

    /// Returns the number of parsed packages.
    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Returns all parsed packages.
    pub fn packages(&self) -> &[Package] {
        &self.packages
    }

    /// Returns one parsed package by slot.
    pub fn package(&self, package_slot: usize) -> Option<&Package> {
        self.packages.get(package_slot)
    }

    /// Returns one mutable parsed package by slot.
    pub fn package_mut(&mut self, package_slot: usize) -> Option<&mut Package> {
        self.packages.get_mut(package_slot)
    }

    /// Replaces the in-memory text for every parsed package that already owns `file_path`.
    ///
    /// This keeps package-local `FileId`s stable. Unknown files do not appear in the returned
    /// owner list yet, but their source override is still recorded so later module discovery can
    /// parse the editor text instead of the on-disk file.
    pub fn set_file_text(
        &mut self,
        file_path: &Path,
        text: impl AsRef<str>,
    ) -> anyhow::Result<Vec<PackageFileRef>> {
        let canonical_file_path = file_path
            .canonicalize()
            .with_context(|| format!("while attempting to canonicalize {}", file_path.display()))?;
        let mut changed_files = Vec::new();
        let text = text.as_ref();

        for (package_slot, package) in self.packages.iter_mut().enumerate() {
            let Some(file_id) = package.set_file_text(&canonical_file_path, text) else {
                continue;
            };

            changed_files.push(PackageFileRef {
                package: package_slot,
                file: file_id,
            });
        }

        Ok(changed_files)
    }
}

/// Renders a project-level report of parsed packages and diagnostics.
impl fmt::Display for ParseDb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let workspace_member_count = self.workspace_packages().count();
        let dependency_count = self.packages.len().saturating_sub(workspace_member_count);
        writeln!(f, "Project {}", self.workspace_root.display())?;
        writeln!(
            f,
            "Packages {} (workspace members: {}, dependencies: {})",
            self.packages.len(),
            workspace_member_count,
            dependency_count,
        )?;

        for package in &self.packages {
            writeln!(f)?;
            writeln!(f, "Package {} [{}]", package.package_name(), package.id())?;
        }

        Ok(())
    }
}
