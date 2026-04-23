use ra_syntax::{
    TextRange,
    ast::{self, AstNode, HasName, HasVisibility},
};

use crate::parse::{
    FileId,
    span::{LineIndex, Span},
};

pub(crate) use self::{
    import::{
        ExternCrateItem, ImportAlias, UseImport, UseImportKind, UseItem, UsePath, UsePathSegment,
    },
    kind::{ItemKind, ItemTag},
    module::{ModuleItem, ModuleSource},
    visibility::VisibilityLevel,
};

mod import;
mod kind;
mod module;
mod visibility;

/// AST-based module items (impl block, struct, etc) representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemNode {
    pub kind: ItemKind,
    /// Name (when applicable), e.g. for functions or structs.
    pub name: Option<String>,
    pub visibility: VisibilityLevel,
    /// File where this item is declared.
    pub file_id: FileId,
    /// Source span of the declaration.
    pub span: Span,
}

impl ItemNode {
    /// Builds an item node for a top-level `asm!` item expression.
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
        )
    }

    /// Builds an item node for a `const` declaration.
    pub(crate) fn new_const(item: ast::Const, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Const,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
        )
    }

    /// Builds an item node for an `enum` declaration.
    pub(crate) fn new_enum(item: ast::Enum, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Enum,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
        )
    }

    /// Builds an item node for an `extern { ... }` block.
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
        )
    }

    /// Builds an item node for an `extern crate` declaration.
    pub(crate) fn new_extern_crate(
        item: ast::ExternCrate,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> Self {
        let extern_crate = ExternCrateItem::from_ast(&item);
        Self::from_parts(
            ItemKind::ExternCrate(Box::new(extern_crate)),
            item.name_ref()
                .map(|name_ref| name_ref.syntax().text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
        )
    }

    /// Builds an item node for a free function declaration.
    pub(crate) fn new_fn(item: ast::Fn, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Function,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
        )
    }

    /// Builds an item node for an `impl` block.
    ///
    /// Associated items are intentionally not lowered here yet; they are not module-scope
    /// definitions, and should become a separate item-tree concept once we need them.
    pub(crate) fn new_impl_block(item: ast::Impl, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Impl,
            None,
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
        )
    }

    /// Builds an item node for a `macro` definition.
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
        )
    }

    /// Builds an item node for a `macro_rules!` definition.
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
        )
    }

    /// Builds an item node for a module declaration.
    pub(crate) fn new_module(
        item: ast::Module,
        module_item: ModuleItem,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> Self {
        Self::from_parts(
            ItemKind::Module(Box::new(module_item)),
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
        )
    }

    /// Builds an item node for a `static` declaration.
    pub(crate) fn new_static(item: ast::Static, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Static,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
        )
    }

    /// Builds an item node for a `struct` declaration.
    pub(crate) fn new_struct(item: ast::Struct, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Struct,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
        )
    }

    /// Builds an item node for a `trait` declaration.
    pub(crate) fn new_trait(item: ast::Trait, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Trait,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
        )
    }

    /// Builds an item node for a type alias declaration.
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
        )
    }

    /// Builds an item node for a `union` declaration.
    pub(crate) fn new_union(item: ast::Union, file_id: FileId, line_index: &LineIndex) -> Self {
        Self::from_parts(
            ItemKind::Union,
            item.name().map(|name| name.text().to_string()),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
        )
    }

    /// Builds an item node for a `use` declaration and stores a normalized use-tree name.
    pub(crate) fn new_use(item: ast::Use, file_id: FileId, line_index: &LineIndex) -> Self {
        fn use_name(use_item: &ast::Use) -> Option<String> {
            let use_tree = use_item.use_tree()?;
            let text = use_tree.syntax().text().to_string();

            // Normalize all whitespace in an extracted syntax fragment to single spaces.
            let name = text.split_whitespace().collect::<Vec<_>>().join(" ");
            Some(name)
        }

        let use_item = UseItem::from_ast(&item);
        Self::from_parts(
            ItemKind::Use(Box::new(use_item)),
            use_name(&item),
            VisibilityLevel::from_ast(item.visibility()),
            item.syntax().text_range(),
            file_id,
            line_index,
        )
    }

    /// Creates a fully-populated item node from already-extracted parts.
    fn from_parts(
        kind: ItemKind,
        name: Option<String>,
        visibility: VisibilityLevel,
        text_range: TextRange,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> Self {
        Self {
            kind,
            name,
            visibility,
            file_id,
            span: Span::from_text_range(text_range, line_index),
        }
    }
}
