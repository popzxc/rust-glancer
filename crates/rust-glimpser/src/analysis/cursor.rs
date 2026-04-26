use crate::{
    body_ir::{BodyData, BodyRef, ScopeId, StmtKind},
    def_map::{DefId, ModuleRef, Path, TargetRef},
    item_tree::{
        GenericArg, ItemKind, ItemNode, ItemTreeRef, TypeBound, TypePath, TypeRef, UsePath,
    },
    parse::{FileId, span::Span},
    semantic_ir::SemanticCursorCandidate,
};

use super::{
    Analysis,
    data::{SymbolAt, SymbolCandidate},
};

pub(super) fn body_type_path_candidates(
    body_ref: BodyRef,
    body: &BodyData,
    file_id: FileId,
    offset: u32,
) -> Vec<SymbolCandidate> {
    let mut candidates = Vec::new();
    for statement in &body.statements {
        if statement.source.file_id != file_id {
            continue;
        }
        let StmtKind::Let {
            scope,
            annotation: Some(annotation),
            ..
        } = &statement.kind
        else {
            continue;
        };
        BodyTypeCursorScanner {
            body: body_ref,
            scope: *scope,
            offset,
            candidates: &mut candidates,
        }
        .scan_type_ref(annotation);
    }

    candidates
}

pub(super) fn item_signature_candidates(
    analysis: &Analysis<'_>,
    target: TargetRef,
    file_id: FileId,
    offset: u32,
) -> Vec<SymbolCandidate> {
    let mut candidates = Vec::new();

    CursorScanner {
        analysis,
        target,
        file_id,
        offset,
        candidates: &mut candidates,
    }
    .scan();

    candidates
}

struct BodyTypeCursorScanner<'a> {
    body: BodyRef,
    scope: ScopeId,
    offset: u32,
    candidates: &'a mut Vec<SymbolCandidate>,
}

impl BodyTypeCursorScanner<'_> {
    fn scan_type_ref(&mut self, ty: &TypeRef) {
        match ty {
            TypeRef::Path(path) => self.scan_type_path(path),
            TypeRef::Tuple(types) => {
                for ty in types {
                    self.scan_type_ref(ty);
                }
            }
            TypeRef::Reference { inner, .. }
            | TypeRef::RawPointer { inner, .. }
            | TypeRef::Slice(inner) => self.scan_type_ref(inner),
            TypeRef::Array { inner, .. } => self.scan_type_ref(inner),
            TypeRef::FnPointer { params, ret } => {
                for param in params {
                    self.scan_type_ref(param);
                }
                self.scan_type_ref(ret);
            }
            TypeRef::ImplTrait(bounds) | TypeRef::DynTrait(bounds) => {
                for bound in bounds {
                    if let TypeBound::Trait(ty) = bound {
                        self.scan_type_ref(ty);
                    }
                }
            }
            TypeRef::Unknown(_) | TypeRef::Never | TypeRef::Unit | TypeRef::Infer => {}
        }
    }

    fn scan_type_path(&mut self, path: &TypePath) {
        for (idx, segment) in path.segments.iter().enumerate() {
            if segment.span.touches(self.offset) {
                let path = Path::from_type_path_prefix(path, idx);
                self.candidates.push(SymbolCandidate {
                    symbol: SymbolAt::BodyPath {
                        body: self.body,
                        scope: self.scope,
                        path,
                        span: segment.span,
                    },
                    span: segment.span,
                });
            }

            for arg in &segment.args {
                self.scan_generic_arg(arg);
            }
        }
    }

    fn scan_generic_arg(&mut self, arg: &GenericArg) {
        match arg {
            GenericArg::Type(ty) => self.scan_type_ref(ty),
            GenericArg::AssocType { ty: Some(ty), .. } => self.scan_type_ref(ty),
            GenericArg::Lifetime(_)
            | GenericArg::Const(_)
            | GenericArg::AssocType { ty: None, .. }
            | GenericArg::Unsupported(_) => {}
        }
    }
}

struct CursorScanner<'a, 'db> {
    analysis: &'a Analysis<'db>,
    target: TargetRef,
    file_id: FileId,
    offset: u32,
    candidates: &'a mut Vec<SymbolCandidate>,
}

impl CursorScanner<'_, '_> {
    fn scan(&mut self) {
        self.scan_local_definitions();
        self.scan_import_paths();
        self.scan_semantic_items();
    }

    fn scan_local_definitions(&mut self) {
        for (local_def_ref, local_def) in self.analysis.def_map.local_defs(self.target) {
            if local_def.file_id != self.file_id {
                continue;
            }

            let span = self
                .item(local_def.source)
                .and_then(|item| item.name_span)
                .unwrap_or(local_def.span);
            if !span.touches(self.offset) {
                continue;
            }

            self.candidates.push(SymbolCandidate {
                symbol: SymbolAt::Def {
                    def: DefId::Local(local_def_ref),
                    span,
                },
                span,
            });
        }
    }

    fn scan_import_paths(&mut self) {
        for (_, import) in self.analysis.def_map.imports(self.target) {
            if import.source.file_id != self.file_id {
                continue;
            }

            let Some(source_import) = self
                .item(import.source)
                .and_then(|item| match &item.kind {
                    ItemKind::Use(use_item) => use_item.imports.get(import.import_index),
                    _ => None,
                })
                .cloned()
            else {
                continue;
            };
            let module = ModuleRef {
                target: self.target,
                module: import.module,
            };
            self.push_use_path(module, &source_import.path);
            if let crate::item_tree::ImportAlias::Explicit { span, .. } = source_import.alias {
                if span.touches(self.offset) {
                    self.push_use_path_candidate(
                        module,
                        Path::from_use_path(&source_import.path),
                        span,
                    );
                }
            }
        }
    }

    fn scan_semantic_items(&mut self) {
        let candidates = self.analysis.semantic_ir.signature_cursor_candidates(
            self.analysis.item_tree,
            self.target,
            self.file_id,
            self.offset,
        );
        for candidate in candidates {
            match candidate {
                SemanticCursorCandidate::Field { field, span } => {
                    self.candidates.push(SymbolCandidate {
                        symbol: SymbolAt::Field { field, span },
                        span,
                    });
                }
                SemanticCursorCandidate::Function { function, span } => {
                    self.candidates.push(SymbolCandidate {
                        symbol: SymbolAt::Function { function, span },
                        span,
                    });
                }
                SemanticCursorCandidate::TypePath {
                    context,
                    path,
                    span,
                } => {
                    self.candidates.push(SymbolCandidate {
                        symbol: SymbolAt::TypePath {
                            context,
                            path,
                            span,
                        },
                        span,
                    });
                }
            }
        }
    }

    fn push_use_path(&mut self, module: ModuleRef, path: &UsePath) {
        for (idx, segment) in path.segments.iter().enumerate() {
            if segment.span.touches(self.offset) {
                self.push_use_path_candidate(
                    module,
                    Path::from_use_path_prefix(path, idx),
                    segment.span,
                );
            }
        }
    }

    fn push_use_path_candidate(&mut self, module: ModuleRef, path: Path, span: Span) {
        self.candidates.push(SymbolCandidate {
            symbol: SymbolAt::UsePath { module, path, span },
            span,
        });
    }

    fn item(&self, source: ItemTreeRef) -> Option<&ItemNode> {
        self.analysis
            .item_tree
            .package(self.target.package.0)?
            .item(source)
    }
}
