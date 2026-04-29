use std::mem;

use crate::{MemoryRecorder, MemorySize};

macro_rules! impl_leaf_memory_size {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl MemorySize for $ty {
                fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
            }
        )+
    };
}

macro_rules! record_field {
    ($recorder:expr, $owner:expr, $field:ident) => {
        $recorder.scope(stringify!($field), |recorder| {
            $owner.$field.record_memory_children(recorder)
        });
    };
}

impl_leaf_memory_size!(
    ls_types::CompletionItemKind,
    ls_types::CompletionItemTag,
    ls_types::DiagnosticSeverity,
    ls_types::DiagnosticTag,
    ls_types::InlayHintKind,
    ls_types::InsertTextFormat,
    ls_types::InsertTextMode,
    ls_types::MarkupKind,
    ls_types::MessageType,
    ls_types::Position,
    ls_types::SymbolKind,
    ls_types::SymbolTag,
);

impl<A, B> MemorySize for ls_types::OneOf<A, B>
where
    A: MemorySize,
    B: MemorySize,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            ls_types::OneOf::Left(value) => {
                recorder.scope("left", |recorder| value.record_memory_children(recorder));
            }
            ls_types::OneOf::Right(value) => {
                recorder.scope("right", |recorder| value.record_memory_children(recorder));
            }
        }
    }
}

impl MemorySize for ls_types::NumberOrString {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            ls_types::NumberOrString::Number(_) => {}
            ls_types::NumberOrString::String(value) => {
                recorder.scope("string", |recorder| value.record_memory_children(recorder));
            }
        }
    }
}

impl MemorySize for ls_types::LSPAny {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            ls_types::LSPAny::Null | ls_types::LSPAny::Bool(_) | ls_types::LSPAny::Number(_) => {}
            ls_types::LSPAny::String(value) => {
                recorder.scope("string", |recorder| value.record_memory_children(recorder));
            }
            ls_types::LSPAny::Array(items) => {
                recorder.scope("array", |recorder| items.record_memory_children(recorder));
            }
            ls_types::LSPAny::Object(object) => {
                recorder.scope("object", |recorder| object.record_memory_children(recorder));
            }
        }
    }
}

impl MemorySize for ls_types::LSPObject {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        // serde_json hides whether the map is backed by BTreeMap or IndexMap. Count initialized
        // entries and mark their storage as approximate rather than pretending to know node layout.
        recorder.record_approximate::<ls_types::LSPObject>(self.len().saturating_mul(
            mem::size_of::<String>().saturating_add(mem::size_of::<ls_types::LSPAny>()),
        ));

        recorder.scope("entries", |recorder| {
            for (key, value) in self {
                recorder.scope("key", |recorder| key.record_memory_children(recorder));
                recorder.scope("value", |recorder| value.record_memory_children(recorder));
            }
        });
    }
}

impl MemorySize for ls_types::Uri {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.record_approximate::<ls_types::Uri>(self.as_str().len());
    }
}

impl MemorySize for ls_types::Range {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, start);
        record_field!(recorder, self, end);
    }
}

impl MemorySize for ls_types::Location {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, uri);
        record_field!(recorder, self, range);
    }
}

impl MemorySize for ls_types::LocationLink {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, origin_selection_range);
        record_field!(recorder, self, target_uri);
        record_field!(recorder, self, target_range);
        record_field!(recorder, self, target_selection_range);
    }
}

impl MemorySize for ls_types::Diagnostic {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, range);
        record_field!(recorder, self, severity);
        record_field!(recorder, self, code);
        record_field!(recorder, self, code_description);
        record_field!(recorder, self, source);
        record_field!(recorder, self, message);
        record_field!(recorder, self, related_information);
        record_field!(recorder, self, tags);
        record_field!(recorder, self, data);
    }
}

impl MemorySize for ls_types::CodeDescription {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, href);
    }
}

impl MemorySize for ls_types::DiagnosticRelatedInformation {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, location);
        record_field!(recorder, self, message);
    }
}

impl MemorySize for ls_types::Command {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, title);
        record_field!(recorder, self, command);
        record_field!(recorder, self, arguments);
    }
}

impl MemorySize for ls_types::TextEdit {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, range);
        record_field!(recorder, self, new_text);
    }
}

impl MemorySize for ls_types::DocumentSymbol {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, name);
        record_field!(recorder, self, detail);
        record_field!(recorder, self, kind);
        record_field!(recorder, self, tags);
        #[allow(deprecated)]
        {
            record_field!(recorder, self, deprecated);
        }
        record_field!(recorder, self, range);
        record_field!(recorder, self, selection_range);
        record_field!(recorder, self, children);
    }
}

impl MemorySize for ls_types::SymbolInformation {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, name);
        record_field!(recorder, self, kind);
        record_field!(recorder, self, tags);
        #[allow(deprecated)]
        {
            record_field!(recorder, self, deprecated);
        }
        record_field!(recorder, self, location);
        record_field!(recorder, self, container_name);
    }
}

impl MemorySize for ls_types::WorkspaceLocation {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, uri);
    }
}

impl MemorySize for ls_types::WorkspaceSymbol {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, name);
        record_field!(recorder, self, kind);
        record_field!(recorder, self, tags);
        record_field!(recorder, self, container_name);
        record_field!(recorder, self, location);
        record_field!(recorder, self, data);
    }
}

impl MemorySize for ls_types::WorkspaceSymbolResponse {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            ls_types::WorkspaceSymbolResponse::Flat(symbols) => {
                recorder.scope("flat", |recorder| symbols.record_memory_children(recorder));
            }
            ls_types::WorkspaceSymbolResponse::Nested(symbols) => {
                recorder.scope("nested", |recorder| {
                    symbols.record_memory_children(recorder)
                });
            }
        }
    }
}

impl MemorySize for ls_types::InsertReplaceEdit {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, new_text);
        record_field!(recorder, self, insert);
        record_field!(recorder, self, replace);
    }
}

impl MemorySize for ls_types::CompletionTextEdit {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            ls_types::CompletionTextEdit::Edit(edit) => {
                recorder.scope("edit", |recorder| edit.record_memory_children(recorder));
            }
            ls_types::CompletionTextEdit::InsertAndReplace(edit) => {
                recorder.scope("insert_replace", |recorder| {
                    edit.record_memory_children(recorder)
                });
            }
        }
    }
}

impl MemorySize for ls_types::CompletionItemLabelDetails {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, detail);
        record_field!(recorder, self, description);
    }
}

impl MemorySize for ls_types::CompletionItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, label);
        record_field!(recorder, self, label_details);
        record_field!(recorder, self, kind);
        record_field!(recorder, self, detail);
        record_field!(recorder, self, documentation);
        record_field!(recorder, self, deprecated);
        record_field!(recorder, self, preselect);
        record_field!(recorder, self, sort_text);
        record_field!(recorder, self, filter_text);
        record_field!(recorder, self, insert_text);
        record_field!(recorder, self, insert_text_format);
        record_field!(recorder, self, insert_text_mode);
        record_field!(recorder, self, text_edit);
        record_field!(recorder, self, additional_text_edits);
        record_field!(recorder, self, command);
        record_field!(recorder, self, commit_characters);
        record_field!(recorder, self, data);
        record_field!(recorder, self, tags);
    }
}

impl MemorySize for ls_types::Documentation {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            ls_types::Documentation::String(value) => {
                recorder.scope("string", |recorder| value.record_memory_children(recorder));
            }
            ls_types::Documentation::MarkupContent(markup) => {
                recorder.scope("markup", |recorder| markup.record_memory_children(recorder));
            }
        }
    }
}

impl MemorySize for ls_types::LanguageString {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, language);
        record_field!(recorder, self, value);
    }
}

impl MemorySize for ls_types::MarkedString {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            ls_types::MarkedString::String(value) => {
                recorder.scope("string", |recorder| value.record_memory_children(recorder));
            }
            ls_types::MarkedString::LanguageString(value) => {
                recorder.scope("language_string", |recorder| {
                    value.record_memory_children(recorder)
                });
            }
        }
    }
}

impl MemorySize for ls_types::MarkupContent {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, kind);
        record_field!(recorder, self, value);
    }
}

impl MemorySize for ls_types::Hover {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, contents);
        record_field!(recorder, self, range);
    }
}

impl MemorySize for ls_types::HoverContents {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            ls_types::HoverContents::Scalar(value) => {
                recorder.scope("scalar", |recorder| value.record_memory_children(recorder));
            }
            ls_types::HoverContents::Array(values) => {
                recorder.scope("array", |recorder| values.record_memory_children(recorder));
            }
            ls_types::HoverContents::Markup(markup) => {
                recorder.scope("markup", |recorder| markup.record_memory_children(recorder));
            }
        }
    }
}

impl MemorySize for ls_types::InlayHint {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, position);
        record_field!(recorder, self, label);
        record_field!(recorder, self, kind);
        record_field!(recorder, self, text_edits);
        record_field!(recorder, self, tooltip);
        record_field!(recorder, self, padding_left);
        record_field!(recorder, self, padding_right);
        record_field!(recorder, self, data);
    }
}

impl MemorySize for ls_types::InlayHintLabel {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            ls_types::InlayHintLabel::String(value) => {
                recorder.scope("string", |recorder| value.record_memory_children(recorder));
            }
            ls_types::InlayHintLabel::LabelParts(parts) => {
                recorder.scope("parts", |recorder| parts.record_memory_children(recorder));
            }
        }
    }
}

impl MemorySize for ls_types::InlayHintTooltip {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            ls_types::InlayHintTooltip::String(value) => {
                recorder.scope("string", |recorder| value.record_memory_children(recorder));
            }
            ls_types::InlayHintTooltip::MarkupContent(markup) => {
                recorder.scope("markup", |recorder| markup.record_memory_children(recorder));
            }
        }
    }
}

impl MemorySize for ls_types::InlayHintLabelPart {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_field!(recorder, self, value);
        record_field!(recorder, self, tooltip);
        record_field!(recorder, self, location);
        record_field!(recorder, self, command);
    }
}

impl MemorySize for ls_types::InlayHintLabelPartTooltip {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            ls_types::InlayHintLabelPartTooltip::String(value) => {
                recorder.scope("string", |recorder| value.record_memory_children(recorder));
            }
            ls_types::InlayHintLabelPartTooltip::MarkupContent(markup) => {
                recorder.scope("markup", |recorder| markup.record_memory_children(recorder));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{MemoryRecorder, MemorySize};

    #[test]
    fn records_diagnostic_owned_payloads() {
        let diagnostic = ls_types::Diagnostic {
            range: ls_types::Range::new(
                ls_types::Position::new(1, 2),
                ls_types::Position::new(1, 5),
            ),
            severity: Some(ls_types::DiagnosticSeverity::WARNING),
            source: Some("cargo check".to_owned()),
            message: "unused variable".to_owned(),
            ..ls_types::Diagnostic::default()
        };

        let mut recorder = MemoryRecorder::new("diagnostic");
        diagnostic.record_memory_size(&mut recorder);
        let totals = recorder.totals_by_path();

        assert!(totals.contains_key("diagnostic"));
        assert!(totals.contains_key("diagnostic.source.some"));
        assert!(totals.contains_key("diagnostic.message"));
    }

    #[test]
    fn records_completion_docs_and_label_details() {
        let completion = ls_types::CompletionItem {
            label: "new".to_owned(),
            label_details: Some(ls_types::CompletionItemLabelDetails {
                detail: Some("() -> User".to_owned()),
                description: Some("app::User".to_owned()),
            }),
            documentation: Some(ls_types::Documentation::MarkupContent(
                ls_types::MarkupContent {
                    kind: ls_types::MarkupKind::Markdown,
                    value: "Create a user.".to_owned(),
                },
            )),
            ..ls_types::CompletionItem::default()
        };

        let mut recorder = MemoryRecorder::new("completion");
        completion.record_memory_size(&mut recorder);
        let totals = recorder.totals_by_path();

        assert!(totals.contains_key("completion.label"));
        assert!(totals.contains_key("completion.label_details.some.detail.some"));
        assert!(totals.contains_key("completion.documentation.some.markup.value"));
    }
}
