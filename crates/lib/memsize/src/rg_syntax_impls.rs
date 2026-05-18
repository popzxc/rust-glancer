use rg_syntax::AstNode as _;

use crate::{MemoryRecorder, MemorySize};

crate::impl_memory_size_leaf!(
    rg_syntax::Edition,
    rg_syntax::SyntaxKind,
    rg_syntax::TextRange,
    rg_syntax::TextSize,
);

impl MemorySize for rg_syntax::ast::SourceFile {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("syntax", |recorder| {
            self.syntax().record_memory_children(recorder);
        });
    }
}

impl<T> MemorySize for rg_syntax::Parse<T> {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("syntax", |recorder| {
            self.syntax_node().record_memory_children(recorder);
        });
    }
}

impl MemorySize for rg_syntax::SyntaxNode {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        self.tree_memory_usage().record(recorder);
    }
}

impl MemorySize for rg_syntax::SyntaxToken {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        self.tree_memory_usage().record(recorder);
    }
}

trait RecordSyntaxTreeMemory {
    fn record(&self, recorder: &mut MemoryRecorder);
}

impl RecordSyntaxTreeMemory for rg_syntax::SyntaxTreeMemoryUsage {
    fn record(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("tree", |recorder| {
            recorder.scope("source", |recorder| {
                recorder.record_heap::<str>(self.source_bytes);
            });
            recorder.scope("nodes", |recorder| {
                recorder.record_type_name(
                    crate::MemoryRecordKind::Heap,
                    "rg_syntax::NodeData",
                    self.node_table_bytes,
                );
            });
            recorder.scope("tokens", |recorder| {
                recorder.record_type_name(
                    crate::MemoryRecordKind::Heap,
                    "rg_syntax::TokenData",
                    self.token_table_bytes,
                );
            });
            recorder.scope("children", |recorder| {
                recorder.record_type_name(
                    crate::MemoryRecordKind::Heap,
                    "rg_syntax::ElementId",
                    self.child_table_bytes,
                );
            });
            recorder.scope("errors", |recorder| {
                recorder.record_type_name(
                    crate::MemoryRecordKind::Heap,
                    "rg_syntax::SyntaxError",
                    self.error_bytes,
                );
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::{MemoryRecorder, MemorySize};

    #[test]
    fn records_source_file_syntax_tree_as_approximate_memory() {
        let file = rg_syntax::ast::SourceFile::parse(
            r#"
            struct User {
                name: String,
            }
            "#,
            rg_syntax::Edition::CURRENT,
        )
        .ok()
        .expect("test source should parse as a source file");

        let mut recorder = MemoryRecorder::new("source_file");
        file.record_memory_size(&mut recorder);
        let totals = recorder.totals_by_path();

        assert!(totals.contains_key("source_file.syntax.tree.source"));
        assert!(totals.contains_key("source_file.syntax.tree.nodes"));
        assert!(totals.contains_key("source_file.syntax.tree.tokens"));
        assert!(totals.contains_key("source_file.syntax.tree.children"));
    }

    #[test]
    fn records_text_ranges_as_shallow_values() {
        let range =
            rg_syntax::TextRange::new(rg_syntax::TextSize::new(1), rg_syntax::TextSize::new(4));

        assert_eq!(
            range.memory_size(),
            std::mem::size_of::<rg_syntax::TextRange>()
        );
    }
}
