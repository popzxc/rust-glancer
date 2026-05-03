//! LSP-side project shape reporting.
//!
//! These counters describe the retained analysis graph, not allocator behavior. Keeping them out
//! of memory reporting makes save/index logs easier to read and keeps each subsystem honest.

use rg_project::AnalysisSnapshot;

use crate::memory::format_bytes;

/// Coarse counters for the analysis snapshot currently served by the LSP.
#[derive(Debug, Clone, Copy, derive_more::Display)]
#[display("{:?}", self)]
pub(crate) struct ProjectStats {
    package_count: usize,
    workspace_package_count: usize,
    def_map_targets: usize,
    def_map_modules: usize,
    unresolved_imports: usize,
    semantic_targets: usize,
    semantic_type_defs: usize,
    semantic_traits: usize,
    semantic_impls: usize,
    semantic_functions: usize,
    body_targets: usize,
    body_built_targets: usize,
    body_skipped_targets: usize,
    body_count: usize,
    expression_count: usize,
}

impl ProjectStats {
    pub(crate) fn capture(snapshot: AnalysisSnapshot<'_>) -> Self {
        let parse_db = snapshot.parse_db();
        let def_map_stats = snapshot.def_map_db().stats();
        let semantic_ir_stats = snapshot.semantic_ir_db().stats();
        let body_ir_stats = snapshot.body_ir_db().stats();

        Self {
            package_count: parse_db.package_count(),
            workspace_package_count: parse_db.workspace_packages().count(),
            def_map_targets: def_map_stats.target_count,
            def_map_modules: def_map_stats.module_count,
            unresolved_imports: def_map_stats.unresolved_import_count,
            semantic_targets: semantic_ir_stats.target_count,
            semantic_type_defs: semantic_ir_stats.struct_count
                + semantic_ir_stats.enum_count
                + semantic_ir_stats.union_count,
            semantic_traits: semantic_ir_stats.trait_count,
            semantic_impls: semantic_ir_stats.impl_count,
            semantic_functions: semantic_ir_stats.function_count,
            body_targets: body_ir_stats.target_count,
            body_built_targets: body_ir_stats.built_target_count,
            body_skipped_targets: body_ir_stats.skipped_target_count,
            body_count: body_ir_stats.body_count,
            expression_count: body_ir_stats.expression_count,
        }
    }

    pub(crate) fn log_info(self, label: &'static str) {
        tracing::info!(
            label,
            stats = %self,
            "project stats"
        );
    }
}

pub(crate) fn log_retained_memory(snapshot: AnalysisSnapshot<'_>, label: &'static str) {
    if !tracing::enabled!(target: "rg_lsp::memory", tracing::Level::DEBUG) {
        return;
    }

    // Retained-memory accounting walks the full analysis graph. Keep it opt-in so normal editor
    // logs get cheap counters without slowing every save.
    let retained_bytes = snapshot.retained_memory_bytes();
    tracing::debug!(
        target: "rg_lsp::memory",
        label,
        retained_bytes,
        retained = %format_bytes(retained_bytes),
        "analysis retained memory"
    );
}
