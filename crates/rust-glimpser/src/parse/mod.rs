use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use anyhow::Context as _;

use self::{def_map::populate_project_scopes, package::PackageIndex};

pub(crate) mod def_map;
pub(crate) mod error;
pub(crate) mod file;
pub(crate) mod item;
pub(crate) mod package;
pub(crate) mod span;
pub(crate) mod target;

#[cfg(test)]
mod tests;

/// Analysis result for one Cargo metadata graph, including workspace members and dependencies.
#[derive(Debug, Clone)]
pub struct ProjectAnalysis {
    /// Original metadata payload used to produce this analysis.
    metadata: cargo_metadata::Metadata,
    /// Parsed packages.
    packages: Vec<PackageIndex>,
    /// PackageId -> Package
    package_by_id: HashMap<cargo_metadata::PackageId, usize>, // TODO: maybe remove though probably we will need it.
}

impl ProjectAnalysis {
    /// Builds analyses for workspace members only.
    pub fn build(metadata: cargo_metadata::Metadata) -> anyhow::Result<Self> {
        let workspace_ids: HashSet<cargo_metadata::PackageId> = metadata
            .workspace_packages()
            .into_iter()
            .map(|p| p.id.clone())
            .collect();

        let mut slots = metadata
            .packages
            .clone()
            .into_iter()
            .map(|package| -> anyhow::Result<PackageIndex> {
                let id = package.id.clone();
                let is_workspace = workspace_ids.contains(&id);

                Ok(PackageIndex::build(package, is_workspace).with_context(|| {
                    format!("while attempting to build package analysis for {id}",)
                })?)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let slot_by_package = slots
            .iter()
            .enumerate()
            .map(|(idx, package)| (package.id().clone(), idx))
            .collect::<HashMap<_, _>>();

        populate_project_scopes(&metadata, &mut slots, &slot_by_package)
            .context("while attempting to build target namespace maps")?;

        Ok(Self {
            metadata,
            packages: slots,
            package_by_id: slot_by_package,
        })
    }

    /// Returns analysis for a specific package id, if this project contains it.
    pub fn package(&self, package_id: &cargo_metadata::PackageId) -> Option<&PackageIndex> {
        let slot_index = self.package_by_id.get(package_id).copied()?;
        self.packages.get(slot_index)
    }

    /// Iterates over analyzed packages that belong to the workspace members set.
    pub fn workspace_packages(&self) -> impl Iterator<Item = &PackageIndex> + '_ {
        self.metadata
            .packages
            .iter()
            .filter(|&p| self.metadata.workspace_members.contains(&p.id))
            .map(|p| {
                let slot = *self
                    .package_by_id
                    .get(&p.id)
                    .expect("Workspace member must be known");
                &self.packages[slot]
            })
    }

    /// Returns all analyzed packages.
    #[cfg(test)]
    pub(crate) fn packages(&self) -> &[PackageIndex] {
        &self.packages
    }
}

/// Renders a project-level report that includes all analyzed packages.
impl fmt::Display for ProjectAnalysis {
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
            writeln!(f, "Package {} [{}]", package.package_name(), package.id(),)?;
            writeln!(f)?;
            write!(f, "{}", package)?;
        }

        Ok(())
    }
}
