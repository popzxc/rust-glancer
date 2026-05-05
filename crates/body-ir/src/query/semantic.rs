//! Semantic IR read surface required by Body IR resolution.

use rg_def_map::{DefId, LocalDefRef, ModuleRef, Path};
use rg_package_store::PackageStoreError;
use rg_semantic_ir::{
    AssocItemId, EnumVariantRef, FieldData, FieldRef, FunctionData, FunctionRef, ImplData, ImplRef,
    SemanticIrDb, SemanticIrReadTxn, SemanticTypePathResolution, TraitImplRef, TraitRef,
    TypeDefRef, TypePathContext,
};

use super::DefMapQuery;

/// Minimal Semantic IR query surface used by body-resolution reads.
pub(crate) trait SemanticIrQuery {
    fn function_data(
        &self,
        function_ref: FunctionRef,
    ) -> Result<Option<&FunctionData>, PackageStoreError>;

    fn impl_data(&self, impl_ref: ImplRef) -> Result<Option<&ImplData>, PackageStoreError>;

    fn type_def_for_local_def(
        &self,
        def: LocalDefRef,
    ) -> Result<Option<TypeDefRef>, PackageStoreError>;

    fn trait_for_local_def(&self, def: LocalDefRef) -> Result<Option<TraitRef>, PackageStoreError>;

    fn type_def_name(&self, ty: TypeDefRef) -> Result<Option<&str>, PackageStoreError>;

    fn generic_params_for_type_def(
        &self,
        ty: TypeDefRef,
    ) -> Result<Option<&rg_item_tree::GenericParams>, PackageStoreError>;

    fn function_for_local_def(
        &self,
        def: LocalDefRef,
    ) -> Result<Option<FunctionRef>, PackageStoreError>;

    fn field_for_type(
        &self,
        ty: TypeDefRef,
        key: &rg_item_tree::FieldKey,
    ) -> Result<Option<FieldRef>, PackageStoreError>;

    fn field_data(&self, field_ref: FieldRef) -> Result<Option<FieldData<'_>>, PackageStoreError>;

    fn enum_variant_ref_for_type_def(
        &self,
        ty: TypeDefRef,
        variant_name: &str,
    ) -> Result<Option<EnumVariantRef>, PackageStoreError>;

    fn inherent_functions_for_type(
        &self,
        ty: TypeDefRef,
    ) -> Result<Vec<FunctionRef>, PackageStoreError>;

    fn trait_impls_for_type(&self, ty: TypeDefRef) -> Result<Vec<TraitImplRef>, PackageStoreError>;

    fn trait_functions(&self, trait_ref: TraitRef) -> Result<Vec<FunctionRef>, PackageStoreError>;

    fn enum_data_for_type_def(
        &self,
        ty: TypeDefRef,
    ) -> Result<Option<&rg_semantic_ir::EnumData>, PackageStoreError>;

    fn enum_variant_for_type_def(
        &self,
        ty: TypeDefRef,
        variant_name: &str,
    ) -> Result<Option<(usize, &rg_item_tree::EnumVariantItem)>, PackageStoreError>;

    fn type_path_context_for_function(
        &self,
        function_ref: FunctionRef,
    ) -> Result<Option<TypePathContext>, PackageStoreError>;

    fn resolve_type_path(
        &self,
        def_map: &impl DefMapQuery,
        context: TypePathContext,
        path: &Path,
    ) -> Result<SemanticTypePathResolution, PackageStoreError> {
        if path.is_self_type() {
            let Some(impl_ref) = context.impl_ref else {
                return Ok(SemanticTypePathResolution::Unknown);
            };
            let types = self
                .impl_data(impl_ref)?
                .map(|data| data.resolved_self_tys.clone())
                .unwrap_or_default();
            return Ok(if types.is_empty() {
                SemanticTypePathResolution::Unknown
            } else {
                SemanticTypePathResolution::SelfType(types)
            });
        }

        let type_defs =
            resolve_semantic_items_for_path(self, def_map, context.module, path, |db, def| {
                let DefId::Local(local_def) = def else {
                    return Ok(None);
                };

                db.type_def_for_local_def(local_def)
            })?;
        if !type_defs.is_empty() {
            return Ok(SemanticTypePathResolution::TypeDefs(type_defs));
        }

        let traits =
            resolve_semantic_items_for_path(self, def_map, context.module, path, |db, def| {
                let DefId::Local(local_def) = def else {
                    return Ok(None);
                };

                db.trait_for_local_def(local_def)
            })?;
        Ok(if traits.is_empty() {
            SemanticTypePathResolution::Unknown
        } else {
            SemanticTypePathResolution::Traits(traits)
        })
    }
}

impl SemanticIrQuery for SemanticIrDb {
    fn function_data(
        &self,
        function_ref: FunctionRef,
    ) -> Result<Option<&FunctionData>, PackageStoreError> {
        Ok(SemanticIrDb::function_data(self, function_ref))
    }

    fn impl_data(&self, impl_ref: ImplRef) -> Result<Option<&ImplData>, PackageStoreError> {
        Ok(SemanticIrDb::impl_data(self, impl_ref))
    }

    fn type_def_for_local_def(
        &self,
        def: LocalDefRef,
    ) -> Result<Option<TypeDefRef>, PackageStoreError> {
        Ok(SemanticIrDb::type_def_for_local_def(self, def))
    }

    fn trait_for_local_def(&self, def: LocalDefRef) -> Result<Option<TraitRef>, PackageStoreError> {
        Ok(SemanticIrDb::trait_for_local_def(self, def))
    }

    fn type_def_name(&self, ty: TypeDefRef) -> Result<Option<&str>, PackageStoreError> {
        Ok(SemanticIrDb::type_def_name(self, ty))
    }

    fn generic_params_for_type_def(
        &self,
        ty: TypeDefRef,
    ) -> Result<Option<&rg_item_tree::GenericParams>, PackageStoreError> {
        Ok(SemanticIrDb::generic_params_for_type_def(self, ty))
    }

    fn function_for_local_def(
        &self,
        def: LocalDefRef,
    ) -> Result<Option<FunctionRef>, PackageStoreError> {
        Ok(SemanticIrDb::function_for_local_def(self, def))
    }

    fn field_for_type(
        &self,
        ty: TypeDefRef,
        key: &rg_item_tree::FieldKey,
    ) -> Result<Option<FieldRef>, PackageStoreError> {
        Ok(SemanticIrDb::field_for_type(self, ty, key))
    }

    fn field_data(&self, field_ref: FieldRef) -> Result<Option<FieldData<'_>>, PackageStoreError> {
        Ok(SemanticIrDb::field_data(self, field_ref))
    }

    fn enum_variant_ref_for_type_def(
        &self,
        ty: TypeDefRef,
        variant_name: &str,
    ) -> Result<Option<EnumVariantRef>, PackageStoreError> {
        Ok(SemanticIrDb::enum_variant_ref_for_type_def(
            self,
            ty,
            variant_name,
        ))
    }

    fn inherent_functions_for_type(
        &self,
        ty: TypeDefRef,
    ) -> Result<Vec<FunctionRef>, PackageStoreError> {
        Ok(SemanticIrDb::inherent_functions_for_type(self, ty))
    }

    fn trait_impls_for_type(&self, ty: TypeDefRef) -> Result<Vec<TraitImplRef>, PackageStoreError> {
        Ok(SemanticIrDb::trait_impls_for_type(self, ty))
    }

    fn trait_functions(&self, trait_ref: TraitRef) -> Result<Vec<FunctionRef>, PackageStoreError> {
        Ok(SemanticIrDb::trait_functions(self, trait_ref))
    }

    fn enum_data_for_type_def(
        &self,
        ty: TypeDefRef,
    ) -> Result<Option<&rg_semantic_ir::EnumData>, PackageStoreError> {
        Ok(SemanticIrDb::enum_data_for_type_def(self, ty))
    }

    fn enum_variant_for_type_def(
        &self,
        ty: TypeDefRef,
        variant_name: &str,
    ) -> Result<Option<(usize, &rg_item_tree::EnumVariantItem)>, PackageStoreError> {
        Ok(SemanticIrDb::enum_variant_for_type_def(
            self,
            ty,
            variant_name,
        ))
    }

    fn type_path_context_for_function(
        &self,
        function_ref: FunctionRef,
    ) -> Result<Option<TypePathContext>, PackageStoreError> {
        Ok(SemanticIrDb::type_path_context_for_function(
            self,
            function_ref,
        ))
    }
}

impl SemanticIrQuery for SemanticIrReadTxn<'_> {
    fn function_data(
        &self,
        function_ref: FunctionRef,
    ) -> Result<Option<&FunctionData>, PackageStoreError> {
        SemanticIrReadTxn::function_data(self, function_ref)
    }

    fn impl_data(&self, impl_ref: ImplRef) -> Result<Option<&ImplData>, PackageStoreError> {
        SemanticIrReadTxn::impl_data(self, impl_ref)
    }

    fn type_def_for_local_def(
        &self,
        def: LocalDefRef,
    ) -> Result<Option<TypeDefRef>, PackageStoreError> {
        SemanticIrReadTxn::type_def_for_local_def(self, def)
    }

    fn trait_for_local_def(&self, def: LocalDefRef) -> Result<Option<TraitRef>, PackageStoreError> {
        SemanticIrReadTxn::trait_for_local_def(self, def)
    }

    fn type_def_name(&self, ty: TypeDefRef) -> Result<Option<&str>, PackageStoreError> {
        SemanticIrReadTxn::type_def_name(self, ty)
    }

    fn generic_params_for_type_def(
        &self,
        ty: TypeDefRef,
    ) -> Result<Option<&rg_item_tree::GenericParams>, PackageStoreError> {
        SemanticIrReadTxn::generic_params_for_type_def(self, ty)
    }

    fn function_for_local_def(
        &self,
        def: LocalDefRef,
    ) -> Result<Option<FunctionRef>, PackageStoreError> {
        SemanticIrReadTxn::function_for_local_def(self, def)
    }

    fn field_for_type(
        &self,
        ty: TypeDefRef,
        key: &rg_item_tree::FieldKey,
    ) -> Result<Option<FieldRef>, PackageStoreError> {
        SemanticIrReadTxn::field_for_type(self, ty, key)
    }

    fn field_data(&self, field_ref: FieldRef) -> Result<Option<FieldData<'_>>, PackageStoreError> {
        SemanticIrReadTxn::field_data(self, field_ref)
    }

    fn enum_variant_ref_for_type_def(
        &self,
        ty: TypeDefRef,
        variant_name: &str,
    ) -> Result<Option<EnumVariantRef>, PackageStoreError> {
        SemanticIrReadTxn::enum_variant_ref_for_type_def(self, ty, variant_name)
    }

    fn inherent_functions_for_type(
        &self,
        ty: TypeDefRef,
    ) -> Result<Vec<FunctionRef>, PackageStoreError> {
        let mut functions = Vec::new();

        for impl_ref in txn_inherent_impls_for_type(self, ty)? {
            let Some(data) = SemanticIrReadTxn::impl_data(self, impl_ref)? else {
                continue;
            };

            for item in &data.items {
                if let AssocItemId::Function(id) = item {
                    push_unique(
                        &mut functions,
                        FunctionRef {
                            target: impl_ref.target,
                            id: *id,
                        },
                    );
                }
            }
        }

        Ok(functions)
    }

    fn trait_impls_for_type(&self, ty: TypeDefRef) -> Result<Vec<TraitImplRef>, PackageStoreError> {
        let mut trait_impls = Vec::new();

        for impl_ref in txn_impls_for_type(self, ty)? {
            let Some(data) = SemanticIrReadTxn::impl_data(self, impl_ref)? else {
                continue;
            };

            for trait_ref in &data.resolved_trait_refs {
                push_unique(
                    &mut trait_impls,
                    TraitImplRef {
                        impl_ref,
                        trait_ref: *trait_ref,
                    },
                );
            }
        }

        Ok(trait_impls)
    }

    fn trait_functions(&self, trait_ref: TraitRef) -> Result<Vec<FunctionRef>, PackageStoreError> {
        let mut functions = Vec::new();
        let Some(data) = SemanticIrReadTxn::trait_data(self, trait_ref)? else {
            return Ok(functions);
        };

        for item in &data.items {
            if let AssocItemId::Function(id) = item {
                push_unique(
                    &mut functions,
                    FunctionRef {
                        target: trait_ref.target,
                        id: *id,
                    },
                );
            }
        }

        Ok(functions)
    }

    fn enum_data_for_type_def(
        &self,
        ty: TypeDefRef,
    ) -> Result<Option<&rg_semantic_ir::EnumData>, PackageStoreError> {
        SemanticIrReadTxn::enum_data_for_type_def(self, ty)
    }

    fn enum_variant_for_type_def(
        &self,
        ty: TypeDefRef,
        variant_name: &str,
    ) -> Result<Option<(usize, &rg_item_tree::EnumVariantItem)>, PackageStoreError> {
        SemanticIrReadTxn::enum_variant_for_type_def(self, ty, variant_name)
    }

    fn type_path_context_for_function(
        &self,
        function_ref: FunctionRef,
    ) -> Result<Option<TypePathContext>, PackageStoreError> {
        let Some(function_data) = SemanticIrReadTxn::function_data(self, function_ref)? else {
            return Ok(None);
        };
        match function_data.owner {
            rg_semantic_ir::ItemOwner::Module(module_ref) => {
                Ok(Some(TypePathContext::module(module_ref)))
            }
            rg_semantic_ir::ItemOwner::Trait(id) => {
                let trait_ref = TraitRef {
                    target: function_ref.target,
                    id,
                };
                Ok(SemanticIrReadTxn::trait_data(self, trait_ref)?
                    .map(|data| TypePathContext::module(data.owner)))
            }
            rg_semantic_ir::ItemOwner::Impl(id) => {
                let impl_ref = ImplRef {
                    target: function_ref.target,
                    id,
                };
                Ok(
                    SemanticIrReadTxn::impl_data(self, impl_ref)?.map(|data| TypePathContext {
                        module: data.owner,
                        impl_ref: Some(impl_ref),
                    }),
                )
            }
        }
    }
}

fn resolve_semantic_items_for_path<S, D, T>(
    semantic_ir: &S,
    def_map: &D,
    owner: ModuleRef,
    path: &Path,
    map_def: impl Fn(&S, DefId) -> Result<Option<T>, PackageStoreError>,
) -> Result<Vec<T>, PackageStoreError>
where
    S: SemanticIrQuery + ?Sized,
    D: DefMapQuery + ?Sized,
    T: PartialEq,
{
    let mut resolved_items = Vec::new();
    for def in def_map
        .resolve_path_in_type_namespace(owner, path)?
        .resolved
    {
        let Some(item) = map_def(semantic_ir, def)? else {
            continue;
        };
        push_unique(&mut resolved_items, item);
    }

    Ok(resolved_items)
}

fn txn_impls_for_type(
    db: &SemanticIrReadTxn<'_>,
    ty: TypeDefRef,
) -> Result<Vec<ImplRef>, PackageStoreError> {
    let mut impls = Vec::new();

    for (target, _) in db.materialize_included_target_irs()? {
        for (impl_ref, _) in db.impls(target)? {
            let Some(data) = SemanticIrReadTxn::impl_data(db, impl_ref)? else {
                continue;
            };
            if data.resolved_self_tys.contains(&ty) {
                impls.push(impl_ref);
            }
        }
    }

    Ok(impls)
}

fn txn_inherent_impls_for_type(
    db: &SemanticIrReadTxn<'_>,
    ty: TypeDefRef,
) -> Result<Vec<ImplRef>, PackageStoreError> {
    let mut impls = Vec::new();

    for impl_ref in txn_impls_for_type(db, ty)? {
        let Some(data) = SemanticIrReadTxn::impl_data(db, impl_ref)? else {
            continue;
        };
        if data.trait_ref.is_none() {
            impls.push(impl_ref);
        }
    }

    Ok(impls)
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}
