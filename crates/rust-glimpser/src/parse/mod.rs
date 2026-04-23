use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use anyhow::Context as _;

pub(crate) use self::file::{FileDb, FileId};
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
    /// Original metadata payload used to produce this parse database.
    metadata: cargo_metadata::Metadata,
    /// Parsed packages.
    packages: Vec<Package>,
    /// PackageId -> package slot.
    package_by_id: HashMap<cargo_metadata::PackageId, usize>,
}

impl ParseDb {
    /// Builds parsed packages for one Cargo metadata graph.
    pub fn build(metadata: cargo_metadata::Metadata) -> anyhow::Result<Self> {
        let workspace_ids: HashSet<cargo_metadata::PackageId> = metadata
            .workspace_packages()
            .into_iter()
            .map(|package| package.id.clone())
            .collect();

        let packages = metadata
            .packages
            .clone()
            .into_iter()
            .map(|package| -> anyhow::Result<Package> {
                let id = package.id.clone();
                let is_workspace = workspace_ids.contains(&id);

                Package::build(package, is_workspace)
                    .with_context(|| format!("while attempting to build parsed package for {id}"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let package_by_id = packages
            .iter()
            .enumerate()
            .map(|(idx, package)| (package.id().clone(), idx))
            .collect::<HashMap<_, _>>();

        Ok(Self {
            metadata,
            packages,
            package_by_id,
        })
    }

    /// Iterates over parsed packages that belong to the workspace members set.
    pub fn workspace_packages(&self) -> impl Iterator<Item = &Package> + '_ {
        self.metadata
            .packages
            .iter()
            .filter(|package| self.metadata.workspace_members.contains(&package.id))
            .map(|package| {
                let slot = *self
                    .package_by_id
                    .get(&package.id)
                    .expect("workspace member must be known");
                &self.packages[slot]
            })
    }

    /// Returns all parsed packages.
    pub(crate) fn packages(&self) -> &[Package] {
        &self.packages
    }

    /// Returns mutable parsed packages for later phases that enrich the same source data.
    pub(crate) fn packages_mut(&mut self) -> &mut [Package] {
        &mut self.packages
    }

    /// Returns the package slot lookup used by later phases.
    pub(crate) fn package_by_id(&self) -> &HashMap<cargo_metadata::PackageId, usize> {
        &self.package_by_id
    }

    /// Returns the original cargo metadata.
    pub(crate) fn metadata(&self) -> &cargo_metadata::Metadata {
        &self.metadata
    }
}

/// Renders a project-level report of parsed packages and diagnostics.
impl fmt::Display for ParseDb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let workspace_member_count = self.metadata.workspace_packages().len();
        let dependency_count = self.packages.len().saturating_sub(workspace_member_count);
        writeln!(f, "Project {}", self.metadata.workspace_root)?;
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
