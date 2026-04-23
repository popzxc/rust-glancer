use std::{collections::HashSet, path::Path};

use anyhow::Context as _;
use ra_syntax::ast::{self, HasModuleItem, HasName};

mod item;

#[cfg(test)]
mod tests;

use crate::parse::{FileDb, FileId, ParseDb, Target as ParseTarget, TargetId, span::LineIndex};

pub(crate) use self::item::{ItemKind, ItemNode, VisibilityLevel};

/// Lowered item trees for all parsed packages and targets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemTreeDb {
    packages: Vec<Package>,
}

impl ItemTreeDb {
    /// Builds target-local item trees on top of the parsed source database.
    pub(crate) fn build(parse: &mut ParseDb) -> anyhow::Result<Self> {
        let package_count = parse.packages().len();
        let mut packages = Vec::with_capacity(package_count);

        for package in parse.packages_mut() {
            let mut targets = Vec::with_capacity(package.targets.len());

            for target in &package.targets {
                let root_items = TargetTreeBuilder::new(&mut package.files)
                    .build(target)
                    .with_context(|| {
                        format!(
                            "while attempting to build item tree for target {}",
                            target.cargo_target.name
                        )
                    })?;
                targets.push(Target {
                    target: target.id,
                    root_items,
                });
            }

            packages.push(Package { targets });
        }

        Ok(Self { packages })
    }

    /// Returns one package tree set by slot.
    pub(crate) fn package(&self, package_slot: usize) -> Option<&Package> {
        self.packages.get(package_slot)
    }
}

/// Item trees for all targets inside one parsed package.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Package {
    targets: Vec<Target>,
}

impl Package {
    /// Returns all target trees.
    pub(crate) fn targets(&self) -> &[Target] {
        &self.targets
    }
}

/// Target-local lowered item tree rooted at one target entrypoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub target: TargetId,
    pub root_items: Vec<ItemNode>,
}

struct TargetTreeBuilder<'db> {
    files: &'db mut FileDb,
    active_stack: HashSet<FileId>,
}

impl<'db> TargetTreeBuilder<'db> {
    fn new(files: &'db mut FileDb) -> Self {
        Self {
            files,
            active_stack: HashSet::default(),
        }
    }

    fn build(mut self, target: &ParseTarget) -> anyhow::Result<Vec<ItemNode>> {
        self.collect_file_items(target.root_file).with_context(|| {
            format!(
                "while attempting to collect root items for target {}",
                target.cargo_target.name
            )
        })
    }

    fn collect_file_items(&mut self, current_file_id: FileId) -> anyhow::Result<Vec<ItemNode>> {
        if !self.active_stack.insert(current_file_id) {
            return Ok(Vec::new());
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

        self.active_stack.remove(&current_file_id);
        Ok(nodes)
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
        let current_file_path = self.files.file_path(current_file_id).with_context(|| {
            format!(
                "while attempting to resolve current file {:?}",
                current_file_id
            )
        })?;

        // TODO: support `#[path = \"...\"]` and other advanced module-resolution rules when needed.
        let Some(module_file_path) = resolve_module_file(current_file_path, &module_name) else {
            return Ok(Vec::new());
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

        self.collect_file_items(module_file_id).with_context(|| {
            format!(
                "while attempting to collect module items from {}",
                module_file_path.display()
            )
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
