//! Resident Body IR database and body-query helpers.

use std::sync::Arc;

use anyhow::Context as _;

use rg_def_map::{DefMapDb, PackageSlot, Path, TargetRef};
use rg_package_store::PackageStore;
use rg_semantic_ir::{FieldRef, FunctionRef, SemanticIrDb, TraitApplicability};
use rg_text::NameInterner;

use crate::{
    BodyData, BodyFieldData, BodyFieldRef, BodyFunctionData, BodyFunctionRef, BodyIrBuildPolicy,
    BodyIrStats, BodyItemRef, BodyLocalNominalTy, BodyNominalTy, BodyRef, BodyResolution, BodyTy,
    BodyTypePathResolution, PackageBodies, ScopeId, TargetBodies, TargetBodiesStatus, lower,
    resolution,
};

/// Body-level IR for all analyzed packages and targets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BodyIrDb {
    pub(crate) packages: PackageStore<PackageBodies>,
}

impl BodyIrDb {
    /// Builds Body IR using the default editor-oriented policy.
    ///
    /// By default we lower bodies only for workspace packages. Dependency signatures remain
    /// available through Semantic IR, but dependency body internals are skipped to keep the eager
    /// analysis cheaper.
    pub fn build(
        parse: &rg_parse::ParseDb,
        def_map: &rg_def_map::DefMapDb,
        semantic_ir: &rg_semantic_ir::SemanticIrDb,
    ) -> anyhow::Result<Self> {
        let mut interner = NameInterner::new();
        Self::build_with_policy_and_interner(
            parse,
            def_map,
            semantic_ir,
            BodyIrBuildPolicy::default(),
            &mut interner,
        )
    }

    /// Builds Body IR using an explicit package selection policy.
    pub fn build_with_policy(
        parse: &rg_parse::ParseDb,
        def_map: &rg_def_map::DefMapDb,
        semantic_ir: &rg_semantic_ir::SemanticIrDb,
        policy: BodyIrBuildPolicy,
    ) -> anyhow::Result<Self> {
        let mut interner = NameInterner::new();
        Self::build_with_policy_and_interner(parse, def_map, semantic_ir, policy, &mut interner)
    }

    /// Builds Body IR using an explicit package selection policy and retained name interner.
    pub fn build_with_policy_and_interner(
        parse: &rg_parse::ParseDb,
        def_map: &rg_def_map::DefMapDb,
        semantic_ir: &rg_semantic_ir::SemanticIrDb,
        policy: BodyIrBuildPolicy,
        interner: &mut NameInterner,
    ) -> anyhow::Result<Self> {
        let mut db = lower::build_db(parse, semantic_ir, policy, interner)?;
        resolution::resolve_bodies(&mut db, def_map, semantic_ir);
        db.shrink_to_fit();
        Ok(db)
    }

    /// Returns a new Body IR snapshot with selected packages rebuilt.
    pub fn rebuild_packages(
        &self,
        parse: &rg_parse::ParseDb,
        def_map: &rg_def_map::DefMapDb,
        semantic_ir: &rg_semantic_ir::SemanticIrDb,
        policy: BodyIrBuildPolicy,
        packages: &[PackageSlot],
    ) -> anyhow::Result<Self> {
        let mut interner = NameInterner::new();
        self.rebuild_packages_with_interner(
            parse,
            def_map,
            semantic_ir,
            policy,
            packages,
            &mut interner,
        )
    }

    /// Returns a new Body IR snapshot with selected packages rebuilt using retained names.
    pub fn rebuild_packages_with_interner(
        &self,
        parse: &rg_parse::ParseDb,
        def_map: &rg_def_map::DefMapDb,
        semantic_ir: &rg_semantic_ir::SemanticIrDb,
        policy: BodyIrBuildPolicy,
        packages: &[PackageSlot],
        interner: &mut NameInterner,
    ) -> anyhow::Result<Self> {
        let mut next = self.clone();
        let packages = normalized_package_slots(packages);

        for package in &packages {
            let target_count = semantic_ir
                .package(*package)
                .map(|package| package.targets().len())
                .with_context(|| {
                    format!(
                        "while attempting to fetch semantic IR package {}",
                        package.0
                    )
                })?;
            let rebuilt =
                lower::build_package(parse, semantic_ir, policy, *package, target_count, interner)?;
            next.packages.replace(*package, rebuilt).with_context(|| {
                format!("while attempting to replace body IR package {}", package.0)
            })?;
        }

        resolution::resolve_bodies_for_packages(&mut next, def_map, semantic_ir, &packages);
        next.shrink_packages(&packages);
        Ok(next)
    }

    pub(crate) fn new(packages: Vec<PackageBodies>) -> Self {
        Self {
            packages: PackageStore::from_vec(packages),
        }
    }

    fn shrink_to_fit(&mut self) {
        self.packages.shrink_to_fit();
        for package in self.packages.resident_packages_unique_mut() {
            package.shrink_to_fit();
        }
    }

    fn shrink_packages(&mut self, packages: &[PackageSlot]) {
        for package in packages {
            if let Some(package) = self.packages.get_unique_mut(*package) {
                package.shrink_to_fit();
            }
        }
    }

    pub fn stats(&self) -> BodyIrStats {
        let mut stats = BodyIrStats::default();

        for package in self.packages.resident_packages() {
            for target in package.targets() {
                stats.target_count += 1;
                match target.status() {
                    TargetBodiesStatus::Built => stats.built_target_count += 1,
                    TargetBodiesStatus::Skipped => stats.skipped_target_count += 1,
                }
                stats.body_count += target.bodies().len();
                for body in target.bodies() {
                    stats.scope_count += body.scopes.len();
                    stats.local_item_count += body.local_items.len();
                    stats.local_impl_count += body.local_impls.len();
                    stats.local_function_count += body.local_functions.len();
                    stats.binding_count += body.bindings.len();
                    stats.statement_count += body.statements.len();
                    stats.expression_count += body.exprs.len();
                }
            }
        }

        stats
    }

    /// Returns resident package-level body IR sets, skipping offloaded packages.
    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    /// Returns one package by package slot.
    pub fn package(&self, package: PackageSlot) -> Option<&PackageBodies> {
        self.packages.get(package)
    }

    pub fn package_arc(&self, package: PackageSlot) -> Option<Arc<PackageBodies>> {
        self.packages.get_arc(package)
    }

    pub fn replace_package(&mut self, package: PackageSlot, bodies: PackageBodies) -> Option<()> {
        self.packages.replace(package, bodies)
    }

    pub fn offload_package(&mut self, package: PackageSlot) -> Option<()> {
        self.packages.offload(package)
    }

    /// Returns one target body IR by project-wide target reference.
    pub fn target_bodies(&self, target: TargetRef) -> Option<&TargetBodies> {
        self.package(target.package)?.target(target.target)
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
    pub fn body_data(&self, body_ref: BodyRef) -> Option<&BodyData> {
        self.target_bodies(body_ref.target)?.body(body_ref.body)
    }

    pub fn resolve_type_path_in_scope(
        &self,
        def_map: &DefMapDb,
        semantic_ir: &SemanticIrDb,
        body_ref: BodyRef,
        scope: ScopeId,
        path: &Path,
    ) -> BodyTypePathResolution {
        resolution::resolve_type_path_in_scope(self, def_map, semantic_ir, body_ref, scope, path)
    }

    pub fn resolve_value_path_in_scope(
        &self,
        def_map: &DefMapDb,
        semantic_ir: &SemanticIrDb,
        body_ref: BodyRef,
        scope: ScopeId,
        path: &Path,
    ) -> (BodyResolution, BodyTy) {
        // This is intentionally exposed as a query, not just a build-time helper: analysis uses it
        // to resolve the exact qualified-path prefix selected by the cursor.
        resolution::resolve_value_path_in_scope(self, def_map, semantic_ir, body_ref, scope, path)
    }

    pub fn ty_for_field(
        &self,
        def_map: &DefMapDb,
        semantic_ir: &SemanticIrDb,
        field_ref: FieldRef,
    ) -> Option<BodyTy> {
        resolution::ty_for_field(def_map, semantic_ir, field_ref)
    }

    pub fn semantic_function_applies_to_receiver(
        &self,
        def_map: &DefMapDb,
        semantic_ir: &SemanticIrDb,
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

    pub fn semantic_trait_function_candidates_for_receiver(
        &self,
        def_map: &DefMapDb,
        semantic_ir: &SemanticIrDb,
        receiver_ty: &BodyNominalTy,
    ) -> Vec<(FunctionRef, TraitApplicability)> {
        resolution::semantic_trait_function_candidates_for_receiver(
            def_map,
            semantic_ir,
            receiver_ty,
        )
    }

    pub fn local_function_applies_to_receiver(
        &self,
        def_map: &DefMapDb,
        semantic_ir: &SemanticIrDb,
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

    pub fn local_field_data(&self, field_ref: BodyFieldRef) -> Option<BodyFieldData<'_>> {
        let body = self.body_data(field_ref.item.body)?;
        let item = body.local_item(field_ref.item.item)?;
        let field = item.field(field_ref.index)?;

        Some(BodyFieldData { item, field })
    }

    pub fn inherent_functions_for_local_type(&self, item_ref: BodyItemRef) -> Vec<BodyFunctionRef> {
        let Some(body) = self.body_data(item_ref.body) else {
            return Vec::new();
        };

        body.inherent_functions_for_local_type(item_ref.body, item_ref)
    }

    pub fn local_function_data(&self, function_ref: BodyFunctionRef) -> Option<&BodyFunctionData> {
        self.body_data(function_ref.body)?
            .local_function(function_ref.function)
    }

    pub(crate) fn package_mut(&mut self, package: PackageSlot) -> Option<&mut PackageBodies> {
        self.packages.make_mut(package)
    }
}

fn normalized_package_slots(packages: &[PackageSlot]) -> Vec<PackageSlot> {
    let mut slots = packages.to_vec();
    slots.sort_by_key(|slot| slot.0);
    slots.dedup();
    slots
}
