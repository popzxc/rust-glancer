//! Resolution and very small type inference for Body IR.
//!
//! The resolver consumes the already-lowered body store. It answers only cheap questions:
//! local-vs-item path resolution and simple types that are already present in signatures.

mod body;
mod method;
mod ty;
mod type_path;

use rg_def_map::{DefMapDb, PackageSlot, Path, TargetRef};
use rg_parse::TargetId;
use rg_semantic_ir::{FieldRef, FunctionRef, SemanticIrDb, TypePathContext};

use crate::{
    BodyIrDb,
    body::TargetBodiesStatus,
    ids::{BodyFunctionRef, BodyId, BodyRef, ScopeId},
    resolved::BodyTypePathResolution,
    ty::{BodyLocalNominalTy, BodyNominalTy, BodyTy},
};

use self::{
    body::BodyResolver,
    method::{
        local_function_applies_to_receiver as local_function_applies_to_receiver_impl,
        semantic_function_applies_to_receiver as semantic_function_applies_to_receiver_impl,
    },
    ty::{TypeSubst, ty_from_type_ref_in_context},
    type_path::BodyTypePathResolver,
};

pub(super) fn resolve_bodies(db: &mut BodyIrDb, def_map: &DefMapDb, semantic_ir: &SemanticIrDb) {
    // Resolution is a mutation pass over already-lowered bodies. Skipped targets intentionally
    // keep their body stores empty so dependency body internals stay cheap by default.
    for (package_idx, package) in db.packages_mut().iter_mut().enumerate() {
        for (target_idx, target) in package.targets_mut().iter_mut().enumerate() {
            if matches!(target.status(), TargetBodiesStatus::Skipped) {
                continue;
            }

            let target_ref = TargetRef {
                package: PackageSlot(package_idx),
                target: TargetId(target_idx),
            };

            for (body_idx, body) in target.bodies_mut().iter_mut().enumerate() {
                BodyResolver::new(
                    def_map,
                    semantic_ir,
                    BodyRef {
                        target: target_ref,
                        body: BodyId(body_idx),
                    },
                    body,
                )
                .resolve();
            }
        }
    }
}

pub(super) fn resolve_type_path_in_scope(
    db: &BodyIrDb,
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    body_ref: BodyRef,
    scope: ScopeId,
    path: &Path,
) -> BodyTypePathResolution {
    let Some(body) = db.body_data(body_ref) else {
        return BodyTypePathResolution::Unknown;
    };

    BodyTypePathResolver::new(def_map, semantic_ir, body_ref, body).resolve_in_scope(scope, path)
}

pub(super) fn ty_for_field(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    field_ref: FieldRef,
) -> Option<BodyTy> {
    // Field declarations live in Semantic IR, but Analysis expects Body IR's small type
    // vocabulary. Use the owning module as the type-path context for the field signature.
    let field_data = semantic_ir.field_data(field_ref)?;
    Some(ty_from_type_ref_in_context(
        def_map,
        semantic_ir,
        &field_data.field.ty,
        TypePathContext::module(field_data.owner_module),
        BodyTy::Unknown,
        &TypeSubst::new(),
    ))
}

pub(super) fn semantic_function_applies_to_receiver(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    function_ref: FunctionRef,
    receiver_ty: &BodyNominalTy,
) -> bool {
    semantic_function_applies_to_receiver_impl(def_map, semantic_ir, function_ref, receiver_ty)
}

pub(super) fn local_function_applies_to_receiver(
    db: &BodyIrDb,
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    function_ref: BodyFunctionRef,
    receiver_ty: &BodyLocalNominalTy,
) -> bool {
    let Some(body) = db.body_data(function_ref.body) else {
        return false;
    };
    local_function_applies_to_receiver_impl(
        def_map,
        semantic_ir,
        function_ref.body,
        body,
        function_ref,
        receiver_ty,
    )
}

pub(super) fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    // Resolution often merges candidates from local, inherent, and trait sources. Keeping order
    // while deduplicating makes snapshots stable without pretending this is a ranking policy.
    if !items.contains(&item) {
        items.push(item);
    }
}
