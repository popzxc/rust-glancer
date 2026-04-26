use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

/// Normalized workspace metadata used by the analysis pipeline.
///
/// This is our internal view of `cargo metadata`: it keeps only the fields and semantics the
/// later phases care about and avoids leaking Cargo's transport types throughout the codebase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMetadata {
    workspace_root: PathBuf,
    packages: Vec<Package>,
    package_by_id: HashMap<PackageId, usize>,
}

impl WorkspaceMetadata {
    /// Lowers raw `cargo metadata` output into the project's normalized metadata model.
    pub fn from_cargo(metadata: cargo_metadata::Metadata) -> Self {
        let workspace_root = metadata.workspace_root.as_std_path().to_path_buf();
        let workspace_members = metadata
            .workspace_members
            .iter()
            .map(PackageId::from_cargo)
            .collect::<HashSet<_>>();
        let dependencies_by_package = metadata
            .resolve
            .as_ref()
            .map(Self::lower_dependencies)
            .unwrap_or_default();

        let packages = metadata
            .packages
            .into_iter()
            .map(|package| {
                let package_id = PackageId::from_cargo(&package.id);
                Package {
                    id: package_id.clone(),
                    name: package.name.to_string(),
                    is_workspace_member: workspace_members.contains(&package_id),
                    manifest_path: package.manifest_path.as_std_path().to_path_buf(),
                    targets: package.targets.iter().map(Target::from_cargo).collect(),
                    dependencies: dependencies_by_package
                        .get(&package_id)
                        .cloned()
                        .unwrap_or_default(),
                }
            })
            .collect::<Vec<_>>();

        let package_by_id = packages
            .iter()
            .enumerate()
            .map(|(idx, package)| (package.id.clone(), idx))
            .collect();

        Self {
            workspace_root,
            packages,
            package_by_id,
        }
    }

    fn lower_dependencies(
        resolve: &cargo_metadata::Resolve,
    ) -> HashMap<PackageId, Vec<PackageDependency>> {
        resolve
            .nodes
            .iter()
            .map(|node| {
                (
                    PackageId::from_cargo(&node.id),
                    node.deps
                        .iter()
                        .map(PackageDependency::from_cargo)
                        .collect::<Vec<_>>(),
                )
            })
            .collect()
    }

    /// Returns the workspace root directory.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Returns all known packages.
    pub fn packages(&self) -> &[Package] {
        &self.packages
    }

    /// Returns one package by normalized package id.
    pub fn package(&self, package_id: &PackageId) -> Option<&Package> {
        let slot = self.package_by_id.get(package_id).copied()?;
        self.packages.get(slot)
    }

    /// Iterates over packages that belong to the analyzed workspace.
    pub fn workspace_packages(&self) -> impl Iterator<Item = &Package> + '_ {
        self.packages
            .iter()
            .filter(|package| package.is_workspace_member)
    }
}

/// Stable package identifier derived from Cargo metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_more::Display)]
#[display("{_0}")]
pub struct PackageId(String);

impl PackageId {
    fn from_cargo(id: &cargo_metadata::PackageId) -> Self {
        Self(id.to_string())
    }
}

/// Normalized package metadata relevant to later analysis phases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub id: PackageId,
    pub name: String,
    pub is_workspace_member: bool,
    pub manifest_path: PathBuf,
    pub targets: Vec<Target>,
    pub dependencies: Vec<PackageDependency>,
}

/// Normalized target metadata with one target kind per target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub kind: TargetKind,
    pub src_path: PathBuf,
}

impl Target {
    fn from_cargo(target: &cargo_metadata::Target) -> Self {
        Self {
            name: target.name.to_string(),
            kind: TargetKind::from_cargo(target),
            src_path: target.src_path.as_std_path().to_path_buf(),
        }
    }
}

/// One dependency edge after Cargo resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDependency {
    package: PackageId,
    name: String,
    is_normal: bool,
    is_build: bool,
    is_dev: bool,
}

impl PackageDependency {
    fn from_cargo(dependency: &cargo_metadata::NodeDep) -> Self {
        let mut is_normal = dependency.dep_kinds.is_empty();
        let mut is_build = false;
        let mut is_dev = false;

        // Cargo may report separate platform-specific entries for the same dependency kind.
        // Until we analyze a concrete target platform, each listed kind is potentially relevant.
        for kind in &dependency.dep_kinds {
            match kind.kind {
                cargo_metadata::DependencyKind::Normal => is_normal = true,
                cargo_metadata::DependencyKind::Development => is_dev = true,
                cargo_metadata::DependencyKind::Build => is_build = true,
                // Keep future Cargo dependency kinds resolvable instead of silently dropping them.
                cargo_metadata::DependencyKind::Unknown => is_normal = true,
            }
        }

        Self {
            package: PackageId::from_cargo(&dependency.pkg),
            name: dependency.name.clone(),
            is_normal,
            is_build,
            is_dev,
        }
    }

    /// Returns the resolved package this dependency points to.
    pub fn package_id(&self) -> &PackageId {
        &self.package
    }

    /// Returns the crate name used by source code to refer to this dependency.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns whether this edge is visible to normal package targets.
    pub fn is_normal(&self) -> bool {
        self.is_normal
    }

    /// Returns whether this edge is visible to build scripts.
    pub fn is_build(&self) -> bool {
        self.is_build
    }

    /// Returns whether this edge is visible to dev targets.
    pub fn is_dev(&self) -> bool {
        self.is_dev
    }

    /// Returns whether this dependency can be named from a target of the given kind.
    pub fn applies_to_target(&self, target_kind: &TargetKind) -> bool {
        match target_kind {
            TargetKind::CustomBuild => self.is_build,
            TargetKind::Example | TargetKind::Test | TargetKind::Bench => {
                self.is_normal || self.is_dev
            }
            TargetKind::Lib | TargetKind::Bin | TargetKind::Other(_) => self.is_normal,
        }
    }
}

/// Analysis-relevant target kinds.
///
/// We intentionally support less kinds than `cargo_metadata`,
/// since we are only interested in the kinds that are useful
/// for analysis.
#[derive(Debug, Clone, PartialEq, Eq, Hash, derive_more::Display)]
pub enum TargetKind {
    #[display("lib")]
    Lib,
    #[display("bin")]
    Bin,
    #[display("example")]
    Example,
    #[display("test")]
    Test,
    #[display("bench")]
    Bench,
    #[display("custom-build")]
    CustomBuild,
    #[display("{_0}")]
    Other(String),
}

impl TargetKind {
    fn from_cargo(target: &cargo_metadata::Target) -> Self {
        if target.is_kind(cargo_metadata::TargetKind::Lib) {
            Self::Lib
        } else if target.is_kind(cargo_metadata::TargetKind::Bin) {
            Self::Bin
        } else if target.is_kind(cargo_metadata::TargetKind::Example) {
            Self::Example
        } else if target.is_kind(cargo_metadata::TargetKind::Test) {
            Self::Test
        } else if target.is_kind(cargo_metadata::TargetKind::Bench) {
            Self::Bench
        } else if target.is_kind(cargo_metadata::TargetKind::CustomBuild) {
            Self::CustomBuild
        } else {
            let fallback = target
                .kind
                .first()
                .map(|kind| kind.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            Self::Other(fallback)
        }
    }

    pub fn is_lib(&self) -> bool {
        matches!(self, Self::Lib)
    }

    pub fn is_custom_build(&self) -> bool {
        matches!(self, Self::CustomBuild)
    }

    // Used for predictable ordering, e.g.
    // in test snapshots.
    pub fn sort_order(&self) -> u8 {
        match self {
            Self::Lib => 0,
            Self::Bin => 1,
            Self::Example => 2,
            Self::Test => 3,
            Self::Bench => 4,
            Self::CustomBuild => 5,
            Self::Other(_) => 6,
        }
    }
}
