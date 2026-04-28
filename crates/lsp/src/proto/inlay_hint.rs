use anyhow::Context as _;
use rg_analysis::TypeHint;
use rg_parse::ParseDb;
use tower_lsp_server::ls_types::{InlayHint, InlayHintKind, InlayHintLabel};

use crate::proto::position;

pub(crate) fn type_hint(
    parse: &ParseDb,
    package_slot: usize,
    hint: TypeHint,
) -> anyhow::Result<Option<InlayHint>> {
    let Some(package) = parse.package(package_slot) else {
        return Ok(None);
    };
    let file = package
        .parsed_file(hint.file_id)
        .context("while attempting to find file for inlay hint conversion")?;

    Ok(Some(InlayHint {
        position: position::position(file.line_index(), hint.span.text.end),
        label: InlayHintLabel::String(hint.label),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: None,
        padding_right: None,
        data: None,
    }))
}
