mod item;
mod lower;

#[cfg(test)]
mod tests;

use anyhow::Context as _;

use crate::parse::{FileId, ParseDb, TargetId};

pub(crate) use self::item::{
    ConstItem, EnumItem, EnumVariantItem, ExternCrateItem, FieldItem, FieldList, FunctionItem,
    GenericParams, ImplItem, ImportAlias, ItemKind, ItemNode, ItemTag, ItemTreeId, ItemTreeRef,
    ModuleItem, ModuleSource, Mutability, ParamKind, StaticItem, StructItem, TraitItem,
    TypeAliasItem, TypeBound, TypeRef, UnionItem, UseImport, UseImportKind, UseItem, UsePath,
    UsePathSegment, VisibilityLevel,
};

#[cfg(test)]
pub(crate) use self::item::ParamItem;

/// Lowered item trees for all parsed packages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemTreeDb {
    packages: Vec<Package>,
}

impl ItemTreeDb {
    /// Builds file-local item trees on top of the parsed source database.
    pub(crate) fn build(parse: &mut ParseDb) -> anyhow::Result<Self> {
        let package_count = parse.packages().len();
        let mut packages = Vec::with_capacity(package_count);

        for package in parse.packages_mut() {
            packages.push(
                lower::build_package(&mut package.files, &package.targets).with_context(|| {
                    format!(
                        "while attempting to build item trees for package {}",
                        package.package_name()
                    )
                })?,
            );
        }

        Ok(Self { packages })
    }

    /// Returns one package tree set by slot.
    pub(crate) fn package(&self, package_slot: usize) -> Option<&Package> {
        self.packages.get(package_slot)
    }
}

/// Item trees for all files inside one parsed package, plus target entrypoints.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Package {
    files: Vec<Option<FileTree>>,
    target_roots: Vec<TargetRoot>,
}

impl Package {
    /// Returns all file trees.
    pub(crate) fn files(&self) -> impl Iterator<Item = &FileTree> {
        self.files.iter().filter_map(Option::as_ref)
    }

    /// Returns one file tree by parsed file id.
    pub(crate) fn file(&self, file_id: FileId) -> Option<&FileTree> {
        self.files.get(file_id.0)?.as_ref()
    }

    /// Returns one lowered item by stable item-tree reference.
    pub(crate) fn item(&self, item_ref: ItemTreeRef) -> Option<&ItemNode> {
        self.file(item_ref.file_id)?.item(item_ref.item)
    }

    /// Returns all target roots.
    pub(crate) fn target_roots(&self) -> &[TargetRoot] {
        &self.target_roots
    }

    /// Returns one target root by parsed target id.
    pub(crate) fn target_root(&self, target_id: TargetId) -> Option<&TargetRoot> {
        self.target_roots
            .iter()
            .find(|target| target.target == target_id)
    }
}

/// File-local lowered item tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTree {
    pub file: FileId,
    pub top_level: Vec<ItemTreeId>,
    pub items: Vec<ItemNode>,
}

impl FileTree {
    /// Returns one file-local item-tree node by id.
    pub(crate) fn item(&self, item_id: ItemTreeId) -> Option<&ItemNode> {
        self.items.get(item_id.0)
    }
}

/// Target entrypoint into file-local item trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetRoot {
    pub target: TargetId,
    pub root_file: FileId,
}
