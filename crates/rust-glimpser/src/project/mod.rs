use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use anyhow::Context as _;
use rayon::prelude::*;

use self::package::PackageAnalysis;

pub mod package;

#[cfg(test)]
mod tests;

/// Analysis result for one Cargo metadata graph, including workspace members and dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAnalysis {
    /// Original metadata payload used to produce this analysis.
    metadata: cargo_metadata::Metadata,
    /// Parsed analysis slots for the selected packages.
    slots: Vec<PackageAnalysis>,
    /// PackageId -> slot
    slot_by_package: HashMap<cargo_metadata::PackageId, usize>,
}

impl ProjectAnalysis {
    /// Builds analyses for workspace members only.
    pub fn build(metadata: cargo_metadata::Metadata) -> anyhow::Result<Self> {
        let workspace_ids: HashSet<cargo_metadata::PackageId> = metadata
            .workspace_packages()
            .into_iter()
            .map(|p| p.id.clone())
            .collect();

        let slots = metadata
            .packages
            .clone()
            .into_par_iter()
            .map(|package| -> anyhow::Result<PackageAnalysis> {
                let id = package.id.clone();
                let is_workspace = workspace_ids.contains(&id);

                Ok(
                    PackageAnalysis::build(package, is_workspace).with_context(|| {
                        format!("while attempting to build package analysis for {id}",)
                    })?,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let slot_by_package = slots
            .iter()
            .enumerate()
            .map(|(idx, package)| (package.package_id.clone(), idx))
            .collect::<HashMap<_, _>>();

        Ok(Self {
            metadata,
            slots,
            slot_by_package,
        })
    }

    /// Returns analysis for a specific package id, if this project contains it.
    pub fn package(&self, package_id: &cargo_metadata::PackageId) -> Option<&PackageAnalysis> {
        let slot_index = self.slot_by_package.get(package_id).copied()?;
        self.slots.get(slot_index)
    }

    /// Iterates over analyzed packages that belong to the workspace members set.
    pub fn workspace_packages(&self) -> impl Iterator<Item = &PackageAnalysis> + '_ {
        self.metadata
            .packages
            .iter()
            .filter(|&p| self.metadata.workspace_members.contains(&p.id))
            .map(|p| {
                let slot = *self
                    .slot_by_package
                    .get(&p.id)
                    .expect("Workspace member must be known");
                &self.slots[slot]
            })
    }
}

/// Renders a project-level report that includes all analyzed packages.
impl fmt::Display for ProjectAnalysis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let workspace_member_count = self.metadata.workspace_packages().len();
        let dependency_count = self.slots.len().saturating_sub(workspace_member_count);
        writeln!(f, "Project {}", self.metadata.workspace_root)?;
        writeln!(
            f,
            "Packages {} (workspace members: {}, dependencies: {})",
            self.slots.len(),
            workspace_member_count,
            dependency_count,
        )?;

        for package in &self.slots {
            writeln!(f)?;
            writeln!(
                f,
                "Package {} [{}]",
                package.package_name(),
                package.package_id,
            )?;
            writeln!(f)?;
            write!(f, "{}", package.package_index)?;
        }

        Ok(())
    }
}
