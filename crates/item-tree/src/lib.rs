mod item;
mod lower;
mod memsize;

#[cfg(test)]
mod tests;

use anyhow::Context as _;
use rg_arena::Arena;
use rg_parse::{FileId, ParseDb, TargetId};
use rg_text::NameInterner;

pub use self::item::{
    ConstItem, Documentation, EnumItem, EnumVariantItem, ExternCrateItem, FieldItem, FieldKey,
    FieldList, FunctionItem, GenericArg, GenericParams, ImplItem, ImportAlias, ItemKind, ItemNode,
    ItemTag, ItemTreeId, ItemTreeRef, ModuleItem, ModuleSource, Mutability, ParamItem, ParamKind,
    StaticItem, StructItem, TraitItem, TypeAliasItem, TypeBound, TypePath, TypePathSegment,
    TypeRef, UnionItem, UseImport, UseImportKind, UseItem, UsePath, UsePathSegment,
    UsePathSegmentKind, VisibilityLevel, WherePredicate,
};
pub use rg_text::Name;

/// Lowered item trees for all parsed packages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemTreeDb {
    packages: Vec<Option<Package>>,
}

impl ItemTreeDb {
    /// Builds file-local item trees on top of the parsed source database.
    pub fn build(parse: &mut ParseDb) -> anyhow::Result<Self> {
        let mut interner = NameInterner::new();
        Self::build_with_interner(parse, &mut interner)
    }

    /// Builds file-local item trees using a caller-retained name interner.
    pub fn build_with_interner(
        parse: &mut ParseDb,
        interner: &mut NameInterner,
    ) -> anyhow::Result<Self> {
        let package_count = parse.package_count();
        let packages = (0..package_count).collect::<Vec<_>>();
        Self::build_packages_with_interner(parse, &packages, interner)
    }

    /// Builds item trees only for selected packages.
    ///
    /// Project rebuilds use this as a temporary lowering input: affected packages are populated,
    /// while unrelated packages stay absent so accidental cross-package item-tree access fails
    /// loudly instead of retaining the whole item-tree graph.
    pub fn build_packages(parse: &mut ParseDb, packages: &[usize]) -> anyhow::Result<Self> {
        let mut interner = NameInterner::new();
        Self::build_packages_with_interner(parse, packages, &mut interner)
    }

    /// Builds selected packages using a caller-retained name interner.
    pub fn build_packages_with_interner(
        parse: &mut ParseDb,
        packages: &[usize],
        interner: &mut NameInterner,
    ) -> anyhow::Result<Self> {
        let mut trees = Self {
            packages: vec![None; parse.package_count()],
        };
        for package_slot in normalized_package_slots(packages) {
            let package = parse.package_mut(package_slot).with_context(|| {
                format!("while attempting to fetch parsed package {package_slot}")
            })?;
            let lowered = lower::build_package(package, interner).with_context(|| {
                format!(
                    "while attempting to build item trees for package {}",
                    package.package_name()
                )
            })?;
            trees.packages[package_slot] = Some(lowered);
        }

        Ok(trees)
    }

    /// Returns one package tree set by slot.
    pub fn package(&self, package_slot: usize) -> Option<&Package> {
        self.packages.get(package_slot)?.as_ref()
    }
}

fn normalized_package_slots(packages: &[usize]) -> Vec<usize> {
    let mut packages = packages.to_vec();
    packages.sort_unstable();
    packages.dedup();
    packages
}

/// Item trees for all files inside one parsed package, plus target entrypoints.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Package {
    files: Arena<FileId, Option<FileTree>>,
    target_roots: Arena<TargetId, TargetRoot>,
}

impl Package {
    /// Returns all file trees.
    pub fn files(&self) -> impl Iterator<Item = &FileTree> {
        self.files.iter().filter_map(Option::as_ref)
    }

    /// Returns one file tree by parsed file id.
    pub fn file(&self, file_id: FileId) -> Option<&FileTree> {
        self.files.get(file_id)?.as_ref()
    }

    /// Returns one lowered item by stable item-tree reference.
    pub fn item(&self, item_ref: ItemTreeRef) -> Option<&ItemNode> {
        self.file(item_ref.file_id)?.item(item_ref.item)
    }

    /// Returns all target roots.
    pub fn target_roots(&self) -> &[TargetRoot] {
        self.target_roots.as_slice()
    }

    /// Returns one target root by parsed target id.
    pub fn target_root(&self, target_id: TargetId) -> Option<&TargetRoot> {
        self.target_roots.get(target_id)
    }
}

/// File-local lowered item tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTree {
    pub file: FileId,
    pub docs: Option<Documentation>,
    pub top_level: Vec<ItemTreeId>,
    pub items: Arena<ItemTreeId, ItemNode>,
}

impl FileTree {
    /// Returns one file-local item-tree node by id.
    pub fn item(&self, item_id: ItemTreeId) -> Option<&ItemNode> {
        self.items.get(item_id)
    }
}

/// Target entrypoint into file-local item trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetRoot {
    pub target: TargetId,
    pub root_file: FileId,
}
