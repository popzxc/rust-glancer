//! Tiny type conversion helpers used by body resolution.
//!
//! These helpers preserve known nominal/generic facts. They do not infer missing generic
//! arguments, solve bounds, or inspect expression bodies to discover return types.

use rg_def_map::{DefMapDb, Path};
use rg_item_tree::{GenericArg, GenericParams, TypeRef};
use rg_semantic_ir::{SemanticIrDb, TypePathContext};

use crate::{
    resolved::BodyTypePathResolution,
    ty::{BodyGenericArg, BodyLocalNominalTy, BodyNominalTy, BodyTy},
};

use super::type_path::resolve_type_path_in_context;

/// Mapping from a generic type parameter name to the concrete Body IR type known at a use site.
///
/// For example, `Wrapper<User>` against `struct Wrapper<T>` records `T -> User`.
pub(super) type TypeSubst = Vec<(String, BodyTy)>;

/// Converts syntax-level type data into the small Body IR type vocabulary in one module/impl
/// context, applying direct generic substitutions where they are already known.
pub(super) fn ty_from_type_ref_in_context(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    ty: &TypeRef,
    context: TypePathContext,
    unresolved_path_fallback: BodyTy,
    subst: &TypeSubst,
) -> BodyTy {
    match ty {
        TypeRef::Unit => BodyTy::Unit,
        TypeRef::Never => BodyTy::Never,
        TypeRef::Path(type_path) => {
            let path = Path::from_type_path(type_path);
            if let Some(ty) = substitute_type_param(&path, subst) {
                return ty;
            }

            let args = generic_args_from_type_path_in_context(
                def_map,
                semantic_ir,
                type_path,
                context,
                subst,
            );
            ty_from_body_resolution(
                resolve_type_path_in_context(def_map, semantic_ir, context, &path),
                unresolved_path_fallback,
                args,
            )
        }
        TypeRef::Unknown(_) | TypeRef::Infer => BodyTy::Unknown,
        TypeRef::Tuple(types) if types.is_empty() => BodyTy::Unit,
        _ => BodyTy::Syntax(ty.clone()),
    }
}

pub(super) fn ty_from_body_resolution(
    resolution: BodyTypePathResolution,
    fallback: BodyTy,
    args: Vec<BodyGenericArg>,
) -> BodyTy {
    // Attach the generic arguments from the source path to whichever nominal definition the path
    // resolved to. Ambiguous multi-target resolution keeps the same args on every candidate.
    match resolution {
        BodyTypePathResolution::BodyLocal(item) => {
            BodyTy::LocalNominal(vec![BodyLocalNominalTy { item, args }])
        }
        BodyTypePathResolution::SelfType(types) => BodyTy::SelfTy(
            types
                .into_iter()
                .map(|def| BodyNominalTy {
                    def,
                    args: args.clone(),
                })
                .collect(),
        ),
        BodyTypePathResolution::TypeDefs(types) => BodyTy::Nominal(
            types
                .into_iter()
                .map(|def| BodyNominalTy {
                    def,
                    args: args.clone(),
                })
                .collect(),
        ),
        BodyTypePathResolution::Traits(_) => fallback,
        BodyTypePathResolution::Unknown => fallback,
    }
}

pub(super) fn subst_from_generics(generics: &GenericParams, args: &[BodyGenericArg]) -> TypeSubst {
    // We only substitute type parameters. Lifetimes, const args, associated type args, and
    // unsupported args are preserved on the type but ignored by the simple substitution map.
    let type_args = args.iter().filter_map(body_generic_arg_ty);

    generics
        .types
        .iter()
        .zip(type_args)
        .map(|(param, ty)| (param.name.clone(), ty))
        .collect()
}

pub(super) fn body_generic_arg_ty(arg: &BodyGenericArg) -> Option<BodyTy> {
    match arg {
        BodyGenericArg::Type(ty) => Some((**ty).clone()),
        BodyGenericArg::Lifetime(_)
        | BodyGenericArg::Const(_)
        | BodyGenericArg::AssocType { .. }
        | BodyGenericArg::Unsupported(_) => None,
    }
}

pub(super) fn generic_arg_type_ref(arg: &GenericArg) -> Option<&TypeRef> {
    match arg {
        GenericArg::Type(ty) => Some(ty),
        GenericArg::Lifetime(_)
        | GenericArg::Const(_)
        | GenericArg::AssocType { .. }
        | GenericArg::Unsupported(_) => None,
    }
}

pub(super) fn type_param_name_from_type_ref(ty: &TypeRef) -> Option<String> {
    let TypeRef::Path(path) = ty else {
        return None;
    };

    Path::from_type_path(path)
        .single_name()
        .map(ToString::to_string)
}

pub(super) fn substitute_type_param(path: &Path, subst: &TypeSubst) -> Option<BodyTy> {
    // Only plain identifiers can be generic type parameters. Qualified paths like `module::T`
    // remain ordinary type paths and are resolved through DefMap/Semantic IR.
    let name = path.single_name()?;
    subst
        .iter()
        .rev()
        .find_map(|(param, ty)| (param == name).then(|| ty.clone()))
}

pub(super) fn type_ref_is_self(ty: &TypeRef) -> bool {
    Path::from_type_ref(ty).is_some_and(|path| path.is_self_type())
}

fn generic_args_from_type_path_in_context(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    type_path: &rg_item_tree::TypePath,
    context: TypePathContext,
    subst: &TypeSubst,
) -> Vec<BodyGenericArg> {
    // Rust generic args belong to the final path segment for the cases we model here, e.g.
    // `crate::Wrapper<User>` stores `User` on `Wrapper`.
    type_path
        .segments
        .last()
        .map(|segment| {
            generic_args_from_item_tree_args_in_context(
                def_map,
                semantic_ir,
                &segment.args,
                context,
                subst,
            )
        })
        .unwrap_or_default()
}

fn generic_args_from_item_tree_args_in_context(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    args: &[GenericArg],
    context: TypePathContext,
    subst: &TypeSubst,
) -> Vec<BodyGenericArg> {
    args.iter()
        .map(|arg| {
            generic_arg_from_item_tree_arg_in_context(def_map, semantic_ir, arg, context, subst)
        })
        .collect()
}

fn generic_arg_from_item_tree_arg_in_context(
    def_map: &DefMapDb,
    semantic_ir: &SemanticIrDb,
    arg: &GenericArg,
    context: TypePathContext,
    subst: &TypeSubst,
) -> BodyGenericArg {
    match arg {
        GenericArg::Type(ty) => BodyGenericArg::Type(Box::new(ty_from_type_ref_in_context(
            def_map,
            semantic_ir,
            ty,
            context,
            BodyTy::Syntax(ty.clone()),
            subst,
        ))),
        GenericArg::Lifetime(lifetime) => BodyGenericArg::Lifetime(lifetime.clone()),
        GenericArg::Const(value) => BodyGenericArg::Const(value.clone()),
        GenericArg::AssocType { name, ty } => BodyGenericArg::AssocType {
            name: name.clone(),
            ty: ty.as_ref().map(|ty| {
                Box::new(ty_from_type_ref_in_context(
                    def_map,
                    semantic_ir,
                    ty,
                    context,
                    BodyTy::Syntax(ty.clone()),
                    subst,
                ))
            }),
        },
        GenericArg::Unsupported(text) => BodyGenericArg::Unsupported(text.clone()),
    }
}
