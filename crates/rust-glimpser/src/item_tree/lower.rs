//! AST-to-item-tree lowering for one parsed package.
//!
//! This phase is deliberately file-oriented: each source file is lowered once into a `FileTree`,
//! and targets only point at their root file. Out-of-line modules therefore reuse the same lowered
//! file tree whenever multiple targets reach them.

use std::{collections::HashSet, path::Path};

use anyhow::Context as _;
use ra_syntax::{
    AstNode as _,
    ast::{self, HasModuleItem, HasName, HasVisibility},
};

use crate::parse::{FileDb, FileId, Target as ParseTarget, span::LineIndex};

use super::{
    ExternCrateItem, FileTree, ItemKind, ItemNode, ModuleItem, ModuleSource, Package, TargetRoot,
    UseItem, VisibilityLevel,
};

/// Lowers all known files for one parsed package and records target entrypoints into them.
pub(super) fn build_package(
    files: &mut FileDb,
    targets: &[ParseTarget],
) -> anyhow::Result<Package> {
    PackageLowering::new(files).build(targets)
}

/// Mutable lowering context shared while walking all target roots in one package.
///
/// `file_trees` is the cache being built, and `active_stack` prevents infinite recursion while
/// following out-of-line `mod foo;` chains.
struct PackageLowering<'db> {
    files: &'db mut FileDb,
    active_stack: HashSet<FileId>,
    file_trees: Vec<Option<FileTree>>,
}

impl<'db> PackageLowering<'db> {
    fn new(files: &'db mut FileDb) -> Self {
        Self {
            files,
            active_stack: HashSet::default(),
            file_trees: Vec::new(),
        }
    }

    /// Starts from every target root file and lowers the reachable file set once.
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

    /// Lowers one file into a cached `FileTree` unless it was already lowered earlier.
    fn lower_file(&mut self, current_file_id: FileId) -> anyhow::Result<()> {
        self.ensure_file_tree_slot(current_file_id);
        if self.file_trees[current_file_id.0].is_some() {
            return Ok(());
        }

        // Recursive module graphs can revisit a file before the first traversal finishes.
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

    /// Grows the sparse file-tree table so `file_id` can be addressed directly by index.
    fn ensure_file_tree_slot(&mut self, file_id: FileId) {
        let required_len = file_id.0 + 1;
        if self.file_trees.len() < required_len {
            self.file_trees.resize_with(required_len, || None);
        }
    }

    /// Lowers all top-level items from one file into item-tree nodes.
    fn collect_items(
        &mut self,
        items: Vec<ast::Item>,
        current_file_id: FileId,
        line_index: &LineIndex,
    ) -> anyhow::Result<Vec<ItemNode>> {
        let mut nodes = Vec::new();

        for item in items {
            let node = self
                .lower_item(item, current_file_id, line_index)
                .with_context(|| {
                    format!("while attempting to lower item in {:?}", current_file_id)
                })?;

            if let Some(node) = node {
                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    /// Lowers one syntax item into the corresponding item-tree node, when this item kind matters
    /// to later phases.
    fn lower_item(
        &mut self,
        item: ast::Item,
        current_file_id: FileId,
        line_index: &LineIndex,
    ) -> anyhow::Result<Option<ItemNode>> {
        let node = match item {
            ast::Item::AsmExpr(item) => Some(ItemNode::new(
                ItemKind::AsmExpr,
                None,
                VisibilityLevel::Private,
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
            ast::Item::Const(item) => Some(ItemNode::new(
                ItemKind::Const,
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
            ast::Item::Enum(item) => Some(ItemNode::new(
                ItemKind::Enum,
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
            ast::Item::ExternBlock(item) => Some(ItemNode::new(
                ItemKind::ExternBlock,
                None,
                VisibilityLevel::Private,
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
            ast::Item::ExternCrate(item) => Some(ItemNode::new(
                ItemKind::ExternCrate(Box::new(ExternCrateItem::from_ast(&item))),
                item.name_ref()
                    .map(|name_ref| name_ref.syntax().text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
            ast::Item::Fn(item) => Some(ItemNode::new(
                ItemKind::Function,
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
            ast::Item::Impl(item) => {
                // Associated items are intentionally not lowered here yet; they are not module-scope
                // definitions, and should become a separate item-tree concept once we need them.
                Some(ItemNode::new(
                    ItemKind::Impl,
                    None,
                    lower_visibility(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                ))
            }
            ast::Item::MacroCall(_) => None,
            ast::Item::MacroDef(item) => Some(ItemNode::new(
                ItemKind::MacroDefinition,
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
            ast::Item::MacroRules(item) => Some(ItemNode::new(
                ItemKind::MacroDefinition,
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
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
                Some(ItemNode::new(
                    ItemKind::Module(Box::new(module_item)),
                    module_name,
                    lower_visibility(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                ))
            }
            ast::Item::Static(item) => Some(ItemNode::new(
                ItemKind::Static,
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
            ast::Item::Struct(item) => Some(ItemNode::new(
                ItemKind::Struct,
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
            ast::Item::Trait(item) => Some(ItemNode::new(
                ItemKind::Trait,
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
            ast::Item::TypeAlias(item) => Some(ItemNode::new(
                ItemKind::TypeAlias,
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
            ast::Item::Union(item) => Some(ItemNode::new(
                ItemKind::Union,
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
            ast::Item::Use(item) => Some(ItemNode::new(
                ItemKind::Use(Box::new(UseItem::from_ast(&item))),
                normalized_use_name(&item),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
                current_file_id,
                line_index,
            )),
        };

        Ok(node)
    }

    /// Lowers one module declaration into either an inline item list or an out-of-line file link.
    fn collect_module(
        &mut self,
        item: &ast::Module,
        current_file_id: FileId,
        line_index: &LineIndex,
    ) -> anyhow::Result<ModuleItem> {
        if let Some(item_list) = item.item_list() {
            // Inline modules reuse the current file and embed their lowered child items directly.
            let inline_items = item_list.items().collect::<Vec<_>>();
            let items = self
                .collect_items(inline_items, current_file_id, line_index)
                .context("while attempting to collect inline module items")?;
            return Ok(ModuleItem {
                source: ModuleSource::Inline { items },
            });
        }

        // A nameless out-of-line module cannot be resolved to a file path.
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

        // TODO: support `#[path = "..."]` and other advanced module-resolution rules when needed.
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

        // Lower the target file eagerly so later phases can treat every module source uniformly.
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

/// Keeps the original `use ...` text in a compact, human-readable form for debugging and tests.
fn normalized_use_name(use_item: &ast::Use) -> Option<String> {
    let use_tree = use_item.use_tree()?;
    let text = use_tree.syntax().text().to_string();

    // Normalize all whitespace in an extracted syntax fragment to single spaces.
    Some(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Lowers syntax-level visibility into the smaller set currently tracked by the item tree.
fn lower_visibility(visibility: Option<ast::Visibility>) -> VisibilityLevel {
    let Some(visibility) = visibility else {
        return VisibilityLevel::Private;
    };

    let Some(inner) = visibility.visibility_inner() else {
        return VisibilityLevel::Public;
    };

    let Some(path) = inner.path() else {
        return VisibilityLevel::Unknown(visibility.syntax().text().to_string());
    };
    let path_text = path.syntax().text().to_string();

    if inner.in_token().is_some() {
        return VisibilityLevel::Restricted(path_text);
    }

    match path_text.as_str() {
        "crate" => VisibilityLevel::Crate,
        "super" => VisibilityLevel::Super,
        "self" => VisibilityLevel::Self_,
        _ => VisibilityLevel::Unknown(visibility.syntax().text().to_string()),
    }
}

/// Resolves `mod foo;` according to conventional Rust module file rules.
fn resolve_module_file(current_file_path: &Path, module_name: &str) -> Option<std::path::PathBuf> {
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
