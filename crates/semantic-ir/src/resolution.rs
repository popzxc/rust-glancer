//! Semantic resolution that depends on the completed def-map and semantic IR graphs.
//!
//! Lowering records impl headers as syntax-shaped `TypeRef`s. Once every target has a semantic
//! item store, this pass resolves those headers through def-map path resolution and stores the
//! resolved semantic ids for query consumers.

use rg_def_map::{DefId, DefMapDb, ModuleRef, Path};
use rg_item_tree::TypeRef;

use super::{
    data::SemanticIrDb,
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
    for def in def_map.resolve_path(owner, path).resolved {
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
