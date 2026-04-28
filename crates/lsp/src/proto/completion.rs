use rg_analysis::{CompletionApplicability, CompletionItem, CompletionKind};
use tower_lsp_server::ls_types::{CompletionItem as LspCompletionItem, CompletionItemKind};

pub(crate) fn completion_item(item: CompletionItem) -> LspCompletionItem {
    LspCompletionItem {
        label: item.label,
        kind: Some(completion_kind(item.kind)),
        detail: completion_detail(item.applicability),
        ..Default::default()
    }
}

fn completion_kind(kind: CompletionKind) -> CompletionItemKind {
    match kind {
        CompletionKind::Field => CompletionItemKind::FIELD,
        CompletionKind::InherentMethod | CompletionKind::TraitMethod => CompletionItemKind::METHOD,
    }
}

fn completion_detail(applicability: CompletionApplicability) -> Option<String> {
    match applicability {
        CompletionApplicability::Known => None,
        CompletionApplicability::Maybe => Some("maybe applicable".to_string()),
    }
}
