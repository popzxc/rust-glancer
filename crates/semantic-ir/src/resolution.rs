//! Semantic resolution that depends on the completed def-map and semantic IR graphs.
//!
//! Lowering records impl headers as syntax-shaped `TypeRef`s. Once every target has a semantic
//! item store, this pass resolves those headers through def-map path resolution and stores the
//! resolved semantic ids for query consumers.

use rg_def_map::{DefId, DefMapDb, DefMapReadTxn, ModuleRef, PackageSlot, Path, TargetRef};
use rg_item_tree::TypeRef;
use rg_package_store::PackageStoreError;
use rg_parse::TargetId;

use super::{
    SemanticIrDb, SemanticIrReadTxn,
    ids::{ImplRef, TraitRef, TypeDefRef},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypePathContext {
    pub module: ModuleRef,
    pub impl_ref: Option<ImplRef>,
}

impl TypePathContext {
    pub fn module(module: ModuleRef) -> Self {
        Self {
            module,
            impl_ref: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticTypePathResolution {
    SelfType(Vec<TypeDefRef>),
    TypeDefs(Vec<TypeDefRef>),
    Traits(Vec<TraitRef>),
    Unknown,
}

pub(super) fn resolve_impl_headers(db: &mut SemanticIrDb, def_map: &DefMapDb) {
    let impl_refs = db.impl_refs();
    resolve_impl_refs(db, def_map, impl_refs);
}

pub(super) struct ImplHeaderResolution {
    impl_ref: ImplRef,
    resolved_self_tys: Vec<TypeDefRef>,
    resolved_trait_refs: Vec<TraitRef>,
}

pub(super) fn impl_header_resolutions_for_packages(
    semantic_ir: &SemanticIrReadTxn<'_>,
    def_map: &DefMapReadTxn<'_>,
    packages: &[PackageSlot],
) -> Result<Vec<ImplHeaderResolution>, PackageStoreError> {
    let mut resolutions = Vec::new();

    for package in packages {
        let package_ir = semantic_ir.package(*package)?;

        for (target_idx, _) in package_ir.into_ref().targets().iter().enumerate() {
            let target = TargetRef {
                package: *package,
                target: TargetId(target_idx),
            };
            for (impl_ref, _) in semantic_ir.impls(target)? {
                let Some(data) = semantic_ir.impl_data(impl_ref)? else {
                    continue;
                };

                let resolved_self_tys =
                    resolve_type_defs_from_ref(semantic_ir, def_map, data.owner, &data.self_ty)?;
                let resolved_trait_refs = data
                    .trait_ref
                    .as_ref()
                    .map(|ty| resolve_traits_from_ref(semantic_ir, def_map, data.owner, ty))
                    .transpose()?
                    .unwrap_or_default();

                resolutions.push(ImplHeaderResolution {
                    impl_ref,
                    resolved_self_tys,
                    resolved_trait_refs,
                });
            }
        }
    }

    Ok(resolutions)
}

pub(super) fn apply_impl_header_resolutions(
    db: &mut SemanticIrDb,
    resolutions: Vec<ImplHeaderResolution>,
) {
    for resolution in resolutions {
        let Some(data) = db.impl_data_mut(resolution.impl_ref) else {
            continue;
        };
        data.resolved_self_tys = resolution.resolved_self_tys;
        data.resolved_trait_refs = resolution.resolved_trait_refs;
    }
}

fn resolve_impl_refs(db: &mut SemanticIrDb, def_map: &DefMapDb, impl_refs: Vec<ImplRef>) {
    for impl_ref in impl_refs {
        let Some((owner, self_ty, trait_ref)) = db
            .impl_data(impl_ref)
            .map(|data| (data.owner, data.self_ty.clone(), data.trait_ref.clone()))
        else {
            continue;
        };

        let resolved_self_tys = resolve_type_defs(db, def_map, owner, &self_ty);
        let resolved_trait_refs = trait_ref
            .as_ref()
            .map(|ty| resolve_traits(db, def_map, owner, ty))
            .unwrap_or_default();

        let Some(data) = db.impl_data_mut(impl_ref) else {
            continue;
        };
        data.resolved_self_tys = resolved_self_tys;
        data.resolved_trait_refs = resolved_trait_refs;
    }
}

pub(super) fn resolve_type_defs_for_path(
    db: &SemanticIrDb,
    def_map: &DefMapDb,
    owner: ModuleRef,
    path: &Path,
) -> Vec<TypeDefRef> {
    resolve_path(db, def_map, owner, path, |db, def| {
        let DefId::Local(local_def) = def else {
            return None;
        };

        db.type_def_for_local_def(local_def)
    })
}

pub(super) fn resolve_traits_for_path(
    db: &SemanticIrDb,
    def_map: &DefMapDb,
    owner: ModuleRef,
    path: &Path,
) -> Vec<TraitRef> {
    resolve_path(db, def_map, owner, path, |db, def| {
        let DefId::Local(local_def) = def else {
            return None;
        };

        db.trait_for_local_def(local_def)
    })
}

pub(super) fn resolve_type_path(
    db: &SemanticIrDb,
    def_map: &DefMapDb,
    context: TypePathContext,
    path: &Path,
) -> SemanticTypePathResolution {
    if path.is_self_type() {
        let Some(impl_ref) = context.impl_ref else {
            return SemanticTypePathResolution::Unknown;
        };
        let types = db
            .impl_data(impl_ref)
            .map(|data| data.resolved_self_tys.clone())
            .unwrap_or_default();
        return if types.is_empty() {
            SemanticTypePathResolution::Unknown
        } else {
            SemanticTypePathResolution::SelfType(types)
        };
    }

    let type_defs = resolve_type_defs_for_path(db, def_map, context.module, path);
    if type_defs.is_empty() {
        let traits = resolve_traits_for_path(db, def_map, context.module, path);
        if traits.is_empty() {
            SemanticTypePathResolution::Unknown
        } else {
            SemanticTypePathResolution::Traits(traits)
        }
    } else {
        SemanticTypePathResolution::TypeDefs(type_defs)
    }
}

fn resolve_type_defs(
    db: &SemanticIrDb,
    def_map: &DefMapDb,
    owner: ModuleRef,
    ty: &TypeRef,
) -> Vec<TypeDefRef> {
    resolve_type_ref(db, def_map, owner, ty, |db, def| {
        let DefId::Local(local_def) = def else {
            return None;
        };

        db.type_def_for_local_def(local_def)
    })
}

fn resolve_traits(
    db: &SemanticIrDb,
    def_map: &DefMapDb,
    owner: ModuleRef,
    ty: &TypeRef,
) -> Vec<TraitRef> {
    resolve_type_ref(db, def_map, owner, ty, |db, def| {
        let DefId::Local(local_def) = def else {
            return None;
        };

        db.trait_for_local_def(local_def)
    })
}

fn resolve_type_defs_from_ref(
    db: &SemanticIrReadTxn<'_>,
    def_map: &DefMapReadTxn<'_>,
    owner: ModuleRef,
    ty: &TypeRef,
) -> Result<Vec<TypeDefRef>, PackageStoreError> {
    let Some(path) = Path::from_type_ref(ty) else {
        return Ok(Vec::new());
    };

    db.type_defs_for_path(def_map, owner, &path)
}

fn resolve_traits_from_ref(
    db: &SemanticIrReadTxn<'_>,
    def_map: &DefMapReadTxn<'_>,
    owner: ModuleRef,
    ty: &TypeRef,
) -> Result<Vec<TraitRef>, PackageStoreError> {
    let Some(path) = Path::from_type_ref(ty) else {
        return Ok(Vec::new());
    };

    db.traits_for_path(def_map, owner, &path)
}

fn resolve_type_ref<T: PartialEq>(
    db: &SemanticIrDb,
    def_map: &DefMapDb,
    owner: ModuleRef,
    ty: &TypeRef,
    map_def: impl Fn(&SemanticIrDb, DefId) -> Option<T>,
) -> Vec<T> {
    let Some(path) = Path::from_type_ref(ty) else {
        return Vec::new();
    };

    resolve_path(db, def_map, owner, &path, map_def)
}

fn resolve_path<T: PartialEq>(
    db: &SemanticIrDb,
    def_map: &DefMapDb,
    owner: ModuleRef,
    path: &Path,
    map_def: impl Fn(&SemanticIrDb, DefId) -> Option<T>,
) -> Vec<T> {
    let mut resolved_items = Vec::new();
    for def in def_map.resolve_path_in_type_namespace(owner, path).resolved {
        let Some(item) = map_def(db, def) else {
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
