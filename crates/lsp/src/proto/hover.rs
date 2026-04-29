use rg_analysis::HoverInfo;
use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind};

pub(crate) fn hover(info: HoverInfo) -> Option<Hover> {
    let value = HoverMarkdown::from_info(info).finish()?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    })
}

struct HoverMarkdown {
    sections: Vec<String>,
}

impl HoverMarkdown {
    fn from_info(info: HoverInfo) -> Self {
        let mut sections = Vec::new();

        if let Some(signature) = info.signature {
            sections.push(format!("```rust\n{signature}\n```"));
        } else if let Some(ty) = info.ty {
            sections.push(format!("```rust\n{ty}\n```"));
        }

        if let Some(docs) = info.docs {
            let docs = docs.trim();
            if !docs.is_empty() {
                sections.push(docs.to_string());
            }
        }

        Self { sections }
    }

    fn finish(self) -> Option<String> {
        (!self.sections.is_empty()).then(|| self.sections.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use rg_analysis::{HoverInfo, SymbolKind};

    use super::HoverMarkdown;

    #[test]
    fn renders_signature_and_docs_as_markdown() {
        let markdown = HoverMarkdown::from_info(HoverInfo {
            kind: SymbolKind::Struct,
            signature: Some("pub struct User".to_string()),
            ty: None,
            docs: Some("User account.".to_string()),
        })
        .finish();

        assert_eq!(
            markdown.as_deref(),
            Some("```rust\npub struct User\n```\n\nUser account.")
        );
    }
}
