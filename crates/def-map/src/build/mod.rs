//! Builds and rebuilds frozen def-map snapshots.
//!
//! Target collection intentionally stops before cross-target facts such as implicit roots,
//! preludes, and imports are fully known. Clean builds and package rebuilds now share one
//! finalization model:
//! - packages with fresh `TargetState`s are "dirty" and receive fixed-point import resolution;
//! - packages without fresh states are read from an optional frozen baseline;
//! - a clean build has no baseline and marks every package dirty;
//! - a package rebuild has an old baseline and marks only affected packages dirty.

mod clean;
mod finalize;
mod implicit_roots;
mod imports;
mod rebuild;

use rg_item_tree::ItemTreeDb;
use rg_parse;
use rg_text::NameInterner;
use rg_workspace::WorkspaceMetadata;

use crate::{DefMapDb, DefMapReadTxn, PackageSlot};

pub(crate) struct DefMapDbBuilder;

impl DefMapDbBuilder {
    pub(crate) fn build_with_interner(
        workspace: &WorkspaceMetadata,
        parse: &rg_parse::ParseDb,
        item_tree: &ItemTreeDb,
        interner: &mut NameInterner,
    ) -> anyhow::Result<DefMapDb> {
        let mut db = clean::build_db(workspace, parse, item_tree, interner)?;
        db.mutator().shrink_to_fit();
        Ok(db)
    }

    pub(crate) fn rebuild_packages_with_interner_and_read_txn(
        old: &DefMapDb,
        old_read: &DefMapReadTxn<'_>,
        workspace: &WorkspaceMetadata,
        parse: &rg_parse::ParseDb,
        item_tree: &ItemTreeDb,
        packages: &[PackageSlot],
        interner: &mut NameInterner,
    ) -> anyhow::Result<DefMapDb> {
        let mut db = rebuild::rebuild_packages(
            old, old_read, workspace, parse, item_tree, packages, interner,
        )?;
        db.mutator().shrink_packages(packages);
        Ok(db)
    }
}
