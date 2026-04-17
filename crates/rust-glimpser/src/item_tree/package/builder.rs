use anyhow::Context as _;
use ra_syntax::{
    Edition, SourceFile,
    ast::{self, AstNode, HasModuleItem, HasName, HasVisibility},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::item_tree::{
    error::ParseError,
    file::{FileId, FileRecord},
    item::{ItemKind, ItemNode, VisibilityLevel},
    span::{LineIndex, Span},
    target::{TargetBuildState, TargetId, TargetIndex, TargetInput},
};

#[derive(Default)]
pub(super) struct PackageBuilder {
    pub(super) parsed_files: Vec<ParsedFile>,
    pub(super) file_ids_by_path: HashMap<PathBuf, FileId>,
}

pub(super) struct ParsedFile {
    pub(super) record: FileRecord,
    pub(super) line_index: LineIndex,
    pub(super) tree: SourceFile,
}

impl PackageBuilder {
    pub(super) fn build_target(
        &mut self,
        target_id: TargetId,
        target_input: TargetInput,
    ) -> anyhow::Result<TargetIndex> {
        let root_file = self
            .get_or_parse_file(&target_input.root_file)
            .with_context(|| {
                format!(
                    "while attempting to parse target root {}",
                    target_input.root_file.display()
                )
            })?;

        let mut state = TargetBuildState::default();
        let root_items = self
            .collect_file_items(root_file, &mut state)
            .with_context(|| {
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

    fn get_or_parse_file(&mut self, file_path: &Path) -> anyhow::Result<FileId> {
        let canonical_file_path = file_path
            .canonicalize()
            .with_context(|| format!("while attempting to canonicalize {}", file_path.display()))?;

        if let Some(file_id) = self.file_ids_by_path.get(&canonical_file_path) {
            return Ok(*file_id);
        }

        let source = std::fs::read_to_string(&canonical_file_path).with_context(|| {
            format!("while attempting to read {}", canonical_file_path.display())
        })?;
        let line_index = LineIndex::new(&source);
        let parsed_file = SourceFile::parse(&source, Edition::CURRENT);

        let file_id = FileId(self.parsed_files.len());
        let parse_errors = parsed_file
            .errors()
            .into_iter()
            .map(|error| ParseError {
                file_id,
                message: error.to_string(),
                span: Span::from_text_range(error.range(), &line_index),
            })
            .collect();

        let record = FileRecord {
            id: file_id,
            path: canonical_file_path.clone(),
            parse_errors,
        };
        self.parsed_files.push(ParsedFile {
            record,
            line_index,
            tree: parsed_file.tree(),
        });
        self.file_ids_by_path.insert(canonical_file_path, file_id);

        Ok(file_id)
    }

    fn collect_file_items(
        &mut self,
        current_file_id: FileId,
        state: &mut TargetBuildState,
    ) -> anyhow::Result<Vec<ItemNode>> {
        if !state.active_stack.insert(current_file_id) {
            return Ok(Vec::new());
        }

        let (items, line_index) = {
            let parsed_file = self.parsed_file(current_file_id).with_context(|| {
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
            .collect_items(items, current_file_id, &line_index, state)
            .with_context(|| {
                format!(
                    "while attempting to collect file items for {:?}",
                    current_file_id
                )
            })?;

        state.active_stack.remove(&current_file_id);
        Ok(nodes)
    }

    fn collect_items(
        &mut self,
        items: Vec<ast::Item>,
        current_file_id: FileId,
        line_index: &LineIndex,
        state: &mut TargetBuildState,
    ) -> anyhow::Result<Vec<ItemNode>> {
        let mut nodes = Vec::new();

        for item in items {
            let node = match item {
                ast::Item::AsmExpr(item) => Some(ItemNode::new(
                    ItemKind::AsmExpr,
                    None,
                    VisibilityLevel::Private,
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Const(item) => Some(ItemNode::new(
                    ItemKind::Const,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Enum(item) => Some(ItemNode::new(
                    ItemKind::Enum,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::ExternBlock(item) => Some(ItemNode::new(
                    ItemKind::ExternBlock,
                    None,
                    VisibilityLevel::Private,
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::ExternCrate(item) => Some(ItemNode::new(
                    ItemKind::ExternCrate,
                    item.name_ref()
                        .map(|name_ref| name_ref.syntax().text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Fn(item) => Some(ItemNode::new(
                    ItemKind::Function,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Impl(item) => {
                    let children = self.collect_impl_items(&item, current_file_id, line_index);
                    Some(ItemNode::new(
                        ItemKind::Impl,
                        None,
                        VisibilityLevel::from_ast(item.visibility()),
                        item.syntax().text_range(),
                        current_file_id,
                        line_index,
                        children,
                    ))
                }
                ast::Item::MacroCall(_) => None,
                ast::Item::MacroDef(item) => Some(ItemNode::new(
                    ItemKind::MacroDefinition,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::MacroRules(item) => Some(ItemNode::new(
                    ItemKind::MacroDefinition,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Module(item) => {
                    let module_name = item.name().map(|name| name.text().to_string());
                    let children = self
                        .collect_module_children(&item, current_file_id, line_index, state)
                        .with_context(|| {
                            format!(
                                "while attempting to collect module children for {}",
                                module_name.as_deref().unwrap_or("<unnamed>")
                            )
                        })?;
                    Some(ItemNode::new(
                        ItemKind::Module,
                        module_name,
                        VisibilityLevel::from_ast(item.visibility()),
                        item.syntax().text_range(),
                        current_file_id,
                        line_index,
                        children,
                    ))
                }
                ast::Item::Static(item) => Some(ItemNode::new(
                    ItemKind::Static,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Struct(item) => Some(ItemNode::new(
                    ItemKind::Struct,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Trait(item) => Some(ItemNode::new(
                    ItemKind::Trait,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::TypeAlias(item) => Some(ItemNode::new(
                    ItemKind::TypeAlias,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Union(item) => Some(ItemNode::new(
                    ItemKind::Union,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Use(item) => Some(ItemNode::new(
                    ItemKind::Use,
                    ItemNode::use_name(&item),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
            };

            if let Some(node) = node {
                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    fn collect_impl_items(
        &self,
        item: &ast::Impl,
        current_file_id: FileId,
        line_index: &LineIndex,
    ) -> Vec<ItemNode> {
        let Some(assoc_item_list) = item.assoc_item_list() else {
            return Vec::new();
        };

        let mut children = Vec::new();
        for assoc_item in assoc_item_list.assoc_items() {
            let node = match assoc_item {
                ast::AssocItem::Const(item) => Some(ItemNode::new(
                    ItemKind::AssociatedConst,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::AssocItem::Fn(item) => Some(ItemNode::new(
                    ItemKind::AssociatedFunction,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::AssocItem::TypeAlias(item) => Some(ItemNode::new(
                    ItemKind::AssociatedTypeAlias,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    item.syntax().text_range(),
                    current_file_id,
                    line_index,
                    Vec::new(),
                )),
                ast::AssocItem::MacroCall(_) => None,
            };

            if let Some(node) = node {
                children.push(node);
            }
        }

        children
    }

    fn collect_module_children(
        &mut self,
        item: &ast::Module,
        current_file_id: FileId,
        line_index: &LineIndex,
        state: &mut TargetBuildState,
    ) -> anyhow::Result<Vec<ItemNode>> {
        if let Some(item_list) = item.item_list() {
            let inline_items = item_list.items().collect::<Vec<_>>();
            return self
                .collect_items(inline_items, current_file_id, line_index, state)
                .context("while attempting to collect inline module items");
        }

        let Some(module_name) = item.name().map(|name| name.text().to_string()) else {
            return Ok(Vec::new());
        };
        let current_file_path = self.file_path(current_file_id).with_context(|| {
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

        let module_file_id = self.get_or_parse_file(&module_file_path).with_context(|| {
            format!(
                "while attempting to parse module file {}",
                module_file_path.display()
            )
        })?;
        self.collect_file_items(module_file_id, state)
            .with_context(|| {
                format!(
                    "while attempting to collect module items from {}",
                    module_file_path.display()
                )
            })
    }

    fn parsed_file(&self, file_id: FileId) -> anyhow::Result<&ParsedFile> {
        self.parsed_files
            .get(file_id.0)
            .with_context(|| format!("while attempting to look up parsed file {:?}", file_id))
    }

    fn file_path(&self, file_id: FileId) -> anyhow::Result<&Path> {
        self.parsed_files
            .get(file_id.0)
            .map(|parsed_file| parsed_file.record.path.as_path())
            .with_context(|| format!("while attempting to look up path for file {:?}", file_id))
    }

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
