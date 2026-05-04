//! Narrow package materialization plans.
//!
//! Project residency decides what stays in memory between requests. Demand is narrower: it says
//! which package artifacts one query or rebuild needs to materialize before it can inspect them.

use std::collections::HashSet;

use rg_def_map::{PackageSlot, TargetRef};
use rg_workspace::{PackageId, TargetKind, WorkspaceMetadata};

/// Package slots that should be available inside one analysis operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackageDemand {
    packages: Vec<bool>,
}

impl PackageDemand {
    /// Demands every package in the workspace graph.
    pub(crate) fn all(workspace: &WorkspaceMetadata) -> Self {
        Self {
            packages: vec![true; workspace.packages().len()],
        }
    }

    /// Demands the listed package slots without expanding dependency edges.
    pub(crate) fn package_slots(workspace: &WorkspaceMetadata, packages: &[PackageSlot]) -> Self {
        let mut demand = Self::empty(workspace);
        for package in packages {
            demand.insert(*package);
        }
        demand
    }

    /// Demands packages plus every dependency their targets can name during rebuild resolution.
    pub(crate) fn packages_with_dependencies(
        workspace: &WorkspaceMetadata,
        packages: &[PackageSlot],
    ) -> Self {
        let mut demand = Self::empty(workspace);
        let mut expanded = HashSet::new();
        let mut stack = Vec::new();

        for package in packages {
            demand.insert(*package);

            let Some(metadata) = workspace.packages().get(package.0) else {
                continue;
            };
            for target in &metadata.targets {
                if expanded.insert((*package, target.kind.clone())) {
                    stack.push((*package, target.kind.clone()));
                }
            }
        }

        demand.expand_visible_dependencies(workspace, &mut expanded, &mut stack);
        demand
    }

    /// Demands target packages plus the transitive dependencies visible from those targets.
    pub(crate) fn targets(workspace: &WorkspaceMetadata, targets: &[TargetRef]) -> Self {
        let mut demand = Self::empty(workspace);
        let mut expanded = HashSet::new();
        let mut stack = Vec::new();

        for target in targets {
            demand.insert(target.package);

            let Some(target_kind) = Self::target_kind(workspace, *target) else {
                continue;
            };
            if expanded.insert((target.package, target_kind.clone())) {
                stack.push((target.package, target_kind.clone()));
            }
        }

        demand.expand_visible_dependencies(workspace, &mut expanded, &mut stack);
        demand
    }

    fn expand_visible_dependencies(
        &mut self,
        workspace: &WorkspaceMetadata,
        expanded: &mut HashSet<(PackageSlot, TargetKind)>,
        stack: &mut Vec<(PackageSlot, TargetKind)>,
    ) {
        while let Some((package, target_kind)) = stack.pop() {
            let Some(metadata) = workspace.packages().get(package.0) else {
                continue;
            };

            for dependency in &metadata.dependencies {
                if !dependency.applies_to_target(&target_kind) {
                    continue;
                }

                let Some(dependency_slot) = Self::package_slot(workspace, dependency.package_id())
                else {
                    continue;
                };
                self.insert(dependency_slot);
                // Dependencies are reached as library crates. Their own dev/build dependencies
                // are not visible to the original target query.
                if expanded.insert((dependency_slot, TargetKind::Lib)) {
                    stack.push((dependency_slot, TargetKind::Lib));
                }
            }
        }
    }

    pub(crate) fn contains(&self, package: PackageSlot) -> bool {
        self.packages.get(package.0).copied().unwrap_or(false)
    }

    pub(crate) fn package_count(&self) -> usize {
        self.packages.len()
    }

    fn empty(workspace: &WorkspaceMetadata) -> Self {
        Self {
            packages: vec![false; workspace.packages().len()],
        }
    }

    fn insert(&mut self, package: PackageSlot) -> bool {
        let Some(slot) = self.packages.get_mut(package.0) else {
            return false;
        };
        let was_absent = !*slot;
        *slot = true;
        was_absent
    }

    fn target_kind(workspace: &WorkspaceMetadata, target: TargetRef) -> Option<&TargetKind> {
        workspace
            .packages()
            .get(target.package.0)?
            .targets
            .get(target.target.0)
            .map(|target| &target.kind)
    }

    fn package_slot(workspace: &WorkspaceMetadata, package_id: &PackageId) -> Option<PackageSlot> {
        workspace
            .packages()
            .iter()
            .enumerate()
            .find_map(|(slot, package)| (package.id == *package_id).then_some(PackageSlot(slot)))
    }
}
