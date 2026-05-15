//! Qualified-path completion queries over module scopes.
//!
//! Completion uses the same path resolution and visibility checks as imports. This module keeps
//! those rules inside DefMap and exposes only the visible definitions that analysis can render.

use rg_package_store::PackageStoreError;
use rg_parse::{FileId, Span, TextSpan};

use crate::{
    DefId, DefMap, DefMapReadTxn, ImportSourcePath, ModuleRef, Path, TargetRef,
    query::path_resolution,
};

/// Source site selected for a qualified import-path completion query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefMapPathCompletionSite {
    pub module: ModuleRef,
    /// Path before the segment being completed.
    pub qualifier: Path,
    /// Segment prefix already typed after `::`.
    pub member_prefix_span: Span,
}

/// Namespace slot occupied by a visible module-scope definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeNamespace {
    Types,
    Values,
    Macros,
}

impl ScopeNamespace {
    fn sort_rank(self) -> u8 {
        match self {
            Self::Types => 0,
            Self::Values => 1,
            Self::Macros => 2,
        }
    }
}

/// One definition visible from a module through another module's scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleScopeDef {
    pub label: String,
    pub namespace: ScopeNamespace,
    pub def: DefId,
}

impl DefMapReadTxn<'_> {
    /// Returns the source site for a qualified import-path completion query.
    pub fn path_completion_site(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Result<Option<DefMapPathCompletionSite>, PackageStoreError> {
        PathCompletionSiteScanner {
            def_map: self,
            target,
            file_id,
            offset,
        }
        .scan()
    }

    /// Returns definitions from `source_module` that are visible from `importing_module`.
    pub fn visible_scope_defs(
        &self,
        importing_module: ModuleRef,
        source_module: ModuleRef,
    ) -> Result<Vec<VisibleScopeDef>, PackageStoreError> {
        let scope = path_resolution::visible_module_scope_entry_set_with_env(
            self,
            importing_module,
            source_module,
        )?;
        let mut defs = Vec::new();

        // The visibility-aware builder keeps namespace buckets separate. Analysis filters those
        // buckets according to the syntactic context where completion was requested.
        for (name, entry) in scope.entries() {
            for binding in entry.types() {
                defs.push(VisibleScopeDef {
                    label: name.to_string(),
                    namespace: ScopeNamespace::Types,
                    def: binding.def,
                });
            }
            for binding in entry.values() {
                defs.push(VisibleScopeDef {
                    label: name.to_string(),
                    namespace: ScopeNamespace::Values,
                    def: binding.def,
                });
            }
            for binding in entry.macros() {
                defs.push(VisibleScopeDef {
                    label: name.to_string(),
                    namespace: ScopeNamespace::Macros,
                    def: binding.def,
                });
            }
        }

        defs.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then(left.namespace.sort_rank().cmp(&right.namespace.sort_rank()))
                .then(format!("{:?}", left.def).cmp(&format!("{:?}", right.def)))
        });
        Ok(defs)
    }
}

/// Scans import paths owned by DefMap.
struct PathCompletionSiteScanner<'txn, 'db> {
    def_map: &'txn DefMapReadTxn<'db>,
    target: TargetRef,
    file_id: FileId,
    offset: u32,
}

impl PathCompletionSiteScanner<'_, '_> {
    fn scan(&self) -> Result<Option<DefMapPathCompletionSite>, PackageStoreError> {
        let Some(def_map) = self.def_map.def_map(self.target)? else {
            return Ok(None);
        };
        let mut best: Option<(DefMapPathCompletionSite, u32)> = None;

        self.scan_import_paths(def_map, &mut best);
        Ok(best.map(|(site, _)| site))
    }

    fn scan_import_paths(
        &self,
        def_map: &DefMap,
        best: &mut Option<(DefMapPathCompletionSite, u32)>,
    ) {
        for import in def_map.imports() {
            if import.source.file_id != self.file_id {
                continue;
            }
            let module = ModuleRef {
                target: self.target,
                module: import.module,
            };
            let Some((site, source_len)) = self.site_for_import_path(module, &import.source_path)
            else {
                continue;
            };

            if best
                .as_ref()
                .is_none_or(|(_, best_len)| source_len < *best_len)
            {
                *best = Some((site, source_len));
            }
        }
    }

    /// Finds either a partially typed path segment or an empty segment after a trailing `::`.
    fn site_for_import_path(
        &self,
        module: ModuleRef,
        path: &ImportSourcePath,
    ) -> Option<(DefMapPathCompletionSite, u32)> {
        for (idx, segment) in path.segments.iter().enumerate().skip(1) {
            if !segment.span.touches(self.offset) {
                continue;
            }

            return Some((
                DefMapPathCompletionSite {
                    module,
                    qualifier: Path {
                        absolute: path.absolute,
                        segments: path
                            .segments
                            .iter()
                            .take(idx)
                            .map(|segment| segment.segment.clone())
                            .collect(),
                    },
                    member_prefix_span: segment.span,
                },
                path.source_span.unwrap_or(segment.span).len(),
            ));
        }

        let source_span = path.source_span()?;
        let last_segment = path.segments.last()?;
        let offset_after_last_segment =
            last_segment.span.text.end <= self.offset && self.offset <= source_span.text.end;
        if source_span.text.end <= last_segment.span.text.end || !offset_after_last_segment {
            return None;
        }

        Some((
            DefMapPathCompletionSite {
                module,
                qualifier: Path {
                    absolute: path.absolute,
                    segments: path
                        .segments
                        .iter()
                        .map(|segment| segment.segment.clone())
                        .collect(),
                },
                member_prefix_span: Span {
                    text: TextSpan {
                        start: self.offset,
                        end: self.offset,
                    },
                },
            },
            source_span.len(),
        ))
    }
}
