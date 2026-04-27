//! Lightweight semantic method matching.
//!
//! This module checks whether an impl method is a plausible candidate for a known receiver type.
//! It is intentionally not a trait solver: it only compares explicit nominal self types and args.

use rg_def_map::DefMapDb;
use rg_item_tree::{GenericParams, TypeRef};
use rg_semantic_ir::{FunctionRef, ImplRef, ItemOwner, SemanticIrDb, TypePathContext};

use crate::ty::{BodyNominalTy, BodyTy};

use super::ty::{
    TypeSubst, body_generic_arg_ty, generic_arg_type_ref, ty_from_type_ref_in_context,
    type_param_name_from_type_ref,
};

pub(super) fn semantic_function_applies_to_receiver(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    function_ref: FunctionRef,
    receiver_ty: &BodyNominalTy,
) -> bool {
    // Trait items are shared by all impl candidates in the current best-effort model. Inherent
    // impl items, however, must at least match the receiver's resolved self type.
    let Some(function_data) = semantic_ir.function_data(function_ref) else {
        return false;
    };
    let ItemOwner::Impl(impl_id) = function_data.owner else {
        return true;
    };
    let impl_ref = ImplRef {
        target: function_ref.target,
        id: impl_id,
    };
    let Some(impl_data) = semantic_ir.impl_data(impl_ref) else {
        return false;
    };
    if !impl_data.resolved_self_tys.contains(&receiver_ty.def) {
        return false;
    }

    impl_self_args_match_receiver(def_map, semantic_ir, impl_ref, impl_data, receiver_ty)
}

pub(super) fn semantic_impl_self_subst(
    semantic_ir: &SemanticIrDb,
    function_ref: FunctionRef,
    receiver_ty: &BodyNominalTy,
) -> TypeSubst {
    // Convert the impl header into substitutions for method signatures. For
    // `impl<U> Wrapper<U>`, a `Wrapper<User>` receiver gives `U -> User`.
    let Some(function_data) = semantic_ir.function_data(function_ref) else {
        return TypeSubst::new();
    };
    let ItemOwner::Impl(impl_id) = function_data.owner else {
        return TypeSubst::new();
    };
    let Some(impl_data) = semantic_ir.impl_data(ImplRef {
        target: function_ref.target,
        id: impl_id,
    }) else {
        return TypeSubst::new();
    };
    let TypeRef::Path(self_ty) = &impl_data.self_ty else {
        return TypeSubst::new();
    };
    let Some(segment) = self_ty.segments.last() else {
        return TypeSubst::new();
    };

    let impl_type_params = impl_type_param_names(&impl_data.generics);
    let receiver_type_args = receiver_ty
        .args
        .iter()
        .filter_map(body_generic_arg_ty)
        .collect::<Vec<_>>();

    segment
        .args
        .iter()
        .filter_map(generic_arg_type_ref)
        .zip(receiver_type_args)
        .filter_map(|(impl_arg, receiver_arg)| {
            let name = type_param_name_from_type_ref(impl_arg)?;
            impl_type_params
                .contains(&name.as_str())
                .then_some((name, receiver_arg))
        })
        .collect()
}

fn impl_self_args_match_receiver(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    impl_ref: ImplRef,
    impl_data: &rg_semantic_ir::ImplData,
    receiver_ty: &BodyNominalTy,
) -> bool {
    // This is a shallow compatibility check. Impl type parameters behave as wildcards, while
    // concrete args such as `impl Wrapper<User>` must equal the receiver's known args.
    let TypeRef::Path(self_ty) = &impl_data.self_ty else {
        return true;
    };
    let Some(segment) = self_ty.segments.last() else {
        return true;
    };

    let impl_type_args = segment
        .args
        .iter()
        .filter_map(generic_arg_type_ref)
        .collect::<Vec<_>>();
    if impl_type_args.is_empty() {
        return true;
    }

    let receiver_type_args = receiver_ty
        .args
        .iter()
        .filter_map(body_generic_arg_ty)
        .collect::<Vec<_>>();
    if impl_type_args.len() != receiver_type_args.len() {
        return false;
    }

    let impl_type_params = impl_type_param_names(&impl_data.generics);
    for (impl_arg, receiver_arg) in impl_type_args.into_iter().zip(receiver_type_args) {
        if type_param_name_from_type_ref(impl_arg)
            .as_deref()
            .is_some_and(|name| impl_type_params.contains(&name))
        {
            continue;
        }

        let context = TypePathContext {
            module: impl_data.owner,
            impl_ref: Some(impl_ref),
        };
        let impl_arg_ty = ty_from_type_ref_in_context(
            def_map,
            semantic_ir,
            impl_arg,
            context,
            BodyTy::Syntax(impl_arg.clone()),
            &TypeSubst::new(),
        );
        if impl_arg_ty != receiver_arg {
            return false;
        }
    }

    true
}

fn impl_type_param_names(generics: &GenericParams) -> Vec<&str> {
    generics
        .types
        .iter()
        .map(|param| param.name.as_str())
        .collect()
}
