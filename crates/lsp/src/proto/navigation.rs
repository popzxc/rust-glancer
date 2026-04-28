use anyhow::Context as _;
use rg_analysis::NavigationTarget;
use rg_parse::{FileId, ParseDb, Span};
use tower_lsp_server::ls_types::{Location, Range, Uri};

use crate::proto::position;

pub(crate) fn location_for_target(
    parse: &ParseDb,
    target: &NavigationTarget,
) -> anyhow::Result<Option<Location>> {
    let Some(package) = parse.package(target.target.package.0) else {
        return Ok(None);
    };
    let Some(path) = package.file_path(target.file_id) else {
        return Ok(None);
    };
    let Some(uri) = Uri::from_file_path(path) else {
        return Ok(None);
    };

    let range = range_for_file(parse, target.target.package.0, target.file_id, target.span)?;

    Ok(Some(Location { uri, range }))
}

pub(crate) fn range_for_file(
    parse: &ParseDb,
    package_slot: usize,
    file_id: FileId,
    span: Option<Span>,
) -> anyhow::Result<Range> {
    let Some(span) = span else {
        return Ok(position::zero_range());
    };
    let package = parse
        .package(package_slot)
        .context("while attempting to find package for LSP range conversion")?;
    let file = package
        .parsed_file(file_id)
        .context("while attempting to find file for LSP range conversion")?;

    Ok(position::range(file.line_index(), span))
}
