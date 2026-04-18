use anyhow::Context as _;
use ra_syntax::ast::{self, HasModuleItem, HasName};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::item_tree::{
    file::{FileId, ParseDb},
    item::ItemNode,
    span::LineIndex,
};

/// Stable identifier of a target within a package index build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetId(pub usize);

/// Input metadata needed to build one target tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetInput {
    /// Cargo target name.
    pub name: String,
    /// Cargo target kinds (`lib`, `bin`, `test`, ...).
    pub kinds: Vec<String>,
    /// Entrypoint source file for this target.
    pub root_file: PathBuf,
}

/// Final tree output for a single Cargo target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIndex {
    /// Stable target id assigned during package build.
    pub id: TargetId,
    /// Cargo target name.
    pub name: String,
    /// Cargo target kinds (`lib`, `bin`, `test`, ...).
    pub kinds: Vec<String>,
    /// Entrypoint file id for this target.
    pub root_file: FileId,
    /// Collected root-level items for this target.
    pub root_items: Vec<ItemNode>,
}

/// Mutable traversal state used while building one target tree.
#[derive(Default)]
struct TargetBuildState {
    active_stack: HashSet<FileId>,
}

/// Target-local tree builder that traverses module/item structure using `ParseDb`.
pub(crate) struct TargetBuilder<'db> {
    parse_db: &'db mut ParseDb,
    state: TargetBuildState,
}

impl<'db> TargetBuilder<'db> {
    /// Creates a target builder that reuses the shared parse database.
    pub(crate) fn new(parse_db: &'db mut ParseDb) -> Self {
        Self {
            parse_db,
            state: TargetBuildState::default(),
        }
    }

    /// Builds the item tree for one target entrypoint.
    pub(crate) fn build(
        mut self,
        target_id: TargetId,
        target_input: TargetInput,
    ) -> anyhow::Result<TargetIndex> {
        let root_file = self
            .parse_db
            .get_or_parse_file(&target_input.root_file)
            .with_context(|| {
                format!(
                    "while attempting to parse target root {}",
                    target_input.root_file.display()
                )
            })?;

        let root_items = self.collect_file_items(root_file).with_context(|| {
            format!(
                "while attempting to collect root items for target {}",
                target_input.name
            )
        })?;

        Ok(TargetIndex {
            id: target_id,
            name: target_input.name,
            kinds: target_input.kinds,
            root_file,
            root_items,
        })
    }

    /// Collects all top-level items from a file, with cycle protection.
    fn collect_file_items(&mut self, current_file_id: FileId) -> anyhow::Result<Vec<ItemNode>> {
        if !self.state.active_stack.insert(current_file_id) {
            return Ok(Vec::new());
        }

        let (items, line_index) = {
            let parsed_file = self
                .parse_db
                .parsed_file(current_file_id)
                .with_context(|| {
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

        self.state.active_stack.remove(&current_file_id);
        Ok(nodes)
    }

    /// Maps syntax items from one file into normalized `ItemNode` values.
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
                    let children = self
                        .collect_module_children(&item, current_file_id, line_index)
                        .with_context(|| {
                            format!(
                                "while attempting to collect module children for {}",
                                module_name.as_deref().unwrap_or("<unnamed>")
                            )
                        })?;
                    Some(ItemNode::new_module(
                        item,
                        children,
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

    /// Collects module children from inline blocks or resolved module files.
    fn collect_module_children(
        &mut self,
        item: &ast::Module,
        current_file_id: FileId,
        line_index: &LineIndex,
    ) -> anyhow::Result<Vec<ItemNode>> {
        if let Some(item_list) = item.item_list() {
            let inline_items = item_list.items().collect::<Vec<_>>();
            return self
                .collect_items(inline_items, current_file_id, line_index)
                .context("while attempting to collect inline module items");
        }

        let Some(module_name) = item.name().map(|name| name.text().to_string()) else {
            return Ok(Vec::new());
        };
        let current_file_path = self.parse_db.file_path(current_file_id).with_context(|| {
            format!(
                "while attempting to resolve current file {:?}",
                current_file_id
            )
        })?;

        // TODO: support `#[path = "..."]` and other advanced module-resolution rules when needed.
        let Some(module_file_path) = Self::resolve_module_file(current_file_path, &module_name)
        else {
            return Ok(Vec::new());
        };

        let module_file_id = self
            .parse_db
            .get_or_parse_file(&module_file_path)
            .with_context(|| {
                format!(
                    "while attempting to parse module file {}",
                    module_file_path.display()
                )
            })?;

        self.collect_file_items(module_file_id).with_context(|| {
            format!(
                "while attempting to collect module items from {}",
                module_file_path.display()
            )
        })
    }

    /// Resolves `mod foo;` according to conventional Rust module file rules.
    fn resolve_module_file(current_file_path: &Path, module_name: &str) -> Option<PathBuf> {
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
}
