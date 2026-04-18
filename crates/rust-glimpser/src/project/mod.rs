use anyhow::Context as _;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
};

use self::package::PackageAnalysis;

pub mod package;

#[cfg(test)]
mod tests;

/// Controls how far dependency traversal should go from the selected roots.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DependencyScope {
    /// Analyze only explicitly selected root packages.
    #[default]
    WorkspaceOnly,
    /// Include local/path packages reachable from roots.
    WorkspaceAndPathDependencies,
    /// Include the full resolved dependency graph reachable from roots.
    FullResolvedGraph,
}

impl fmt::Display for DependencyScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::WorkspaceOnly => "workspace_only",
            Self::WorkspaceAndPathDependencies => "workspace_and_path_dependencies",
            Self::FullResolvedGraph => "full_resolved_graph",
        };
        write!(f, "{value}")
    }
}

/// Build-time controls for `ProjectAnalysis`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectBuildOptions {
    /// Traversal policy for dependencies.
    pub dependency_scope: DependencyScope,
    /// Optional explicit roots. When empty, workspace members are used.
    pub root_package_ids: Vec<cargo_metadata::PackageId>,
}

/// Analysis result for one Cargo metadata graph, including workspace members and dependencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAnalysis {
    /// Original metadata payload used to produce this analysis.
    pub metadata: cargo_metadata::Metadata,
    /// Parsed analysis slots for the selected packages.
    pub slots: Vec<PackageAnalysis>,
    slot_by_package: HashMap<cargo_metadata::PackageId, usize>,
}

impl ProjectAnalysis {
    /// Builds analyses for workspace members only.
    pub fn build(metadata: cargo_metadata::Metadata) -> anyhow::Result<Self> {
        Self::build_with_options(metadata, ProjectBuildOptions::default())
    }

    /// Builds analyses using explicit traversal options.
    pub fn build_with_options(
        metadata: cargo_metadata::Metadata,
        build_options: ProjectBuildOptions,
    ) -> anyhow::Result<Self> {
        let dependency_ids_by_package = Self::dependency_ids_by_package(&metadata);
        let packages_by_id = metadata
            .packages
            .iter()
            .map(|package| (package.id.clone(), package))
            .collect::<HashMap<_, _>>();
        let known_package_ids = packages_by_id.keys().cloned().collect::<HashSet<_>>();
        let root_package_ids =
            Self::root_package_ids(&metadata, &known_package_ids, &build_options)
                .context("while attempting to choose project roots")?;
        let reachable_package_ids = Self::reachable_package_ids(
            &root_package_ids,
            build_options.dependency_scope,
            &dependency_ids_by_package,
            &packages_by_id,
        );
        let workspace_member_set = metadata
            .workspace_members
            .iter()
            .cloned()
            .collect::<HashSet<_>>();

        let mut slots = Vec::new();
        for package_id in reachable_package_ids {
            let package = packages_by_id
                .get(&package_id)
                .copied()
                .expect("reachable package id must be present in packages map");
            let is_workspace_member = workspace_member_set.contains(&package_id);
            let dependency_ids = dependency_ids_by_package
                .get(&package_id)
                .cloned()
                .unwrap_or_default();
            let slot = PackageAnalysis::build(package, is_workspace_member, dependency_ids)
                .with_context(|| {
                    format!(
                        "while attempting to build package analysis for {}",
                        package_id
                    )
                })?;
            slots.push(slot);
        }

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
        self.slot(package_id)
    }

    /// Returns the analysis slot for a package id, if present.
    pub fn slot(&self, package_id: &cargo_metadata::PackageId) -> Option<&PackageAnalysis> {
        let slot_index = self.slot_index(package_id)?;
        self.slots.get(slot_index)
    }

    /// Returns slot/index for a package id, if present in `slots`.
    pub fn slot_index(&self, package_id: &cargo_metadata::PackageId) -> Option<usize> {
        self.slot_by_package.get(package_id).copied()
    }

    /// Iterates over analyzed packages that belong to the workspace members set.
    pub fn workspace_packages(&self) -> impl Iterator<Item = &PackageAnalysis> + '_ {
        self.slots
            .iter()
            .filter(|package| package.is_workspace_member)
    }

    /// Collects direct dependency ids from Cargo's resolve graph.
    fn dependency_ids_by_package(
        metadata: &cargo_metadata::Metadata,
    ) -> HashMap<cargo_metadata::PackageId, Vec<cargo_metadata::PackageId>> {
        let Some(resolve) = metadata.resolve.as_ref() else {
            return HashMap::new();
        };

        let mut dependency_ids_by_package =
            HashMap::<cargo_metadata::PackageId, Vec<cargo_metadata::PackageId>>::new();
        for node in &resolve.nodes {
            let mut dependency_ids = if node.deps.is_empty() {
                // TODO: remove this fallback once old Cargo output compatibility is unnecessary.
                node.dependencies.clone()
            } else {
                node.deps
                    .iter()
                    .map(|dependency| dependency.pkg.clone())
                    .collect()
            };
            dependency_ids.sort_by_key(|package_id| package_id.to_string());
            dependency_ids.dedup();
            dependency_ids_by_package.insert(node.id.clone(), dependency_ids);
        }

        dependency_ids_by_package
    }

    /// Chooses traversal roots from options, workspace members, or resolve root fallback.
    fn root_package_ids(
        metadata: &cargo_metadata::Metadata,
        known_package_ids: &HashSet<cargo_metadata::PackageId>,
        options: &ProjectBuildOptions,
    ) -> anyhow::Result<Vec<cargo_metadata::PackageId>> {
        let mut root_package_ids = if options.root_package_ids.is_empty() {
            let mut workspace_members = metadata.workspace_members.clone();
            workspace_members.sort_by_key(|package_id| package_id.to_string());
            workspace_members
        } else {
            options.root_package_ids.clone()
        };

        if root_package_ids.is_empty() {
            if let Some(resolve_root) = metadata
                .resolve
                .as_ref()
                .and_then(|resolve| resolve.root.clone())
            {
                root_package_ids.push(resolve_root);
            }
        }
        if root_package_ids.is_empty() {
            root_package_ids.extend(known_package_ids.iter().cloned());
        }

        root_package_ids.sort_by_key(|package_id| package_id.to_string());
        root_package_ids.dedup();

        let missing_roots = root_package_ids
            .iter()
            .filter(|package_id| !known_package_ids.contains(*package_id))
            .cloned()
            .collect::<Vec<_>>();
        if !missing_roots.is_empty() {
            anyhow::bail!(
                "unknown root package ids: {}",
                missing_roots
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        Ok(root_package_ids)
    }

    /// Expands roots according to dependency scope and resolve adjacency.
    fn reachable_package_ids(
        root_package_ids: &[cargo_metadata::PackageId],
        dependency_scope: DependencyScope,
        outgoing_dependency_ids: &HashMap<
            cargo_metadata::PackageId,
            Vec<cargo_metadata::PackageId>,
        >,
        packages_by_id: &HashMap<cargo_metadata::PackageId, &cargo_metadata::Package>,
    ) -> Vec<cargo_metadata::PackageId> {
        if dependency_scope == DependencyScope::WorkspaceOnly {
            let mut package_ids = root_package_ids.to_vec();
            package_ids.sort_by_key(|package_id| package_id.to_string());
            package_ids.dedup();
            return package_ids;
        }

        let mut reachable_package_ids = HashSet::new();
        let mut queue = VecDeque::new();
        for root_package_id in root_package_ids {
            queue.push_back(root_package_id.clone());
        }

        while let Some(package_id) = queue.pop_front() {
            if !reachable_package_ids.insert(package_id.clone()) {
                continue;
            }
            let Some(dependency_ids) = outgoing_dependency_ids.get(&package_id) else {
                continue;
            };

            for dependency_id in dependency_ids {
                let include_dependency = match dependency_scope {
                    DependencyScope::WorkspaceOnly => false,
                    DependencyScope::WorkspaceAndPathDependencies => packages_by_id
                        .get(dependency_id)
                        .map(|package| package.source.is_none())
                        .unwrap_or(false),
                    DependencyScope::FullResolvedGraph => true,
                };

                if include_dependency {
                    queue.push_back(dependency_id.clone());
                }
            }
        }

        let mut package_ids = reachable_package_ids.into_iter().collect::<Vec<_>>();
        package_ids.sort_by_key(|package_id| package_id.to_string());
        package_ids
    }

    /// Formats one dependency reference as a package name plus cargo package id.
    fn format_dependency(&self, package_id: &cargo_metadata::PackageId) -> String {
        self.package(package_id)
            .map(|package| format!("{} ({package_id})", package.package_name()))
            .unwrap_or_else(|| package_id.to_string())
    }
}

/// Renders a project-level report that includes all analyzed packages.
impl fmt::Display for ProjectAnalysis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let workspace_member_count = self.workspace_packages().count();
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
            let scope = if package.is_workspace_member {
                "workspace_member"
            } else {
                "dependency"
            };
            let dependencies = if package.dependency_ids.is_empty() {
                "<none>".to_string()
            } else {
                package
                    .dependency_ids
                    .iter()
                    .map(|package_id| self.format_dependency(package_id))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            writeln!(f)?;
            writeln!(
                f,
                "Package {} [{}] ({scope})",
                package.package_name(),
                package.package_id,
            )?;
            writeln!(f, "Manifest {}", package.manifest_path.display())?;
            writeln!(f, "Dependencies {dependencies}")?;
            writeln!(f)?;
            write!(f, "{}", package.package_index)?;
        }

        Ok(())
    }
}
