//! AST-to-item-tree lowering for one parsed package.
//!
//! This phase is deliberately file-oriented: each source file is lowered once into a `FileTree`,
//! and targets only point at their root file. Out-of-line modules therefore reuse the same lowered
//! file tree whenever multiple targets reach them.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use ra_syntax::{
    AstNode as _,
    ast::{
        self, HasGenericArgs, HasGenericParams, HasModuleItem, HasName, HasTypeBounds,
        HasVisibility,
    },
};

use crate::parse::{FileDb, FileId, Target as ParseTarget, span::LineIndex};

use super::{
    ConstItem, ConstParamData, EnumItem, EnumVariantItem, ExternCrateItem, FieldItem, FieldList,
    FileTree, FunctionItem, FunctionQualifiers, GenericArg, GenericParams, ImplItem, ItemKind,
    ItemNode, ItemTreeId, LifetimeParamData, ModuleItem, ModuleSource, Mutability, Package,
    ParamItem, ParamKind, StaticItem, StructItem, TargetRoot, TraitItem, TypeAliasItem, TypeBound,
    TypeParamData, TypePath, TypePathSegment, TypeRef, UnionItem, UseItem, VisibilityLevel,
    WherePredicate,
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
                        target.name
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

        let (items, line_index, module_file_context) = {
            let parsed_file = self.files.parsed_file(current_file_id).with_context(|| {
                format!(
                    "while attempting to fetch parsed file {:?}",
                    current_file_id
                )
            })?;
            (
                parsed_file.tree.items().collect::<Vec<_>>(),
                parsed_file.line_index.clone(),
                ModuleFileContext::from_definition_file(&parsed_file.path),
            )
        };

        let mut builder = FileTreeBuilder::new(current_file_id, &line_index);
        let top_level = self
            .collect_items(&mut builder, items, &module_file_context)
            .with_context(|| {
                format!(
                    "while attempting to collect file items for {:?}",
                    current_file_id
                )
            })?;

        self.file_trees[current_file_id.0] = Some(FileTree {
            file: current_file_id,
            top_level,
            items: builder.items,
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
        builder: &mut FileTreeBuilder<'_>,
        items: Vec<ast::Item>,
        module_file_context: &ModuleFileContext,
    ) -> anyhow::Result<Vec<ItemTreeId>> {
        let mut item_ids = Vec::new();

        for item in items {
            let item_id = self
                .lower_item(builder, item, module_file_context)
                .with_context(|| {
                    format!(
                        "while attempting to lower item in {:?}",
                        builder.current_file_id
                    )
                })?;

            if let Some(item_id) = item_id {
                item_ids.push(item_id);
            }
        }

        Ok(item_ids)
    }

    /// Lowers one syntax item into the corresponding item-tree node, when this item kind matters
    /// to later phases.
    fn lower_item(
        &mut self,
        builder: &mut FileTreeBuilder<'_>,
        item: ast::Item,
        module_file_context: &ModuleFileContext,
    ) -> anyhow::Result<Option<ItemTreeId>> {
        let item_id = match item {
            ast::Item::AsmExpr(item) => Some(builder.alloc_item(
                ItemKind::AsmExpr,
                None,
                VisibilityLevel::Private,
                item.syntax().text_range(),
            )),
            ast::Item::Const(item) => Some(builder.alloc_item(
                ItemKind::Const(Box::new(ConstItem {
                    generics: lower_generic_params(&item),
                    ty: item.ty().map(lower_type_ref),
                })),
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
            )),
            ast::Item::Enum(item) => Some(builder.alloc_item(
                ItemKind::Enum(Box::new(EnumItem {
                    generics: lower_generic_params(&item),
                    variants: lower_enum_variants(&item),
                })),
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
            )),
            ast::Item::ExternBlock(item) => Some(builder.alloc_item(
                ItemKind::ExternBlock,
                None,
                VisibilityLevel::Private,
                item.syntax().text_range(),
            )),
            ast::Item::ExternCrate(item) => Some(
                builder.alloc_item(
                    ItemKind::ExternCrate(Box::new(ExternCrateItem::from_ast(&item))),
                    item.name_ref()
                        .map(|name_ref| name_ref.syntax().text().to_string()),
                    lower_visibility(item.visibility()),
                    item.syntax().text_range(),
                ),
            ),
            ast::Item::Fn(item) => Some(builder.alloc_item(
                ItemKind::Function(Box::new(lower_function_item(&item))),
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
            )),
            ast::Item::Impl(item) => {
                let impl_item = self
                    .lower_impl_item(builder, &item)
                    .context("while attempting to lower impl declaration")?;
                Some(builder.alloc_item(
                    ItemKind::Impl(Box::new(impl_item)),
                    None,
                    lower_visibility(item.visibility()),
                    item.syntax().text_range(),
                ))
            }
            ast::Item::MacroCall(_) => None,
            ast::Item::MacroDef(item) => Some(builder.alloc_item(
                ItemKind::MacroDefinition,
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
            )),
            ast::Item::MacroRules(item) => Some(builder.alloc_item(
                ItemKind::MacroDefinition,
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
            )),
            ast::Item::Module(item) => {
                let module_name = item.name().map(|name| name.text().to_string());
                let module_item = self
                    .collect_module(builder, &item, module_file_context)
                    .with_context(|| {
                        format!(
                            "while attempting to collect module item for {}",
                            module_name.as_deref().unwrap_or("<unnamed>")
                        )
                    })?;
                Some(builder.alloc_item(
                    ItemKind::Module(Box::new(module_item)),
                    module_name,
                    lower_visibility(item.visibility()),
                    item.syntax().text_range(),
                ))
            }
            ast::Item::Static(item) => Some(builder.alloc_item(
                ItemKind::Static(Box::new(StaticItem {
                    ty: item.ty().map(lower_type_ref),
                    mutability: if item.mut_token().is_some() {
                        Mutability::Mutable
                    } else {
                        Mutability::Shared
                    },
                })),
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
            )),
            ast::Item::Struct(item) => Some(builder.alloc_item(
                ItemKind::Struct(Box::new(StructItem {
                    generics: lower_generic_params(&item),
                    fields: lower_field_list(item.field_list()),
                })),
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
            )),
            ast::Item::Trait(item) => {
                let trait_item = self
                    .lower_trait_item(builder, &item)
                    .context("while attempting to lower trait declaration")?;
                Some(builder.alloc_item(
                    ItemKind::Trait(Box::new(trait_item)),
                    item.name().map(|name| name.text().to_string()),
                    lower_visibility(item.visibility()),
                    item.syntax().text_range(),
                ))
            }
            ast::Item::TypeAlias(item) => Some(builder.alloc_item(
                ItemKind::TypeAlias(Box::new(TypeAliasItem {
                    generics: lower_generic_params(&item),
                    bounds: lower_type_bound_list(item.type_bound_list()),
                    aliased_ty: item.ty().map(lower_type_ref),
                })),
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
            )),
            ast::Item::Union(item) => Some(
                builder.alloc_item(
                    ItemKind::Union(Box::new(UnionItem {
                        generics: lower_generic_params(&item),
                        fields: item
                            .record_field_list()
                            .map(lower_record_fields)
                            .unwrap_or_default(),
                    })),
                    item.name().map(|name| name.text().to_string()),
                    lower_visibility(item.visibility()),
                    item.syntax().text_range(),
                ),
            ),
            ast::Item::Use(item) => Some(builder.alloc_item(
                ItemKind::Use(Box::new(UseItem::from_ast(&item))),
                normalized_use_name(&item),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
            )),
        };

        Ok(item_id)
    }

    /// Lowers one module declaration into either an inline item list or an out-of-line file link.
    fn collect_module(
        &mut self,
        builder: &mut FileTreeBuilder<'_>,
        item: &ast::Module,
        module_file_context: &ModuleFileContext,
    ) -> anyhow::Result<ModuleItem> {
        if let Some(item_list) = item.item_list() {
            // Inline modules reuse the current file, but their out-of-line descendants are
            // resolved under a directory named after the inline module path.
            let inline_module_context = item
                .name()
                .map(|name| module_file_context.descend(name.text().as_str()))
                .unwrap_or_else(|| module_file_context.clone());
            let inline_items = item_list.items().collect::<Vec<_>>();
            let items = self
                .collect_items(builder, inline_items, &inline_module_context)
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
        // TODO: support `#[path = "..."]` and other advanced module-resolution rules when needed.
        let Some(module_file_path) = module_file_context.resolve_child_file(&module_name) else {
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

    fn lower_trait_item(
        &mut self,
        builder: &mut FileTreeBuilder<'_>,
        item: &ast::Trait,
    ) -> anyhow::Result<TraitItem> {
        let assoc_items = item
            .assoc_item_list()
            .map(|item_list| item_list.assoc_items().collect::<Vec<_>>())
            .unwrap_or_default();
        let items = self
            .collect_assoc_items(builder, assoc_items)
            .context("while attempting to lower trait associated items")?;

        Ok(TraitItem {
            generics: lower_generic_params(item),
            super_traits: lower_type_bound_list(item.type_bound_list()),
            items,
            is_unsafe: item.unsafe_token().is_some(),
        })
    }

    fn lower_impl_item(
        &mut self,
        builder: &mut FileTreeBuilder<'_>,
        item: &ast::Impl,
    ) -> anyhow::Result<ImplItem> {
        let assoc_items = item
            .assoc_item_list()
            .map(|item_list| item_list.assoc_items().collect::<Vec<_>>())
            .unwrap_or_default();
        let items = self
            .collect_assoc_items(builder, assoc_items)
            .context("while attempting to lower impl associated items")?;
        let (trait_ref, self_ty) = lower_impl_header(item);

        Ok(ImplItem {
            generics: lower_generic_params(item),
            trait_ref,
            self_ty,
            items,
            is_unsafe: item.unsafe_token().is_some(),
        })
    }

    fn collect_assoc_items(
        &mut self,
        builder: &mut FileTreeBuilder<'_>,
        items: Vec<ast::AssocItem>,
    ) -> anyhow::Result<Vec<ItemTreeId>> {
        let mut item_ids = Vec::new();

        for item in items {
            if let Some(item_id) = self.lower_assoc_item(builder, item)? {
                item_ids.push(item_id);
            }
        }

        Ok(item_ids)
    }

    fn lower_assoc_item(
        &mut self,
        builder: &mut FileTreeBuilder<'_>,
        item: ast::AssocItem,
    ) -> anyhow::Result<Option<ItemTreeId>> {
        let item_id = match item {
            ast::AssocItem::Const(item) => Some(builder.alloc_item(
                ItemKind::Const(Box::new(ConstItem {
                    generics: lower_generic_params(&item),
                    ty: item.ty().map(lower_type_ref),
                })),
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
            )),
            ast::AssocItem::Fn(item) => Some(builder.alloc_item(
                ItemKind::Function(Box::new(lower_function_item(&item))),
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
            )),
            ast::AssocItem::MacroCall(_) => None,
            ast::AssocItem::TypeAlias(item) => Some(builder.alloc_item(
                ItemKind::TypeAlias(Box::new(TypeAliasItem {
                    generics: lower_generic_params(&item),
                    bounds: lower_type_bound_list(item.type_bound_list()),
                    aliased_ty: item.ty().map(lower_type_ref),
                })),
                item.name().map(|name| name.text().to_string()),
                lower_visibility(item.visibility()),
                item.syntax().text_range(),
            )),
        };

        Ok(item_id)
    }
}

/// File-local item arena under construction.
struct FileTreeBuilder<'a> {
    current_file_id: FileId,
    line_index: &'a LineIndex,
    items: Vec<ItemNode>,
}

impl<'a> FileTreeBuilder<'a> {
    fn new(current_file_id: FileId, line_index: &'a LineIndex) -> Self {
        Self {
            current_file_id,
            line_index,
            items: Vec::new(),
        }
    }

    fn alloc_item(
        &mut self,
        kind: ItemKind,
        name: Option<String>,
        visibility: VisibilityLevel,
        text_range: ra_syntax::TextRange,
    ) -> ItemTreeId {
        let item_id = ItemTreeId(self.items.len());
        self.items.push(ItemNode::new(
            kind,
            name,
            visibility,
            text_range,
            self.current_file_id,
            self.line_index,
        ));
        item_id
    }
}

fn lower_function_item(item: &ast::Fn) -> FunctionItem {
    FunctionItem {
        generics: lower_generic_params(item),
        params: lower_params(item.param_list()),
        ret_ty: item
            .ret_type()
            .and_then(|ret_ty| ret_ty.ty())
            .map(lower_type_ref),
        qualifiers: FunctionQualifiers {
            is_async: item.async_token().is_some(),
            is_const: item.const_token().is_some(),
            is_unsafe: item.unsafe_token().is_some(),
        },
    }
}

fn lower_params(param_list: Option<ast::ParamList>) -> Vec<ParamItem> {
    let Some(param_list) = param_list else {
        return Vec::new();
    };

    let mut params = Vec::new();

    if let Some(self_param) = param_list.self_param() {
        params.push(ParamItem {
            pat: normalized_syntax(&self_param),
            ty: self_param.ty().map(lower_type_ref),
            kind: ParamKind::SelfParam,
        });
    }

    for param in param_list.params() {
        params.push(ParamItem {
            pat: param
                .pat()
                .map(|pat| normalized_syntax(&pat))
                .unwrap_or_else(|| "<missing>".to_string()),
            ty: param.ty().map(lower_type_ref),
            kind: ParamKind::Normal,
        });
    }

    params
}

fn lower_impl_header(item: &ast::Impl) -> (Option<TypeRef>, TypeRef) {
    let types = item
        .syntax()
        .children()
        .filter_map(ast::Type::cast)
        .collect::<Vec<_>>();

    if item.for_token().is_some() {
        let trait_ref = types.first().cloned().map(lower_type_ref);
        let self_ty = types
            .get(1)
            .cloned()
            .map(lower_type_ref)
            .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(item)));
        return (trait_ref, self_ty);
    }

    let self_ty = types
        .first()
        .cloned()
        .map(lower_type_ref)
        .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(item)));
    (None, self_ty)
}

fn lower_enum_variants(item: &ast::Enum) -> Vec<EnumVariantItem> {
    item.variant_list()
        .map(|variant_list| {
            variant_list
                .variants()
                .map(|variant| EnumVariantItem {
                    name: variant
                        .name()
                        .map(|name| name.text().to_string())
                        .unwrap_or_else(|| "<missing>".to_string()),
                    fields: lower_field_list(variant.field_list()),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn lower_field_list(field_list: Option<ast::FieldList>) -> FieldList {
    match field_list {
        Some(ast::FieldList::RecordFieldList(fields)) => {
            FieldList::Named(lower_record_fields(fields))
        }
        Some(ast::FieldList::TupleFieldList(fields)) => {
            FieldList::Tuple(lower_tuple_fields(fields))
        }
        None => FieldList::Unit,
    }
}

fn lower_record_fields(fields: ast::RecordFieldList) -> Vec<FieldItem> {
    fields
        .fields()
        .map(|field| FieldItem {
            name: field.name().map(|name| name.text().to_string()),
            visibility: lower_visibility(field.visibility()),
            ty: field
                .ty()
                .map(lower_type_ref)
                .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(&field))),
        })
        .collect()
}

fn lower_tuple_fields(fields: ast::TupleFieldList) -> Vec<FieldItem> {
    fields
        .fields()
        .map(|field| FieldItem {
            name: None,
            visibility: lower_visibility(field.visibility()),
            ty: field
                .ty()
                .map(lower_type_ref)
                .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(&field))),
        })
        .collect()
}

fn lower_generic_params<T>(item: &T) -> GenericParams
where
    T: HasGenericParams,
{
    let mut params = GenericParams::default();

    if let Some(param_list) = item.generic_param_list() {
        for param in param_list.generic_params() {
            match param {
                ast::GenericParam::ConstParam(param) => {
                    params.consts.push(ConstParamData {
                        name: param
                            .name()
                            .map(|name| name.text().to_string())
                            .unwrap_or_else(|| "<missing>".to_string()),
                        ty: param.ty().map(lower_type_ref),
                        default: param.default_val().map(|value| normalized_syntax(&value)),
                    });
                }
                ast::GenericParam::LifetimeParam(param) => {
                    params.lifetimes.push(LifetimeParamData {
                        name: param
                            .lifetime()
                            .map(|lifetime| normalized_syntax(&lifetime))
                            .unwrap_or_else(|| "<missing>".to_string()),
                        bounds: lower_lifetime_bounds(param.type_bound_list()),
                    });
                }
                ast::GenericParam::TypeParam(param) => {
                    params.types.push(TypeParamData {
                        name: param
                            .name()
                            .map(|name| name.text().to_string())
                            .unwrap_or_else(|| "<missing>".to_string()),
                        bounds: lower_type_bound_list(param.type_bound_list()),
                        default: param.default_type().map(lower_type_ref),
                    });
                }
            }
        }
    }

    if let Some(where_clause) = item.where_clause() {
        params.where_predicates = where_clause
            .predicates()
            .map(|predicate| {
                if let Some(lifetime) = predicate.lifetime() {
                    return WherePredicate::Lifetime {
                        lifetime: normalized_syntax(&lifetime),
                        bounds: lower_lifetime_bounds(predicate.type_bound_list()),
                    };
                }

                if let Some(ty) = predicate.ty() {
                    return WherePredicate::Type {
                        ty: lower_type_ref(ty),
                        bounds: lower_type_bound_list(predicate.type_bound_list()),
                    };
                }

                WherePredicate::Unsupported(normalized_syntax(&predicate))
            })
            .collect();
    }

    params
}

fn lower_lifetime_bounds(bound_list: Option<ast::TypeBoundList>) -> Vec<String> {
    bound_list
        .into_iter()
        .flat_map(|bound_list| bound_list.bounds())
        .filter_map(|bound| {
            bound
                .lifetime()
                .map(|lifetime| normalized_syntax(&lifetime))
        })
        .collect()
}

fn lower_type_bound_list(bound_list: Option<ast::TypeBoundList>) -> Vec<TypeBound> {
    bound_list
        .into_iter()
        .flat_map(|bound_list| bound_list.bounds())
        .map(lower_type_bound)
        .collect()
}

fn lower_type_bound(bound: ast::TypeBound) -> TypeBound {
    if let Some(lifetime) = bound.lifetime() {
        return TypeBound::Lifetime(normalized_syntax(&lifetime));
    }

    if let Some(ty) = bound.ty() {
        return TypeBound::Trait(lower_type_ref(ty));
    }

    TypeBound::Unsupported(normalized_syntax(&bound))
}

fn lower_type_ref(ty: ast::Type) -> TypeRef {
    match ty {
        ast::Type::ArrayType(ty) => TypeRef::Array {
            inner: Box::new(
                ty.ty()
                    .map(lower_type_ref)
                    .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(&ty))),
            ),
            len: ty.const_arg().map(|arg| normalized_syntax(&arg)),
        },
        ast::Type::DynTraitType(ty) => {
            TypeRef::DynTrait(lower_type_bound_list(ty.type_bound_list()))
        }
        ast::Type::FnPtrType(ty) => TypeRef::FnPointer {
            params: lower_params(ty.param_list())
                .into_iter()
                .map(|param| param.ty.unwrap_or_else(|| TypeRef::Unknown(String::new())))
                .collect(),
            ret: Box::new(
                ty.ret_type()
                    .and_then(|ret_ty| ret_ty.ty())
                    .map(lower_type_ref)
                    .unwrap_or(TypeRef::Unit),
            ),
        },
        ast::Type::ForType(ty) => ty
            .ty()
            .map(lower_type_ref)
            .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(&ty))),
        ast::Type::ImplTraitType(ty) => {
            TypeRef::ImplTrait(lower_type_bound_list(ty.type_bound_list()))
        }
        ast::Type::InferType(_) => TypeRef::Infer,
        ast::Type::MacroType(ty) => TypeRef::unknown_from_text(normalized_syntax(&ty)),
        ast::Type::NeverType(_) => TypeRef::Never,
        ast::Type::ParenType(ty) => ty
            .ty()
            .map(lower_type_ref)
            .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(&ty))),
        ast::Type::PathType(ty) => ty
            .path()
            .map(lower_type_path)
            .map(TypeRef::Path)
            .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(&ty))),
        ast::Type::PtrType(ty) => TypeRef::RawPointer {
            mutability: if ty.mut_token().is_some() {
                Mutability::Mutable
            } else {
                Mutability::Shared
            },
            inner: Box::new(
                ty.ty()
                    .map(lower_type_ref)
                    .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(&ty))),
            ),
        },
        ast::Type::RefType(ty) => TypeRef::Reference {
            lifetime: ty.lifetime().map(|lifetime| normalized_syntax(&lifetime)),
            mutability: if ty.mut_token().is_some() {
                Mutability::Mutable
            } else {
                Mutability::Shared
            },
            inner: Box::new(
                ty.ty()
                    .map(lower_type_ref)
                    .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(&ty))),
            ),
        },
        ast::Type::SliceType(ty) => TypeRef::Slice(Box::new(
            ty.ty()
                .map(lower_type_ref)
                .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(&ty))),
        )),
        ast::Type::TupleType(ty) => {
            let fields = ty.fields().map(lower_type_ref).collect::<Vec<_>>();
            if fields.is_empty() {
                TypeRef::Unit
            } else {
                TypeRef::Tuple(fields)
            }
        }
    }
}

fn lower_type_path(path: ast::Path) -> TypePath {
    let absolute = path
        .first_segment()
        .is_some_and(|segment| segment.coloncolon_token().is_some());
    let mut segments = Vec::new();
    collect_path_segments(&path, &mut segments);

    TypePath { absolute, segments }
}

fn collect_path_segments(path: &ast::Path, segments: &mut Vec<TypePathSegment>) {
    if let Some(qualifier) = path.qualifier() {
        collect_path_segments(&qualifier, segments);
    }

    if let Some(segment) = path.segment() {
        segments.push(lower_type_path_segment(&segment));
    }
}

fn lower_type_path_segment(segment: &ast::PathSegment) -> TypePathSegment {
    let name = segment
        .name_ref()
        .map(|name| name.syntax().text().to_string())
        .unwrap_or_else(|| normalized_syntax(segment));
    let mut args = Vec::new();

    if let Some(arg_list) = segment.generic_arg_list() {
        args.extend(arg_list.generic_args().map(lower_generic_arg));
    }

    if let Some(parenthesized_args) = segment.parenthesized_arg_list() {
        args.push(GenericArg::Unsupported(normalized_syntax(
            &parenthesized_args,
        )));
    }

    TypePathSegment { name, args }
}

fn lower_generic_arg(arg: ast::GenericArg) -> GenericArg {
    match arg {
        ast::GenericArg::AssocTypeArg(arg) => GenericArg::AssocType {
            name: arg
                .name_ref()
                .map(|name| name.syntax().text().to_string())
                .unwrap_or_else(|| "<missing>".to_string()),
            ty: arg.ty().map(lower_type_ref),
        },
        ast::GenericArg::ConstArg(arg) => GenericArg::Const(normalized_syntax(&arg)),
        ast::GenericArg::LifetimeArg(arg) => arg
            .lifetime()
            .map(|lifetime| GenericArg::Lifetime(normalized_syntax(&lifetime)))
            .unwrap_or_else(|| GenericArg::Unsupported(normalized_syntax(&arg))),
        ast::GenericArg::TypeArg(arg) => arg
            .ty()
            .map(lower_type_ref)
            .map(GenericArg::Type)
            .unwrap_or_else(|| GenericArg::Unsupported(normalized_syntax(&arg))),
    }
}

fn normalized_syntax(node: &impl ra_syntax::AstNode) -> String {
    node.syntax()
        .text()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Filesystem context for resolving out-of-line child modules of the current logical module.
#[derive(Debug, Clone)]
struct ModuleFileContext {
    child_module_dir: PathBuf,
}

impl ModuleFileContext {
    /// Builds the child-module directory for a file-backed module.
    fn from_definition_file(definition_file: &Path) -> Self {
        let parent_dir = definition_file
            .parent()
            .expect("definition file should have a parent directory");
        let file_name = definition_file
            .file_name()
            .and_then(|name| name.to_str())
            .expect("definition file name should be UTF-8");
        let file_stem = definition_file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("definition file stem should be UTF-8");

        let child_module_dir = match file_name {
            "lib.rs" | "main.rs" | "mod.rs" => parent_dir.to_path_buf(),
            _ => parent_dir.join(file_stem),
        };

        Self { child_module_dir }
    }

    /// Builds the child-module directory for an inline child module.
    fn descend(&self, module_name: &str) -> Self {
        Self {
            child_module_dir: self.child_module_dir.join(module_name),
        }
    }

    /// Resolves `mod name;` according to conventional Rust module file rules.
    fn resolve_child_file(&self, module_name: &str) -> Option<PathBuf> {
        let flat_file = self.child_module_dir.join(format!("{module_name}.rs"));
        if flat_file.exists() {
            return Some(flat_file);
        }

        let nested_file = self.child_module_dir.join(module_name).join("mod.rs");
        if nested_file.exists() {
            return Some(nested_file);
        }

        None
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
