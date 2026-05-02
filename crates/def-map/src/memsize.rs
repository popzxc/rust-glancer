use rg_memsize::{MemoryRecorder, MemorySize};

use crate::{
    DefId, DefMap, DefMapDb, DefMapPackageBundle, DefMapStats, ImportBinding, ImportData, ImportId,
    ImportKind, ImportPath, ImportRef, ImportSourcePath, LocalDefData, LocalDefId, LocalDefKind,
    LocalDefRef, LocalImplData, LocalImplId, LocalImplRef, ModuleData, ModuleId, ModuleOrigin,
    ModuleRef, ModuleScope, Package, Path, PathSegment, ScopeBinding, ScopeEntry, TargetRef,
    import::ImportSourcePathSegment, scope::ScopeNameEntry,
};

macro_rules! record_fields {
    ($recorder:expr, $owner:expr, $($field:ident),+ $(,)?) => {
        $(
            $recorder.scope(stringify!($field), |recorder| {
                $owner.$field.record_memory_children(recorder);
            });
        )+
    };
}

impl MemorySize for DefMapDb {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("packages", |recorder| {
            self.packages.record_memory_children(recorder);
        });
    }
}

impl MemorySize for DefMapPackageBundle {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("package", |recorder| {
            self.package().record_memory_children(recorder);
        });
    }
}

impl MemorySize for Package {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, name, target_names, targets);
    }
}

impl MemorySize for DefMap {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            root_module,
            extern_prelude,
            prelude,
            modules,
            local_defs,
            local_impls,
            imports,
        );
    }
}

impl MemorySize for ModuleData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            name,
            name_span,
            docs,
            parent,
            children,
            local_defs,
            impls,
            imports,
            unresolved_imports,
            scope,
            origin,
        );
    }
}

impl MemorySize for ModuleOrigin {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Root { file_id } => file_id.record_memory_children(recorder),
            Self::Inline {
                declaration_file,
                declaration_span,
            } => {
                recorder.scope("declaration_file", |recorder| {
                    declaration_file.record_memory_children(recorder);
                });
                recorder.scope("declaration_span", |recorder| {
                    declaration_span.record_memory_children(recorder);
                });
            }
            Self::OutOfLine {
                declaration_file,
                declaration_span,
                definition_file,
            } => {
                recorder.scope("declaration_file", |recorder| {
                    declaration_file.record_memory_children(recorder);
                });
                recorder.scope("declaration_span", |recorder| {
                    declaration_span.record_memory_children(recorder);
                });
                recorder.scope("definition_file", |recorder| {
                    definition_file.record_memory_children(recorder);
                });
            }
        }
    }
}

impl MemorySize for LocalDefData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder, self, module, name, kind, visibility, source, file_id, name_span, span,
        );
    }
}

impl MemorySize for LocalImplData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, module, source, file_id, span);
    }
}

impl MemorySize for LocalDefKind {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for ModuleScope {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("entries", |recorder| {
            self.entries.record_memory_children(recorder);
        });
    }
}

impl MemorySize for ScopeNameEntry {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, name, entry);
    }
}

impl MemorySize for ScopeEntry {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, types, values, macros);
    }
}

impl MemorySize for ScopeBinding {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, def, visibility, owner);
    }
}

impl MemorySize for ImportData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            module,
            visibility,
            kind,
            path,
            source_path,
            binding,
            alias_span,
            source,
            import_index,
        );
    }
}

impl MemorySize for ImportBinding {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Inferred | Self::Hidden => {}
            Self::Explicit(name) => name.record_memory_children(recorder),
        }
    }
}

impl MemorySize for ImportKind {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for ImportPath {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, absolute, segments);
    }
}

impl MemorySize for ImportSourcePath {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, absolute, segments);
    }
}

impl MemorySize for ImportSourcePathSegment {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, segment, span);
    }
}

impl MemorySize for Path {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, absolute, segments);
    }
}

impl MemorySize for PathSegment {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Name(name) => name.record_memory_children(recorder),
            Self::SelfKw | Self::SuperKw | Self::CrateKw => {}
        }
    }
}

impl MemorySize for ModuleId {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for LocalDefId {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for LocalImplId {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for ImportId {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for TargetRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, package, target);
    }
}

impl MemorySize for ModuleRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, module);
    }
}

impl MemorySize for LocalDefRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, local_def);
    }
}

impl MemorySize for LocalImplRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, local_impl);
    }
}

impl MemorySize for ImportRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, import);
    }
}

impl MemorySize for DefId {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Module(module) => module.record_memory_children(recorder),
            Self::Local(local) => local.record_memory_children(recorder),
        }
    }
}

impl MemorySize for DefMapStats {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            target_count,
            module_count,
            local_def_count,
            local_impl_count,
            import_count,
            unresolved_import_count,
        );
    }
}
