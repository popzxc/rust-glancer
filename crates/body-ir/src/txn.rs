//! Read transactions over frozen Body IR package data.

use rg_def_map::{DefMapReadTxn, PackageSlot, Path, TargetRef};
use rg_package_store::{PackageRead, PackageStoreReadTxn};
use rg_semantic_ir::{FieldRef, FunctionRef, SemanticIrReadTxn, TraitApplicability};

use crate::{
    BodyData, BodyFieldData, BodyFieldRef, BodyFunctionData, BodyFunctionRef, BodyItemRef,
    BodyLocalNominalTy, BodyNominalTy, BodyRef, BodyResolution, BodyTy, BodyTypePathResolution,
    PackageBodies, ScopeId, TargetBodies, resolution,
};

/// Read-only Body IR access for one query transaction.
#[derive(Debug, Clone)]
pub struct BodyIrReadTxn<'db> {
    packages: PackageStoreReadTxn<'db, PackageBodies>,
}

impl<'db> BodyIrReadTxn<'db> {
    pub(crate) fn new(packages: PackageStoreReadTxn<'db, PackageBodies>) -> Self {
        Self { packages }
    }

    pub fn packages(&self) -> impl ExactSizeIterator<Item = PackageRead<'db, PackageBodies>> + '_ {
        self.packages.iter()
    }

    pub fn package(&self, package: PackageSlot) -> Option<PackageRead<'db, PackageBodies>> {
        self.packages.read(package)
    }

    pub fn target_bodies(&self, target: TargetRef) -> Option<&'db TargetBodies> {
        self.package(target.package)?
            .into_ref()
            .target(target.target)
    }

    /// Returns the body associated with a semantic function, if that function has a body.
    pub fn body_for_function(&self, function: FunctionRef) -> Option<BodyRef> {
        let body = self
            .target_bodies(function.target)?
            .body_for_function(function.id)?;
        Some(BodyRef {
            target: function.target,
            body,
        })
    }

    /// Returns one body by project-wide body reference.
    pub fn body_data(&self, body_ref: BodyRef) -> Option<&'db BodyData> {
        self.target_bodies(body_ref.target)?.body(body_ref.body)
    }

    /// Resolves a type path from a body-local lexical scope.
    ///
    /// This is a query-time counterpart to body lowering: local items in the body scope are checked
    /// before falling back to semantic type resolution.
    pub fn resolve_type_path_in_scope(
        &self,
        def_map: &DefMapReadTxn<'db>,
        semantic_ir: &SemanticIrReadTxn<'db>,
        body_ref: BodyRef,
        scope: ScopeId,
        path: &Path,
    ) -> BodyTypePathResolution {
        resolution::resolve_type_path_in_scope(self, def_map, semantic_ir, body_ref, scope, path)
    }

    /// Resolves a value path from a body-local lexical scope.
    ///
    /// Analysis uses this for cursor prefixes such as associated functions and enum variants,
    /// where a selected path segment can differ from the surrounding expression's final result.
    pub fn resolve_value_path_in_scope(
        &self,
        def_map: &DefMapReadTxn<'db>,
        semantic_ir: &SemanticIrReadTxn<'db>,
        body_ref: BodyRef,
        scope: ScopeId,
        path: &Path,
    ) -> (BodyResolution, BodyTy) {
        resolution::resolve_value_path_in_scope(self, def_map, semantic_ir, body_ref, scope, path)
    }

    /// Converts one Semantic IR field declaration type into Body IR's small type vocabulary.
    pub fn ty_for_field(
        &self,
        def_map: &DefMapReadTxn<'db>,
        semantic_ir: &SemanticIrReadTxn<'db>,
        field_ref: FieldRef,
    ) -> Option<BodyTy> {
        resolution::ty_for_field(def_map, semantic_ir, field_ref)
    }

    /// Checks whether a semantic function is a plausible method candidate for a receiver type.
    pub fn semantic_function_applies_to_receiver(
        &self,
        def_map: &DefMapReadTxn<'db>,
        semantic_ir: &SemanticIrReadTxn<'db>,
        function_ref: FunctionRef,
        receiver_ty: &BodyNominalTy,
    ) -> bool {
        resolution::semantic_function_applies_to_receiver(
            def_map,
            semantic_ir,
            function_ref,
            receiver_ty,
        )
    }

    /// Returns trait-associated function candidates for a semantic receiver type.
    pub fn semantic_trait_function_candidates_for_receiver(
        &self,
        def_map: &DefMapReadTxn<'db>,
        semantic_ir: &SemanticIrReadTxn<'db>,
        receiver_ty: &BodyNominalTy,
    ) -> Vec<(FunctionRef, TraitApplicability)> {
        resolution::semantic_trait_function_candidates_for_receiver(
            def_map,
            semantic_ir,
            receiver_ty,
        )
    }

    /// Checks whether a body-local function is a plausible method candidate for a receiver type.
    pub fn local_function_applies_to_receiver(
        &self,
        def_map: &DefMapReadTxn<'db>,
        semantic_ir: &SemanticIrReadTxn<'db>,
        function_ref: BodyFunctionRef,
        receiver_ty: &BodyLocalNominalTy,
    ) -> bool {
        resolution::local_function_applies_to_receiver(
            self,
            def_map,
            semantic_ir,
            function_ref,
            receiver_ty,
        )
    }

    /// Returns all body-local fields declared for a body-local type item.
    pub fn fields_for_local_type(&self, item_ref: BodyItemRef) -> Vec<BodyFieldRef> {
        let Some(body) = self.body_data(item_ref.body) else {
            return Vec::new();
        };
        let Some(item) = body.local_item(item_ref.item) else {
            return Vec::new();
        };

        (0..item.fields.fields().len())
            .map(|index| BodyFieldRef {
                item: item_ref,
                index,
            })
            .collect()
    }

    /// Returns declaration data for one body-local field.
    pub fn local_field_data(&self, field_ref: BodyFieldRef) -> Option<BodyFieldData<'db>> {
        let body = self.body_data(field_ref.item.body)?;
        let item = body.local_item(field_ref.item.item)?;
        let field = item.field(field_ref.index)?;

        Some(BodyFieldData { item, field })
    }

    /// Returns inherent body-local impl functions declared for a body-local type item.
    pub fn inherent_functions_for_local_type(&self, item_ref: BodyItemRef) -> Vec<BodyFunctionRef> {
        let Some(body) = self.body_data(item_ref.body) else {
            return Vec::new();
        };

        body.inherent_functions_for_local_type(item_ref.body, item_ref)
    }

    /// Returns declaration data for one body-local function.
    pub fn local_function_data(
        &self,
        function_ref: BodyFunctionRef,
    ) -> Option<&'db BodyFunctionData> {
        self.body_data(function_ref.body)?
            .local_function(function_ref.function)
    }
}
