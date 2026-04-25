//! Semantic resolution that depends on the completed def-map and semantic IR graphs.
//!
//! Lowering records impl headers as syntax-shaped `TypeRef`s. Once every target has a semantic
//! item store, this pass resolves those headers through def-map path resolution and stores the
//! resolved semantic ids for query consumers.

use crate::{
    def_map::{DefId, DefMapDb, Path, PathSegment},
    item_tree::TypeRef,
};

use super::{
    data::SemanticIrDb,
    ids::{TraitRef, TypeDefRef},
};

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
    owner: crate::def_map::ModuleRef,
    path: &Path,
) -> Vec<TypeDefRef> {
    resolve_path(db, def_map, owner, path, |db, def| {
        let DefId::Local(local_def) = def else {
            return None;
        };

        db.type_def_for_local_def(local_def)
    })
}

fn resolve_type_defs(
    db: &SemanticIrDb,
    def_map: &DefMapDb,
    owner: crate::def_map::ModuleRef,
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
    owner: crate::def_map::ModuleRef,
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
    owner: crate::def_map::ModuleRef,
    ty: &TypeRef,
    map_def: impl Fn(&SemanticIrDb, DefId) -> Option<T>,
) -> Vec<T> {
    let Some(path) = path_from_type_ref(ty) else {
        return Vec::new();
    };

    resolve_path(db, def_map, owner, &path, map_def)
}

fn resolve_path<T: PartialEq>(
    db: &SemanticIrDb,
    def_map: &DefMapDb,
    owner: crate::def_map::ModuleRef,
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

fn path_from_type_ref(ty: &TypeRef) -> Option<Path> {
    let TypeRef::Path(path) = ty else {
        return None;
    };

    Some(Path {
        absolute: path.absolute,
        segments: path
            .segments
            .iter()
            .map(|segment| match segment.name.as_str() {
                "self" => PathSegment::SelfKw,
                "super" => PathSegment::SuperKw,
                "crate" => PathSegment::CrateKw,
                name => PathSegment::Name(name.to_string()),
            })
            .collect(),
    })
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}
