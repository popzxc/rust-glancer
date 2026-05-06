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

/// Builder for a fresh def-map snapshot.
pub struct DefMapDbBuilder<'a, 'names> {
    workspace: &'a WorkspaceMetadata,
    parse: &'a rg_parse::ParseDb,
    item_tree: &'a ItemTreeDb,
    interner: NameInternerSource<'names>,
}

impl<'a> DefMapDbBuilder<'a, 'static> {
    pub(crate) fn new(
        workspace: &'a WorkspaceMetadata,
        parse: &'a rg_parse::ParseDb,
        item_tree: &'a ItemTreeDb,
    ) -> Self {
        DefMapDbBuilder {
            workspace,
            parse,
            item_tree,
            interner: NameInternerSource::Owned(NameInterner::new()),
        }
    }
}

impl<'a, 'names> DefMapDbBuilder<'a, 'names> {
    pub fn name_interner(self, interner: &'names mut NameInterner) -> DefMapDbBuilder<'a, 'names> {
        DefMapDbBuilder {
            workspace: self.workspace,
            parse: self.parse,
            item_tree: self.item_tree,
            interner: NameInternerSource::Borrowed(interner),
        }
    }

    pub fn build(mut self) -> anyhow::Result<DefMapDb> {
        let mut db = clean::build_db(
            self.workspace,
            self.parse,
            self.item_tree,
            self.interner.as_mut(),
        )?;
        db.mutator().shrink_to_fit();
        Ok(db)
    }
}

enum NameInternerSource<'names> {
    Owned(NameInterner),
    Borrowed(&'names mut NameInterner),
}

impl NameInternerSource<'_> {
    fn as_mut(&mut self) -> &mut NameInterner {
        match self {
            Self::Owned(interner) => interner,
            Self::Borrowed(interner) => interner,
        }
    }
}

/// Builder for a new def-map snapshot that reuses unchanged packages from an old snapshot.
pub struct DefMapDbPackageRebuilder<'a, 'db> {
    old: &'a DefMapDb,
    old_read: &'a DefMapReadTxn<'db>,
    workspace: &'a WorkspaceMetadata,
    parse: &'a rg_parse::ParseDb,
    item_tree: &'a ItemTreeDb,
    packages: &'a [PackageSlot],
    interner: &'a mut NameInterner,
}

impl<'a, 'db> DefMapDbPackageRebuilder<'a, 'db> {
    pub(crate) fn new(
        old: &'a DefMapDb,
        old_read: &'a DefMapReadTxn<'db>,
        workspace: &'a WorkspaceMetadata,
        parse: &'a rg_parse::ParseDb,
        item_tree: &'a ItemTreeDb,
        packages: &'a [PackageSlot],
        interner: &'a mut NameInterner,
    ) -> Self {
        DefMapDbPackageRebuilder {
            old,
            old_read,
            workspace,
            parse,
            item_tree,
            packages,
            interner,
        }
    }

    pub fn build(self) -> anyhow::Result<DefMapDb> {
        let mut db = rebuild::rebuild_packages(
            self.old,
            self.old_read,
            self.workspace,
            self.parse,
            self.item_tree,
            self.packages,
            self.interner,
        )?;
        db.mutator().shrink_packages(self.packages);
        Ok(db)
    }
}
