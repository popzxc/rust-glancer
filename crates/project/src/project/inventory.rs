//! Residency-independent project inventory.
//!
//! Analysis payloads can be offloaded, but package slots, target ids, and parsed file ids are part
//! of the project graph itself. This module is the high-level place for code that needs to ask
//! "what exists in this workspace snapshot?" without accidentally depending on which packages are
//! currently resident in phase databases.

use std::path::Path;

use rg_def_map::{PackageSlot, TargetRef};
use rg_parse::{FileId, ParseDb};
use rg_workspace::WorkspaceMetadata;

/// Read-only graph inventory for one built project snapshot.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProjectInventory<'a> {
    workspace: &'a WorkspaceMetadata,
    parse: &'a ParseDb,
}

impl<'a> ProjectInventory<'a> {
    pub(crate) fn new(workspace: &'a WorkspaceMetadata, parse: &'a ParseDb) -> Self {
        Self { workspace, parse }
    }

    /// Iterates over every package slot in the project graph, including offloadable packages.
    pub(crate) fn all_packages(&self) -> impl Iterator<Item = InventoryPackage<'a>> + '_ {
        self.workspace
            .packages()
            .iter()
            .zip(self.parse.packages())
            .enumerate()
            .map(|(package_idx, (metadata, parsed))| InventoryPackage {
                slot: PackageSlot(package_idx),
                metadata,
                parsed,
            })
    }

    /// Iterates over non-sysroot packages that are part of the user-visible Cargo graph.
    pub(crate) fn non_sysroot_packages(&self) -> impl Iterator<Item = InventoryPackage<'a>> + '_ {
        self.all_packages().filter(|package| !package.is_sysroot())
    }

    /// Returns all targets declared by the given package slot.
    pub(crate) fn target_refs_for_package(&self, package: PackageSlot) -> Vec<TargetRef> {
        let Some(parsed_package) = self.parse.package(package.0) else {
            return Vec::new();
        };

        parsed_package
            .targets()
            .iter()
            .map(|target| TargetRef {
                package,
                target: target.id,
            })
            .collect()
    }

    /// Returns all files currently parsed for the given canonical path.
    pub(crate) fn file_refs_for_path(&self, canonical_path: &Path) -> Vec<ProjectFileRef> {
        let mut files = Vec::new();

        for package in self.all_packages() {
            for parsed_file in package.parsed.parsed_files() {
                if parsed_file.path() != canonical_path {
                    continue;
                }

                files.push(ProjectFileRef {
                    package: package.slot,
                    file: parsed_file.file_id(),
                });
            }
        }

        files
    }
}

/// One package in the residency-independent project graph.
#[derive(Debug, Clone, Copy)]
pub(crate) struct InventoryPackage<'a> {
    slot: PackageSlot,
    metadata: &'a rg_workspace::Package,
    parsed: &'a rg_parse::Package,
}

impl<'a> InventoryPackage<'a> {
    pub(crate) fn slot(self) -> PackageSlot {
        self.slot
    }

    pub(crate) fn is_sysroot(self) -> bool {
        self.metadata.origin.is_sysroot()
    }
}

/// One package-local parsed file in the project graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ProjectFileRef {
    pub package: PackageSlot,
    pub file: FileId,
}
