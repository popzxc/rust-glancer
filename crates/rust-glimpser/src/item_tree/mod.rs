use std::{collections::HashSet, path::Path};

use anyhow::Context as _;
use ra_syntax::ast::{self, HasModuleItem, HasName};

mod item;

#[cfg(test)]
mod tests;

use crate::parse::{FileDb, FileId, ParseDb, Target as ParseTarget, TargetId, span::LineIndex};

pub(crate) use self::item::{
    ExternCrateItem, ImportAlias, ItemKind, ItemNode, ItemTag, ModuleItem, ModuleSource, UseImport,
    UseImportKind, UseItem, UsePath, UsePathSegment, VisibilityLevel,
};

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
                PackageTreeBuilder::new(&mut package.files)
                    .build(&package.targets)
                    .with_context(|| {
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
    pub items: Vec<ItemNode>,
}

/// Target entrypoint into file-local item trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetRoot {
    pub target: TargetId,
    pub root_file: FileId,
}

struct PackageTreeBuilder<'db> {
    files: &'db mut FileDb,
    active_stack: HashSet<FileId>,
    file_trees: Vec<Option<FileTree>>,
}

impl<'db> PackageTreeBuilder<'db> {
    fn new(files: &'db mut FileDb) -> Self {
        Self {
            files,
            active_stack: HashSet::default(),
            file_trees: Vec::new(),
        }
    }

    fn build(mut self, targets: &[ParseTarget]) -> anyhow::Result<Package> {
        let target_roots = targets
            .iter()
            .map(|target| {
                self.lower_file(target.root_file).with_context(|| {
                    format!(
                        "while attempting to lower root file for target {}",
                        target.cargo_target.name
                    )
                })?;
                Ok(TargetRoot {
                    target: target.id,
                    root_file: target.root_file,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(Package {
            files: self.file_trees,
            target_roots,
        })
    }

    fn lower_file(&mut self, current_file_id: FileId) -> anyhow::Result<()> {
        self.ensure_file_tree_slot(current_file_id);
        if self.file_trees[current_file_id.0].is_some() {
            return Ok(());
        }

        if !self.active_stack.insert(current_file_id) {
            return Ok(());
        }

        let (items, line_index) = {
            let parsed_file = self.files.parsed_file(current_file_id).with_context(|| {
                format!(
                    "while attempting to fetch parsed file {:?}",
                    current_file_id
                )
            })?;
            (
                parsed_file.tree.items().collect::<Vec<_>>(),
                parsed_file.line_index.clone(),
            )
        };

        let nodes = self
            .collect_items(items, current_file_id, &line_index)
            .with_context(|| {
                format!(
                    "while attempting to collect file items for {:?}",
                    current_file_id
                )
            })?;

        self.file_trees[current_file_id.0] = Some(FileTree {
            file: current_file_id,
            items: nodes,
        });
        self.active_stack.remove(&current_file_id);
        Ok(())
    }

    fn ensure_file_tree_slot(&mut self, file_id: FileId) {
        let required_len = file_id.0 + 1;
        if self.file_trees.len() < required_len {
            self.file_trees.resize_with(required_len, || None);
        }
    }

    fn collect_items(
        &mut self,
        items: Vec<ast::Item>,
        current_file_id: FileId,
        line_index: &LineIndex,
    ) -> anyhow::Result<Vec<ItemNode>> {
        let mut nodes = Vec::new();

        for item in items {
            let node = match item {
                ast::Item::AsmExpr(item) => {
                    Some(ItemNode::new_asm_expr(item, current_file_id, line_index))
                }
                ast::Item::Const(item) => {
                    Some(ItemNode::new_const(item, current_file_id, line_index))
                }
                ast::Item::Enum(item) => {
                    Some(ItemNode::new_enum(item, current_file_id, line_index))
                }
                ast::Item::ExternBlock(item) => Some(ItemNode::new_extern_block(
                    item,
                    current_file_id,
                    line_index,
                )),
                ast::Item::ExternCrate(item) => Some(ItemNode::new_extern_crate(
                    item,
                    current_file_id,
                    line_index,
                )),
                ast::Item::Fn(item) => Some(ItemNode::new_fn(item, current_file_id, line_index)),
                ast::Item::Impl(item) => {
                    Some(ItemNode::new_impl_block(item, current_file_id, line_index))
                }
                ast::Item::MacroCall(_) => None,
                ast::Item::MacroDef(item) => {
                    Some(ItemNode::new_macro_def(item, current_file_id, line_index))
                }
                ast::Item::MacroRules(item) => {
                    Some(ItemNode::new_macro_rules(item, current_file_id, line_index))
                }
                ast::Item::Module(item) => {
                    let module_name = item.name().map(|name| name.text().to_string());
                    let module_item = self
                        .collect_module(&item, current_file_id, line_index)
                        .with_context(|| {
                            format!(
                                "while attempting to collect module item for {}",
                                module_name.as_deref().unwrap_or("<unnamed>")
                            )
                        })?;
                    Some(ItemNode::new_module(
                        item,
                        module_item,
                        current_file_id,
                        line_index,
                    ))
                }
                ast::Item::Static(item) => {
                    Some(ItemNode::new_static(item, current_file_id, line_index))
                }
                ast::Item::Struct(item) => {
                    Some(ItemNode::new_struct(item, current_file_id, line_index))
                }
                ast::Item::Trait(item) => {
                    Some(ItemNode::new_trait(item, current_file_id, line_index))
                }
                ast::Item::TypeAlias(item) => {
                    Some(ItemNode::new_type_alias(item, current_file_id, line_index))
                }
                ast::Item::Union(item) => {
                    Some(ItemNode::new_union(item, current_file_id, line_index))
                }
                ast::Item::Use(item) => Some(ItemNode::new_use(item, current_file_id, line_index)),
            };

            if let Some(node) = node {
                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    fn collect_module(
        &mut self,
        item: &ast::Module,
        current_file_id: FileId,
        line_index: &LineIndex,
    ) -> anyhow::Result<ModuleItem> {
        if let Some(item_list) = item.item_list() {
            let inline_items = item_list.items().collect::<Vec<_>>();
            let items = self
                .collect_items(inline_items, current_file_id, line_index)
                .context("while attempting to collect inline module items")?;
            return Ok(ModuleItem {
                source: ModuleSource::Inline { items },
            });
        }

        let Some(module_name) = item.name().map(|name| name.text().to_string()) else {
            return Ok(ModuleItem {
                source: ModuleSource::OutOfLine {
                    definition_file: None,
                },
            });
        };
        let current_file_path = self.files.file_path(current_file_id).with_context(|| {
            format!(
                "while attempting to resolve current file {:?}",
                current_file_id
            )
        })?;

        // TODO: support `#[path = \"...\"]` and other advanced module-resolution rules when needed.
        let Some(module_file_path) = resolve_module_file(current_file_path, &module_name) else {
            return Ok(ModuleItem {
                source: ModuleSource::OutOfLine {
                    definition_file: None,
                },
            });
        };

        let module_file_id = self
            .files
            .get_or_parse_file(&module_file_path)
            .with_context(|| {
                format!(
                    "while attempting to parse module file {}",
                    module_file_path.display()
                )
            })?;

        self.lower_file(module_file_id).with_context(|| {
            format!(
                "while attempting to collect module items from {}",
                module_file_path.display()
            )
        })?;

        Ok(ModuleItem {
            source: ModuleSource::OutOfLine {
                definition_file: Some(module_file_id),
            },
        })
    }
}

/// Resolves `mod foo;` according to conventional Rust module file rules.
pub(crate) fn resolve_module_file(
    current_file_path: &Path,
    module_name: &str,
) -> Option<std::path::PathBuf> {
    let parent_dir = current_file_path.parent()?;
    let file_name = current_file_path.file_name()?.to_str()?;
    let file_stem = current_file_path.file_stem()?.to_str()?;

    let module_parent = match file_name {
        "lib.rs" | "main.rs" | "mod.rs" => parent_dir.to_path_buf(),
        _ => parent_dir.join(file_stem),
    };

    let flat_file = module_parent.join(format!("{module_name}.rs"));
    if flat_file.exists() {
        return Some(flat_file);
    }

    let nested_file = module_parent.join(module_name).join("mod.rs");
    if nested_file.exists() {
        return Some(nested_file);
    }

    None
}
