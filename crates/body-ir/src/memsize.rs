use crate::{
    BindingData, BindingId, BindingKind, BodyData, BodyFieldRef, BodyFunctionData, BodyFunctionId,
    BodyFunctionOwner, BodyFunctionRef, BodyGenericArg, BodyId, BodyImplData, BodyImplId,
    BodyIrBuildPolicy, BodyIrDb, BodyIrPackageBundle, BodyIrStats, BodyItemData, BodyItemId,
    BodyItemKind, BodyItemRef, BodyLocalNominalTy, BodyNominalTy, BodyPath, BodyRef,
    BodyResolution, BodySource, BodyTy, BodyTypePathResolution, ExprData, ExprId, ExprKind,
    LiteralKind, PackageBodies, PatData, PatId, PatKind, RecordPatField, ResolvedFieldRef,
    ResolvedFunctionRef, ScopeData, ScopeId, StmtData, StmtKind, TargetBodies, TargetBodiesStatus,
    expr::{ExprWrapperKind, MatchArmData},
    ids::StmtId,
};
use rg_memsize::{MemoryRecorder, MemorySize};

macro_rules! record_fields {
    ($recorder:expr, $owner:expr, $($field:ident),+ $(,)?) => {
        $(
            $recorder.scope(stringify!($field), |recorder| {
                $owner.$field.record_memory_children(recorder);
            });
        )+
    };
}

macro_rules! impl_leaf_memory_size {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl MemorySize for $ty {
                fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
            }
        )+
    };
}

impl MemorySize for BodyIrDb {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("packages", |recorder| {
            self.record_packages_memory_children(recorder);
        });
    }
}

impl MemorySize for BodyIrPackageBundle {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("package", |recorder| {
            self.package().record_memory_children(recorder);
        });
    }
}

impl MemorySize for BodyIrBuildPolicy {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("package_scope", |recorder| {
            self.package_scope.record_memory_children(recorder);
        });
    }
}

impl MemorySize for crate::BodyIrPackageScope {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for PackageBodies {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("targets", |recorder| {
            self.targets.record_memory_children(recorder);
        });
    }
}

impl MemorySize for TargetBodies {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, status, function_bodies, bodies);
    }
}

impl MemorySize for TargetBodiesStatus {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for BodyData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            owner,
            owner_module,
            source,
            param_scope,
            root_expr,
            params,
            scopes,
            local_items,
            local_impls,
            local_functions,
            bindings,
            pats,
            statements,
            exprs,
        );
    }
}

impl MemorySize for BodySource {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, file_id, span);
    }
}

impl MemorySize for ScopeData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, parent, local_items, local_impls, bindings);
    }
}

impl MemorySize for ExprData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            source,
            scope,
            visible_bindings,
            kind,
            resolution,
            ty,
        );
    }
}

impl MemorySize for ExprKind {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Block {
                scope,
                statements,
                tail,
            } => {
                recorder.scope("scope", |recorder| scope.record_memory_children(recorder));
                recorder.scope("statements", |recorder| {
                    statements.record_memory_children(recorder);
                });
                recorder.scope("tail", |recorder| tail.record_memory_children(recorder));
            }
            Self::Path { path } => path.record_memory_children(recorder),
            Self::Call { callee, args } => {
                recorder.scope("callee", |recorder| callee.record_memory_children(recorder));
                recorder.scope("args", |recorder| args.record_memory_children(recorder));
            }
            Self::Match { scrutinee, arms } => {
                recorder.scope("scrutinee", |recorder| {
                    scrutinee.record_memory_children(recorder);
                });
                recorder.scope("arms", |recorder| arms.record_memory_children(recorder));
            }
            Self::MethodCall {
                receiver,
                dot_span,
                method_name,
                method_name_span,
                args,
            } => {
                recorder.scope("receiver", |recorder| {
                    receiver.record_memory_children(recorder);
                });
                recorder.scope("dot_span", |recorder| {
                    dot_span.record_memory_children(recorder)
                });
                recorder.scope("method_name", |recorder| {
                    method_name.record_memory_children(recorder);
                });
                recorder.scope("method_name_span", |recorder| {
                    method_name_span.record_memory_children(recorder);
                });
                recorder.scope("args", |recorder| args.record_memory_children(recorder));
            }
            Self::Field {
                base,
                dot_span,
                field,
                field_span,
            } => {
                recorder.scope("base", |recorder| base.record_memory_children(recorder));
                recorder.scope("dot_span", |recorder| {
                    dot_span.record_memory_children(recorder)
                });
                recorder.scope("field", |recorder| field.record_memory_children(recorder));
                recorder.scope("field_span", |recorder| {
                    field_span.record_memory_children(recorder);
                });
            }
            Self::Wrapper { kind, inner } => {
                recorder.scope("kind", |recorder| kind.record_memory_children(recorder));
                recorder.scope("inner", |recorder| inner.record_memory_children(recorder));
            }
            Self::Literal { kind } => kind.record_memory_children(recorder),
            Self::Unknown { children } => children.record_memory_children(recorder),
        }
    }
}

impl MemorySize for ExprWrapperKind {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for MatchArmData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, pat, scope, expr);
    }
}

impl MemorySize for LiteralKind {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for BodyPath {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, path, segment_spans);
    }
}

impl MemorySize for BodyResolution {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Local(binding) => binding.record_memory_children(recorder),
            Self::LocalItem(item) => item.record_memory_children(recorder),
            Self::Item(items) => items.record_memory_children(recorder),
            Self::Field(fields) => fields.record_memory_children(recorder),
            Self::Function(functions) | Self::Method(functions) => {
                functions.record_memory_children(recorder);
            }
            Self::EnumVariant(variants) => variants.record_memory_children(recorder),
            Self::Unknown => {}
        }
    }
}

impl MemorySize for ResolvedFieldRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Semantic(field) => field.record_memory_children(recorder),
            Self::BodyLocal(field) => field.record_memory_children(recorder),
        }
    }
}

impl MemorySize for ResolvedFunctionRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Semantic(function) => function.record_memory_children(recorder),
            Self::BodyLocal(function) => function.record_memory_children(recorder),
        }
    }
}

impl MemorySize for BodyTypePathResolution {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::BodyLocal(item) => item.record_memory_children(recorder),
            Self::SelfType(types) | Self::TypeDefs(types) => types.record_memory_children(recorder),
            Self::Traits(traits) => traits.record_memory_children(recorder),
            Self::Unknown => {}
        }
    }
}

impl MemorySize for BodyTy {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Unit | Self::Never | Self::Unknown => {}
            Self::Syntax(ty) => ty.record_memory_children(recorder),
            Self::Reference(inner) => inner.record_memory_children(recorder),
            Self::LocalNominal(types) => types.record_memory_children(recorder),
            Self::Nominal(types) | Self::SelfTy(types) => types.record_memory_children(recorder),
        }
    }
}

impl MemorySize for BodyLocalNominalTy {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, item, args);
    }
}

impl MemorySize for BodyNominalTy {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, def, args);
    }
}

impl MemorySize for BodyGenericArg {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Type(ty) => ty.record_memory_children(recorder),
            Self::Lifetime(text) | Self::Const(text) | Self::Unsupported(text) => {
                text.record_memory_children(recorder);
            }
            Self::AssocType { name, ty } => {
                recorder.scope("name", |recorder| name.record_memory_children(recorder));
                recorder.scope("ty", |recorder| ty.record_memory_children(recorder));
            }
        }
    }
}

impl MemorySize for BodyItemData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            source,
            name_source,
            scope,
            kind,
            name,
            docs,
            generics,
            fields,
        );
    }
}

impl MemorySize for BodyImplData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder, self, source, scope, generics, trait_ref, self_ty, self_item, functions,
        );
    }
}

impl MemorySize for BodyFunctionData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            source,
            name_source,
            owner,
            name,
            docs,
            declaration,
        );
    }
}

impl MemorySize for BodyFunctionOwner {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::LocalImpl(impl_id) => impl_id.record_memory_children(recorder),
        }
    }
}

impl MemorySize for BodyItemKind {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for PatData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, source, kind);
    }
}

impl MemorySize for PatKind {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Binding { binding, subpat } => {
                recorder.scope("binding", |recorder| {
                    binding.record_memory_children(recorder)
                });
                recorder.scope("subpat", |recorder| subpat.record_memory_children(recorder));
            }
            Self::Tuple { fields } | Self::Or { pats: fields } | Self::Slice { fields } => {
                fields.record_memory_children(recorder);
            }
            Self::TupleStruct { path, fields } => {
                recorder.scope("path", |recorder| path.record_memory_children(recorder));
                recorder.scope("fields", |recorder| fields.record_memory_children(recorder));
            }
            Self::Record { path, fields } => {
                recorder.scope("path", |recorder| path.record_memory_children(recorder));
                recorder.scope("fields", |recorder| fields.record_memory_children(recorder));
            }
            Self::Ref { pat } | Self::Box { pat } => pat.record_memory_children(recorder),
            Self::Path { path } => path.record_memory_children(recorder),
            Self::Wildcard | Self::Unsupported => {}
        }
    }
}

impl MemorySize for RecordPatField {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, key, pat);
    }
}

impl MemorySize for BindingData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, source, scope, kind, name, annotation, ty);
    }
}

impl MemorySize for BindingKind {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for StmtData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, source, kind);
    }
}

impl MemorySize for StmtKind {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Let {
                scope,
                pat,
                bindings,
                annotation,
                initializer,
            } => {
                recorder.scope("scope", |recorder| scope.record_memory_children(recorder));
                recorder.scope("pat", |recorder| pat.record_memory_children(recorder));
                recorder.scope("bindings", |recorder| {
                    bindings.record_memory_children(recorder);
                });
                recorder.scope("annotation", |recorder| {
                    annotation.record_memory_children(recorder);
                });
                recorder.scope("initializer", |recorder| {
                    initializer.record_memory_children(recorder);
                });
            }
            Self::Expr {
                expr,
                has_semicolon,
            } => {
                recorder.scope("expr", |recorder| expr.record_memory_children(recorder));
                recorder.scope("has_semicolon", |recorder| {
                    has_semicolon.record_memory_children(recorder);
                });
            }
            Self::Item { item } => item.record_memory_children(recorder),
            Self::Impl { impl_id } => impl_id.record_memory_children(recorder),
            Self::ItemIgnored => {}
        }
    }
}

impl_leaf_memory_size!(
    BodyId,
    BodyItemId,
    BodyImplId,
    BodyFunctionId,
    ExprId,
    PatId,
    StmtId,
    BindingId,
    ScopeId,
);

impl MemorySize for BodyRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, body);
    }
}

impl MemorySize for BodyItemRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, body, item);
    }
}

impl MemorySize for BodyFieldRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, item, index);
    }
}

impl MemorySize for BodyFunctionRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, body, function);
    }
}

impl MemorySize for BodyIrStats {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            target_count,
            built_target_count,
            skipped_target_count,
            body_count,
            scope_count,
            local_item_count,
            local_impl_count,
            local_function_count,
            binding_count,
            statement_count,
            expression_count,
        );
    }
}
