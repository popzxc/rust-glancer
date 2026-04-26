use std::{fmt, path::PathBuf};

use anyhow::Context as _;

pub(crate) use self::file::{FileId, ParsedFile};
pub use self::{
    package::Package,
    target::{Target, TargetId},
};

pub(crate) mod error;
pub(crate) mod file;
pub(crate) mod package;
pub(crate) mod span;
pub(crate) mod target;

#[cfg(test)]
mod tests;

/// Parsed project metadata, packages, and source files.
#[derive(Debug, Clone)]
pub struct ParseDb {
    workspace_root: PathBuf,
    packages: Vec<Package>,
}

impl ParseDb {
    /// Builds parsed packages for one normalized workspace metadata graph.
    pub fn build(workspace: &crate::workspace_metadata::WorkspaceMetadata) -> anyhow::Result<Self> {
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
    pub(crate) fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Returns all parsed packages.
    pub(crate) fn packages(&self) -> &[Package] {
        &self.packages
    }

    /// Returns one parsed package by slot.
    pub(crate) fn package(&self, package_slot: usize) -> Option<&Package> {
        self.packages.get(package_slot)
    }

    /// Returns one mutable parsed package by slot.
    pub(crate) fn package_mut(&mut self, package_slot: usize) -> Option<&mut Package> {
        self.packages.get_mut(package_slot)
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
