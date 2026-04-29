//! Cursor-oriented queries over the frozen namespace map.
//!
//! DefMap owns module-scope source facts such as local definition names and import path spans.
//! Analysis can therefore ask for cursor candidates without reaching back into item-tree storage.

use rg_parse::{FileId, Span};

use crate::{DefId, DefMapDb, ModuleOrigin, ModuleRef, Path, TargetRef};

/// One def-map source node that can participate in cursor queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefMapCursorCandidate {
    Def {
        def: DefId,
        span: Span,
    },
    UsePath {
        module: ModuleRef,
        path: Path,
        span: Span,
    },
}

impl DefMapDb {
    pub fn cursor_candidates(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> Vec<DefMapCursorCandidate> {
        let mut candidates = Vec::new();
        self.push_module_candidates(target, file_id, offset, &mut candidates);
        self.push_local_def_candidates(target, file_id, offset, &mut candidates);
        self.push_import_candidates(target, file_id, offset, &mut candidates);
        candidates
    }

    fn push_module_candidates(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
        candidates: &mut Vec<DefMapCursorCandidate>,
    ) {
        for (module_ref, module) in self.modules(target) {
            let declaration_file = match module.origin {
                ModuleOrigin::Root { .. } => continue,
                ModuleOrigin::Inline {
                    declaration_file, ..
                }
                | ModuleOrigin::OutOfLine {
                    declaration_file, ..
                } => declaration_file,
            };
            if declaration_file != file_id {
                continue;
            }

            let Some(span) = module.name_span else {
                continue;
            };
            if span.touches(offset) {
                candidates.push(DefMapCursorCandidate::Def {
                    def: DefId::Module(module_ref),
                    span,
                });
            }
        }
    }

    fn push_local_def_candidates(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
        candidates: &mut Vec<DefMapCursorCandidate>,
    ) {
        for (local_def_ref, local_def) in self.local_defs(target) {
            if local_def.file_id != file_id {
                continue;
            }

            let span = local_def.name_span.unwrap_or(local_def.span);
            if span.touches(offset) {
                candidates.push(DefMapCursorCandidate::Def {
                    def: DefId::Local(local_def_ref),
                    span,
                });
            }
        }
    }

    fn push_import_candidates(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
        candidates: &mut Vec<DefMapCursorCandidate>,
    ) {
        for (_, import) in self.imports(target) {
            if import.source.file_id != file_id {
                continue;
            }

            let module = ModuleRef {
                target,
                module: import.module,
            };
            for (idx, segment) in import.source_path.segments().iter().enumerate() {
                if segment.span.touches(offset) {
                    candidates.push(DefMapCursorCandidate::UsePath {
                        module,
                        path: import.source_path.prefix_path(idx),
                        span: segment.span,
                    });
                }
            }

            if let Some(alias_span) = import.alias_span
                && alias_span.touches(offset)
            {
                candidates.push(DefMapCursorCandidate::UsePath {
                    module,
                    path: Path::from(&import.path),
                    span: alias_span,
                });
            }
        }
    }
}
