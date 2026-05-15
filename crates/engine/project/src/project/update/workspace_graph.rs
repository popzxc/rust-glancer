//! Detects changes that invalidate the workspace package graph.
//!
//! Source saves can usually reuse package and target slots, but manifest or lockfile edits may
//! add, remove, or reorder packages, dependencies, or targets. Those graph-level changes are
//! uncommon enough that the project intentionally treats them as a full-project rebuild boundary
//! instead of forcing every downstream phase to support slot remapping.
//!
//! Saved paths are canonicalized by `Project`, and workspace metadata paths are canonicalized when
//! `WorkspaceMetadata` is built. That lets this module express graph checks as direct path
//! comparisons instead of carrying defensive path-normalization fallbacks.

use std::{
    ffi::OsStr,
    path::{Component, Path},
};

use rg_parse::ParseDb;
use rg_workspace::WorkspaceMetadata;

use crate::project::SavedFileChange;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkspaceGraphChanges {
    Changed,
    Unchanged,
}

impl WorkspaceGraphChanges {
    pub(super) fn check(
        workspace: &WorkspaceMetadata,
        parse: &ParseDb,
        change: &SavedFileChange,
    ) -> Self {
        let workspace_lockfile = workspace.workspace_root().join("Cargo.lock");
        let workspace_manifest = workspace.workspace_root().join("Cargo.toml");
        let path = change.path.as_path();

        // If `Cargo.lock` in workspace changed (e.g. `cargo update`, rebuild).
        if path == workspace_lockfile {
            return Self::Changed;
        }

        // If any of `Cargo.toml` files changed, rebuild.
        // TODO: Is that needed/sufficient? If new dep is added, it might not be in `Cargo` cache
        // though probably `cargo check` will update `Cargo.lock` and it will trigger the rebuild
        // right after if that's the case. Low priority, to be tested later.
        if path.file_name() == Some(OsStr::new("Cargo.toml"))
            && (path == workspace_manifest
                || workspace
                    .workspace_packages()
                    .any(|package| package.manifest_path == path))
        {
            return Self::Changed;
        }

        if path.extension() != Some(OsStr::new("rs")) || parse.contains_file_path(path) {
            return Self::Unchanged;
        }

        // This is deliberately a conservative heuristic for Cargo's default target
        // autodiscovery. Parsing each manifest just to honor rare `autotests = false`-style
        // settings is overkill for now; a full metadata reload asks Cargo for the final truth
        // if the saved path merely looks like it could introduce a target.
        for package in workspace.workspace_packages() {
            let package_root = package.root_dir();
            if path == package_root.join("src").join("main.rs") {
                return Self::Changed;
            }

            let autodiscovery_roots = [
                package_root.join("src").join("bin"),
                package_root.join("examples"),
                package_root.join("tests"),
                package_root.join("benches"),
            ];

            if autodiscovery_roots.iter().any(|root| {
                path.strip_prefix(root)
                    .is_ok_and(is_auto_discovered_target_file)
            }) {
                return Self::Changed;
            }
        }

        Self::Unchanged
    }
}

fn is_auto_discovered_target_file(path_in_target_dir: &Path) -> bool {
    let mut components = path_in_target_dir.components();
    let Some(Component::Normal(target_name)) = components.next() else {
        return false;
    };

    let Some(target_root) = components.next() else {
        return Path::new(target_name).extension() == Some(OsStr::new("rs"));
    };

    components.next().is_none()
        && !target_name.is_empty()
        && target_root.as_os_str() == OsStr::new("main.rs")
}
