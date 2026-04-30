use rg_project::AnalysisSnapshot;

/// Allocator counters collected by the executable that selected the allocator.
///
/// The LSP crate receives these through `MemoryControl`, so it can observe allocator behavior
/// without depending on, or accidentally choosing, a concrete global allocator itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocatorStats {
    pub allocated_bytes: usize,
    pub active_bytes: usize,
    pub resident_bytes: usize,
    pub mapped_bytes: usize,
    pub retained_bytes: usize,
}

/// Outcome of one allocator purge attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocatorPurgeResult {
    pub tcache_flushed: bool,
    pub arenas_purged: bool,
}

/// Runtime memory controls supplied by the executable.
///
/// The default implementation is intentionally empty. The binary can provide allocator-specific
/// controls, while tests and non-jemalloc builds keep the server behavior deterministic.
pub trait MemoryControl: std::fmt::Debug + Send + Sync {
    fn allocator_name(&self) -> &'static str {
        "unknown"
    }

    fn allocator_purge_enabled(&self) -> bool {
        false
    }

    fn allocator_stats(&self) -> Option<AllocatorStats> {
        None
    }

    fn try_purge_allocator(&self) -> Option<AllocatorPurgeResult> {
        None
    }
}

impl MemoryControl for () {}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MemoryStats {
    allocator: Option<AllocatorStats>,
}

impl MemoryStats {
    pub(crate) fn capture(memory_control: &dyn MemoryControl) -> Self {
        Self {
            allocator: memory_control.allocator_stats(),
        }
    }

    pub(crate) fn report(
        self,
        memory_control: &dyn MemoryControl,
        snapshot: AnalysisSnapshot<'_>,
        label: &'static str,
        purge: Option<MemoryPurge>,
    ) {
        let parse_db = snapshot.parse_db();
        let def_map_stats = snapshot.def_map_db().stats();
        let semantic_ir_stats = snapshot.semantic_ir_db().stats();
        let body_ir_stats = snapshot.body_ir_db().stats();
        let purge_result = purge.map(|purge| purge.result);

        tracing::info!(
            label,
            allocator = memory_control.allocator_name(),
            allocator_purge_enabled = memory_control.allocator_purge_enabled(),
            allocator_purged = purge_result.is_some(),
            tcache_flushed = purge_result.map(|result| result.tcache_flushed),
            arenas_purged = purge_result.map(|result| result.arenas_purged),
            allocator_stats_available = self.allocator.is_some(),
            allocator_allocated = %format_optional_bytes(self.allocated_bytes()),
            allocator_active = %format_optional_bytes(self.active_bytes()),
            allocator_resident = %format_optional_bytes(self.resident_bytes()),
            allocator_mapped = %format_optional_bytes(self.mapped_bytes()),
            allocator_retained = %format_optional_bytes(self.retained_bytes()),
            "allocation info"
        );
        if let Some(purge) = purge {
            tracing::info!(
                allocator_allocated_delta = %format_optional_byte_delta(purge.allocated_delta()),
                allocator_active_delta = %format_optional_byte_delta(purge.active_delta()),
                allocator_resident_delta = %format_optional_byte_delta(purge.resident_delta()),
                allocator_mapped_delta = %format_optional_byte_delta(purge.mapped_delta()),
                package_count = parse_db.package_count(),
                workspace_package_count = parse_db.workspace_packages().count(),
                def_map_targets = def_map_stats.target_count,
                def_map_modules = def_map_stats.module_count,
                unresolved_imports = def_map_stats.unresolved_import_count,
                semantic_targets = semantic_ir_stats.target_count,
                semantic_type_defs = semantic_ir_stats.struct_count
                    + semantic_ir_stats.enum_count
                    + semantic_ir_stats.union_count,
                semantic_traits = semantic_ir_stats.trait_count,
                semantic_impls = semantic_ir_stats.impl_count,
                semantic_functions = semantic_ir_stats.function_count,
                body_targets = body_ir_stats.target_count,
                body_built_targets = body_ir_stats.built_target_count,
                body_skipped_targets = body_ir_stats.skipped_target_count,
                body_count = body_ir_stats.body_count,
                expression_count = body_ir_stats.expression_count,
                "purge stats"
            );
        }
        tracing::info!(
            package_count = parse_db.package_count(),
            workspace_package_count = parse_db.workspace_packages().count(),
            def_map_targets = def_map_stats.target_count,
            def_map_modules = def_map_stats.module_count,
            unresolved_imports = def_map_stats.unresolved_import_count,
            semantic_targets = semantic_ir_stats.target_count,
            semantic_type_defs = semantic_ir_stats.struct_count
                + semantic_ir_stats.enum_count
                + semantic_ir_stats.union_count,
            semantic_traits = semantic_ir_stats.trait_count,
            semantic_impls = semantic_ir_stats.impl_count,
            semantic_functions = semantic_ir_stats.function_count,
            body_targets = body_ir_stats.target_count,
            body_built_targets = body_ir_stats.built_target_count,
            body_skipped_targets = body_ir_stats.skipped_target_count,
            body_count = body_ir_stats.body_count,
            expression_count = body_ir_stats.expression_count,
            "parse stats"
        );

        if tracing::enabled!(target: "rg_lsp::memory", tracing::Level::DEBUG) {
            // Retained-memory accounting walks the full analysis graph. Keep it opt-in so normal
            // editor logs get cheap allocator counters without slowing every save.
            let retained_bytes = snapshot.retained_memory_bytes();
            tracing::debug!(
                target: "rg_lsp::memory",
                label,
                retained_bytes,
                retained = %format_bytes(retained_bytes),
                "analysis retained memory"
            );
        }
    }

    fn allocated_bytes(self) -> Option<usize> {
        self.allocator.map(|stats| stats.allocated_bytes)
    }

    fn active_bytes(self) -> Option<usize> {
        self.allocator.map(|stats| stats.active_bytes)
    }

    fn resident_bytes(self) -> Option<usize> {
        self.allocator.map(|stats| stats.resident_bytes)
    }

    fn mapped_bytes(self) -> Option<usize> {
        self.allocator.map(|stats| stats.mapped_bytes)
    }

    fn retained_bytes(self) -> Option<usize> {
        self.allocator.map(|stats| stats.retained_bytes)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MemoryPurge {
    result: AllocatorPurgeResult,
    before: MemoryStats,
    after: MemoryStats,
}

impl MemoryPurge {
    pub(crate) fn try_purge(
        memory_control: &dyn MemoryControl,
        before: MemoryStats,
    ) -> Option<Self> {
        let result = memory_control.try_purge_allocator()?;
        let after = MemoryStats::capture(memory_control);

        Some(Self {
            result,
            before,
            after,
        })
    }

    pub(crate) fn after(self) -> MemoryStats {
        self.after
    }

    fn allocated_delta(self) -> Option<i64> {
        byte_delta(self.after.allocated_bytes(), self.before.allocated_bytes())
    }

    fn active_delta(self) -> Option<i64> {
        byte_delta(self.after.active_bytes(), self.before.active_bytes())
    }

    fn resident_delta(self) -> Option<i64> {
        byte_delta(self.after.resident_bytes(), self.before.resident_bytes())
    }

    fn mapped_delta(self) -> Option<i64> {
        byte_delta(self.after.mapped_bytes(), self.before.mapped_bytes())
    }
}

fn format_bytes(bytes: usize) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    let mut value = bytes as f64;
    let mut unit = UNITS[0];
    for next_unit in UNITS.iter().skip(1) {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next_unit;
    }

    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {unit}")
    }
}

fn format_optional_bytes(bytes: Option<usize>) -> String {
    bytes.map(format_bytes).unwrap_or_else(|| "-".to_string())
}

fn byte_delta(after: Option<usize>, before: Option<usize>) -> Option<i64> {
    let after = i64::try_from(after?).ok()?;
    let before = i64::try_from(before?).ok()?;
    Some(after - before)
}

fn format_optional_byte_delta(delta: Option<i64>) -> String {
    let Some(delta) = delta else {
        return "-".to_string();
    };

    let prefix = if delta >= 0 { "+" } else { "-" };
    let bytes = delta.unsigned_abs();
    let bytes = usize::try_from(bytes).ok().map(format_bytes);
    match bytes {
        Some(bytes) => format!("{prefix}{bytes}"),
        None => format!("{delta} B"),
    }
}
