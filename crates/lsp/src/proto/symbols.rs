use anyhow::Context as _;
use rg_analysis::{DocumentSymbol, SymbolKind, WorkspaceSymbol};
use rg_parse::ParseDb;
use tower_lsp_server::ls_types::{
    DocumentSymbol as LspDocumentSymbol, Location, OneOf, SymbolKind as LspSymbolKind, Uri,
    WorkspaceSymbol as LspWorkspaceSymbol,
};

use crate::proto::{navigation, position};

#[allow(deprecated)]
pub(crate) fn document_symbol(
    parse: &ParseDb,
    package_slot: usize,
    symbol: DocumentSymbol,
) -> anyhow::Result<LspDocumentSymbol> {
    let package = parse
        .package(package_slot)
        .context("while attempting to find package for document symbol conversion")?;
    let file = package
        .parsed_file(symbol.file_id)
        .context("while attempting to find file for document symbol conversion")?;
    let children = symbol
        .children
        .into_iter()
        .map(|child| document_symbol(parse, package_slot, child))
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(LspDocumentSymbol {
        name: symbol.name,
        detail: None,
        kind: symbol_kind(symbol.kind),
        tags: None,
        deprecated: None,
        range: position::range(file.line_index(), symbol.span),
        selection_range: position::range(file.line_index(), symbol.selection_span),
        children: (!children.is_empty()).then_some(children),
    })
}

pub(crate) fn workspace_symbol(
    parse: &ParseDb,
    symbol: WorkspaceSymbol,
) -> anyhow::Result<Option<LspWorkspaceSymbol>> {
    let Some(package) = parse.package(symbol.target.package.0) else {
        return Ok(None);
    };
    let Some(path) = package.file_path(symbol.file_id) else {
        return Ok(None);
    };
    let Some(uri) = Uri::from_file_path(path) else {
        return Ok(None);
    };
    let range =
        navigation::range_for_file(parse, symbol.target.package.0, symbol.file_id, symbol.span)?;

    Ok(Some(LspWorkspaceSymbol {
        name: symbol.name,
        kind: symbol_kind(symbol.kind),
        tags: None,
        container_name: symbol.container_name,
        location: OneOf::Left(Location { uri, range }),
        data: None,
    }))
}

pub(crate) fn symbol_kind(kind: SymbolKind) -> LspSymbolKind {
    match kind {
        SymbolKind::Const | SymbolKind::Static => LspSymbolKind::CONSTANT,
        SymbolKind::Enum => LspSymbolKind::ENUM,
        SymbolKind::EnumVariant => LspSymbolKind::ENUM_MEMBER,
        SymbolKind::Field => LspSymbolKind::FIELD,
        SymbolKind::Function => LspSymbolKind::FUNCTION,
        SymbolKind::Impl => LspSymbolKind::OBJECT,
        SymbolKind::Macro => LspSymbolKind::FUNCTION,
        SymbolKind::Method => LspSymbolKind::METHOD,
        SymbolKind::Module => LspSymbolKind::MODULE,
        SymbolKind::Struct | SymbolKind::Union => LspSymbolKind::STRUCT,
        SymbolKind::Trait => LspSymbolKind::INTERFACE,
        SymbolKind::TypeAlias => LspSymbolKind::CLASS,
        SymbolKind::Variable => LspSymbolKind::VARIABLE,
    }
}
