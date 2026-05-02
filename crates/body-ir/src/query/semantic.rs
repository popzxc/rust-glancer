//! Semantic IR read surface required by Body IR resolution.

use rg_def_map::{DefId, LocalDefRef, ModuleRef, Path};
use rg_semantic_ir::{
    EnumVariantRef, FieldData, FieldRef, FunctionData, FunctionRef, ImplData, ImplRef,
    SemanticIrDb, SemanticIrReadTxn, SemanticTypePathResolution, TraitImplRef, TraitRef,
    TypeDefRef, TypePathContext,
};

use super::DefMapQuery;

/// Minimal Semantic IR query surface used by body-resolution reads.
pub(crate) trait SemanticIrQuery {
    fn function_data(&self, function_ref: FunctionRef) -> Option<&FunctionData>;

    fn impl_data(&self, impl_ref: ImplRef) -> Option<&ImplData>;

    fn type_def_for_local_def(&self, def: LocalDefRef) -> Option<TypeDefRef>;

    fn trait_for_local_def(&self, def: LocalDefRef) -> Option<TraitRef>;

    fn type_def_name(&self, ty: TypeDefRef) -> Option<&str>;

    fn field_data(&self, field_ref: FieldRef) -> Option<FieldData<'_>>;

    fn enum_variant_ref_for_type_def(
        &self,
        ty: TypeDefRef,
        variant_name: &str,
    ) -> Option<EnumVariantRef>;

    fn inherent_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef>;

    fn trait_impls_for_type(&self, ty: TypeDefRef) -> Vec<TraitImplRef>;

    fn trait_functions(&self, trait_ref: TraitRef) -> Vec<FunctionRef>;

    fn type_path_context_for_function(&self, function_ref: FunctionRef) -> Option<TypePathContext>;

    fn resolve_type_path(
        &self,
        def_map: &impl DefMapQuery,
        context: TypePathContext,
        path: &Path,
    ) -> SemanticTypePathResolution {
        if path.is_self_type() {
            let Some(impl_ref) = context.impl_ref else {
                return SemanticTypePathResolution::Unknown;
            };
            let types = self
                .impl_data(impl_ref)
                .map(|data| data.resolved_self_tys.clone())
                .unwrap_or_default();
            return if types.is_empty() {
                SemanticTypePathResolution::Unknown
            } else {
                SemanticTypePathResolution::SelfType(types)
            };
        }

        let type_defs =
            resolve_semantic_items_for_path(self, def_map, context.module, path, |db, def| {
                let DefId::Local(local_def) = def else {
                    return None;
                };

                db.type_def_for_local_def(local_def)
            });
        if !type_defs.is_empty() {
            return SemanticTypePathResolution::TypeDefs(type_defs);
        }

        let traits =
            resolve_semantic_items_for_path(self, def_map, context.module, path, |db, def| {
                let DefId::Local(local_def) = def else {
                    return None;
                };

                db.trait_for_local_def(local_def)
            });
        if traits.is_empty() {
            SemanticTypePathResolution::Unknown
        } else {
            SemanticTypePathResolution::Traits(traits)
        }
    }
}

impl SemanticIrQuery for SemanticIrDb {
    fn function_data(&self, function_ref: FunctionRef) -> Option<&FunctionData> {
        SemanticIrDb::function_data(self, function_ref)
    }

    fn impl_data(&self, impl_ref: ImplRef) -> Option<&ImplData> {
        SemanticIrDb::impl_data(self, impl_ref)
    }

    fn type_def_for_local_def(&self, def: LocalDefRef) -> Option<TypeDefRef> {
        SemanticIrDb::type_def_for_local_def(self, def)
    }

    fn trait_for_local_def(&self, def: LocalDefRef) -> Option<TraitRef> {
        SemanticIrDb::trait_for_local_def(self, def)
    }

    fn type_def_name(&self, ty: TypeDefRef) -> Option<&str> {
        SemanticIrDb::type_def_name(self, ty)
    }

    fn field_data(&self, field_ref: FieldRef) -> Option<FieldData<'_>> {
        SemanticIrDb::field_data(self, field_ref)
    }

    fn enum_variant_ref_for_type_def(
        &self,
        ty: TypeDefRef,
        variant_name: &str,
    ) -> Option<EnumVariantRef> {
        SemanticIrDb::enum_variant_ref_for_type_def(self, ty, variant_name)
    }

    fn inherent_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef> {
        SemanticIrDb::inherent_functions_for_type(self, ty)
    }

    fn trait_impls_for_type(&self, ty: TypeDefRef) -> Vec<TraitImplRef> {
        SemanticIrDb::trait_impls_for_type(self, ty)
    }

    fn trait_functions(&self, trait_ref: TraitRef) -> Vec<FunctionRef> {
        SemanticIrDb::trait_functions(self, trait_ref)
    }

    fn type_path_context_for_function(&self, function_ref: FunctionRef) -> Option<TypePathContext> {
        SemanticIrDb::type_path_context_for_function(self, function_ref)
    }
}

impl SemanticIrQuery for SemanticIrReadTxn<'_> {
    fn function_data(&self, function_ref: FunctionRef) -> Option<&FunctionData> {
        SemanticIrReadTxn::function_data(self, function_ref)
    }

    fn impl_data(&self, impl_ref: ImplRef) -> Option<&ImplData> {
        SemanticIrReadTxn::impl_data(self, impl_ref)
    }

    fn type_def_for_local_def(&self, def: LocalDefRef) -> Option<TypeDefRef> {
        SemanticIrReadTxn::type_def_for_local_def(self, def)
    }

    fn trait_for_local_def(&self, def: LocalDefRef) -> Option<TraitRef> {
        SemanticIrReadTxn::trait_for_local_def(self, def)
    }

    fn type_def_name(&self, ty: TypeDefRef) -> Option<&str> {
        SemanticIrReadTxn::type_def_name(self, ty)
    }

    fn field_data(&self, field_ref: FieldRef) -> Option<FieldData<'_>> {
        SemanticIrReadTxn::field_data(self, field_ref)
    }

    fn enum_variant_ref_for_type_def(
        &self,
        ty: TypeDefRef,
        variant_name: &str,
    ) -> Option<EnumVariantRef> {
        SemanticIrReadTxn::enum_variant_ref_for_type_def(self, ty, variant_name)
    }

    fn inherent_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef> {
        SemanticIrReadTxn::inherent_functions_for_type(self, ty)
    }

    fn trait_impls_for_type(&self, ty: TypeDefRef) -> Vec<TraitImplRef> {
        SemanticIrReadTxn::trait_impls_for_type(self, ty)
    }

    fn trait_functions(&self, trait_ref: TraitRef) -> Vec<FunctionRef> {
        SemanticIrReadTxn::trait_functions(self, trait_ref)
    }

    fn type_path_context_for_function(&self, function_ref: FunctionRef) -> Option<TypePathContext> {
        SemanticIrReadTxn::type_path_context_for_function(self, function_ref)
    }
}

fn resolve_semantic_items_for_path<S, D, T>(
    semantic_ir: &S,
    def_map: &D,
    owner: ModuleRef,
    path: &Path,
    map_def: impl Fn(&S, DefId) -> Option<T>,
) -> Vec<T>
where
    S: SemanticIrQuery + ?Sized,
    D: DefMapQuery + ?Sized,
    T: PartialEq,
{
    let mut resolved_items = Vec::new();
    for def in def_map.resolve_path_in_type_namespace(owner, path).resolved {
        let Some(item) = map_def(semantic_ir, def) else {
            continue;
        };
        push_unique(&mut resolved_items, item);
    }

    resolved_items
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}
