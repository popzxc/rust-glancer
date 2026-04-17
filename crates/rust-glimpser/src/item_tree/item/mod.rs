use ra_syntax::{
    TextRange,
    ast::{self, AstNode, HasName, HasVisibility},
};

use crate::item_tree::{
    file::FileId,
    span::{LineIndex, Span},
};

pub(crate) use self::types::{ItemKind, VisibilityLevel};

mod types;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemNode {
    pub kind: ItemKind,
    pub name: Option<String>,
    pub visibility: VisibilityLevel,
    pub file_id: FileId,
    pub span: Span,
    pub children: Vec<ItemNode>,
}

impl ItemNode {
    pub(crate) fn new_asm_expr(
        item: ast::AsmExpr,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> Self {
        Self::from_parts(
            ItemKind::AsmExpr,
            None,
            VisibilityLevel::Private,
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_const(item: ast::Const, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Const,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_enum(item: ast::Enum, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Enum,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_extern_block(
        item: ast::ExternBlock,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> Self {
        Self::from_parts(
            ItemKind::ExternBlock,
            None,
            VisibilityLevel::Private,
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_extern_crate(
        item: ast::ExternCrate,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> Self {
        Self::from_parts(
            ItemKind::ExternCrate,
            item.name_ref()
                .map(|name_ref| name_ref.syntax().text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_fn(item: ast::Fn, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Function,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_impl_block(item: ast::Impl, file_id: FileId, line_index: &LineIndex) -> Self {
        let children = Self::collect_impl_items(&item, file_id, line_index);
        Self::from_parts(
            ItemKind::Impl,
            None,
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            children,
        )
    }

    pub(crate) fn new_macro_def(
        item: ast::MacroDef,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> Self {
        Self::from_parts(
            ItemKind::MacroDefinition,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_macro_rules(
        item: ast::MacroRules,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> Self {
        Self::from_parts(
            ItemKind::MacroDefinition,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_module(
        item: ast::Module,
        children: Vec<ItemNode>,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> Self {
        Self::from_parts(
            ItemKind::Module,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            children,
        )
    }

    pub(crate) fn new_static(item: ast::Static, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Static,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_struct(item: ast::Struct, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Struct,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_trait(item: ast::Trait, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Trait,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_type_alias(
        item: ast::TypeAlias,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> Self {
        Self::from_parts(
            ItemKind::TypeAlias,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_union(item: ast::Union, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Union,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    pub(crate) fn new_use(item: ast::Use, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Use,
            Self::use_name(&item),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    fn from_parts(
        kind: ItemKind,
        name: Option<String>,
        visibility: VisibilityLevel,
        text_range: TextRange,
        file_id: FileId,
        line_index: &LineIndex,
        children: Vec<ItemNode>,
    ) -> Self {
        Self {
            kind,
            name,
            visibility,
            file_id,
            span: Span::from_text_range(text_range, line_index),
            children,
        }
    }

    fn collect_impl_items(
        item: &ast::Impl,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> Vec<ItemNode> {
        let Some(assoc_item_list) = item.assoc_item_list() else {
            return Vec::new();
        };

        let mut children = Vec::new();
        for assoc_item in assoc_item_list.assoc_items() {
            let node = match assoc_item {
                ast::AssocItem::Const(item) => {
                    Some(Self::new_assoc_const(item, file_id, line_index))
                }
                ast::AssocItem::Fn(item) => Some(Self::new_assoc_fn(item, file_id, line_index)),
                ast::AssocItem::TypeAlias(item) => {
                    Some(Self::new_assoc_type_alias(item, file_id, line_index))
                }
                ast::AssocItem::MacroCall(_) => None,
            };

            if let Some(node) = node {
                children.push(node);
            }
        }

        children
    }

    fn new_assoc_const(item: ast::Const, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::AssociatedConst,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    fn new_assoc_fn(item: ast::Fn, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::AssociatedFunction,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    fn new_assoc_type_alias(item: ast::TypeAlias, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::AssociatedTypeAlias,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
            Vec::new(),
        )
    }

    fn use_name(use_item: &ast::Use) -> Option<String> {
        let use_tree = use_item.use_tree()?;
        let text = use_tree.syntax().text().to_string();
        Some(Self::collapse_whitespace(&text))
    }

    fn collapse_whitespace(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}
