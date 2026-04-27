mod cursor;
mod ids;
mod items;
mod lower;
mod package;
mod resolution;
mod stats;
mod target;

#[cfg(test)]
mod tests;

use rg_def_map::{DefMapDb, LocalDefRef, ModuleRef, PackageSlot, Path, TargetRef};
use rg_item_tree::FieldKey;
use rg_parse::TargetId;

pub use self::{
    cursor::SemanticCursorCandidate,
    ids::{
        AssocItemId, ConstId, ConstRef, EnumId, FieldRef, FunctionId, FunctionRef, ImplId, ImplRef,
        ItemId, ItemOwner, StaticId, StaticRef, StructId, TraitId, TraitImplRef, TraitRef,
        TypeAliasId, TypeAliasRef, TypeDefId, TypeDefRef, UnionId,
    },
    items::{
        ConstData, EnumData, FieldData, FunctionData, ImplData, ItemStore, StaticData, StructData,
        TraitData, TypeAliasData, UnionData,
    },
    package::PackageIr,
    resolution::{SemanticTypePathResolution, TypePathContext},
    stats::SemanticIrStats,
    target::TargetIr,
};

/// Semantic item graph for all analyzed packages and targets.
///
/// Semantic IR is the signature layer: it keeps named items, fields, impl headers, function
/// signatures, and enough resolution metadata to answer LSP-shaped questions without parsing AST
/// again. Bodies live in `rg_body_ir`; this layer intentionally stops at item/signature facts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticIrDb {
    packages: Vec<PackageIr>,
}

impl SemanticIrDb {
    /// Builds semantic IR from already-collected item trees and frozen name-resolution maps.
    pub fn build(
        item_tree: &rg_item_tree::ItemTreeDb,
        def_map: &rg_def_map::DefMapDb,
    ) -> anyhow::Result<Self> {
        let mut db = lower::build_db(item_tree, def_map)?;
        resolution::resolve_impl_headers(&mut db, def_map);
        Ok(db)
    }

    pub(crate) fn new(packages: Vec<PackageIr>) -> Self {
        Self { packages }
    }

    /// Returns coarse item counts for status output and smoke checks.
    pub fn stats(&self) -> SemanticIrStats {
        let mut stats = SemanticIrStats::default();

        for package in &self.packages {
            for target in package.targets() {
                let items = target.items();
                stats.target_count += 1;
                stats.struct_count += items.structs.len();
                stats.union_count += items.unions.len();
                stats.enum_count += items.enums.len();
                stats.trait_count += items.traits.len();
                stats.impl_count += items.impls.len();
                stats.function_count += items.functions.len();
                stats.type_alias_count += items.type_aliases.len();
                stats.const_count += items.consts.len();
                stats.static_count += items.statics.len();
            }
        }

        stats
    }

    /// Returns all package-level semantic IR sets.
    pub fn packages(&self) -> &[PackageIr] {
        &self.packages
    }

    /// Returns one package by package slot.
    pub fn package(&self, package: PackageSlot) -> Option<&PackageIr> {
        self.packages.get(package.0)
    }

    /// Returns one target semantic IR by project-wide target reference.
    pub fn target_ir(&self, target: TargetRef) -> Option<&TargetIr> {
        self.package(target.package)?.target(target.target)
    }

    /// Iterates over every target IR together with its project-wide target reference.
    pub fn target_irs(&self) -> impl Iterator<Item = (TargetRef, &TargetIr)> {
        self.packages
            .iter()
            .enumerate()
            .flat_map(|(package_idx, package)| {
                package
                    .targets()
                    .iter()
                    .enumerate()
                    .map(move |(target_idx, target_ir)| {
                        (
                            TargetRef {
                                package: PackageSlot(package_idx),
                                target: TargetId(target_idx),
                            },
                            target_ir,
                        )
                    })
            })
    }

    /// Iterates over one target's structs together with stable project-wide references.
    pub fn structs(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (TypeDefRef, &StructData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .structs
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            TypeDefRef {
                                target,
                                id: TypeDefId::Struct(StructId(idx)),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's unions together with stable project-wide references.
    pub fn unions(&self, target: TargetRef) -> impl Iterator<Item = (TypeDefRef, &UnionData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .unions
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            TypeDefRef {
                                target,
                                id: TypeDefId::Union(UnionId(idx)),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's enums together with stable project-wide references.
    pub fn enums(&self, target: TargetRef) -> impl Iterator<Item = (TypeDefRef, &EnumData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .enums
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            TypeDefRef {
                                target,
                                id: TypeDefId::Enum(EnumId(idx)),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's traits together with stable project-wide references.
    pub fn traits(&self, target: TargetRef) -> impl Iterator<Item = (TraitRef, &TraitData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .traits
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            TraitRef {
                                target,
                                id: TraitId(idx),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's impls together with stable project-wide references.
    pub fn impls(&self, target: TargetRef) -> impl Iterator<Item = (ImplRef, &ImplData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .impls
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            ImplRef {
                                target,
                                id: ImplId(idx),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's functions together with stable project-wide references.
    pub fn functions(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (FunctionRef, &FunctionData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .functions
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            FunctionRef {
                                target,
                                id: FunctionId(idx),
                            },
                            data,
                        )
                    })
            })
    }

    /// Returns the number of function slots in one target.
    pub fn function_count(&self, target: TargetRef) -> usize {
        self.functions(target).count()
    }

    /// Iterates over one target's type aliases together with stable project-wide references.
    pub fn type_aliases(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (TypeAliasRef, &TypeAliasData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .type_aliases
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            TypeAliasRef {
                                target,
                                id: TypeAliasId(idx),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's consts together with stable project-wide references.
    pub fn consts(&self, target: TargetRef) -> impl Iterator<Item = (ConstRef, &ConstData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .consts
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            ConstRef {
                                target,
                                id: ConstId(idx),
                            },
                            data,
                        )
                    })
            })
    }

    /// Iterates over one target's statics together with stable project-wide references.
    pub fn statics(
        &self,
        target: TargetRef,
    ) -> impl Iterator<Item = (StaticRef, &StaticData)> + '_ {
        self.target_ir(target)
            .into_iter()
            .flat_map(move |target_ir| {
                target_ir
                    .items()
                    .statics
                    .iter()
                    .enumerate()
                    .map(move |(idx, data)| {
                        (
                            StaticRef {
                                target,
                                id: StaticId(idx),
                            },
                            data,
                        )
                    })
            })
    }

    /// Resolves a path as a nominal type definition from one module.
    pub fn type_defs_for_path(
        &self,
        def_map: &DefMapDb,
        from: ModuleRef,
        path: &Path,
    ) -> Vec<TypeDefRef> {
        resolution::resolve_type_defs_for_path(self, def_map, from, path)
    }

    /// Resolves a type path with enough owner context to handle `Self`.
    pub fn resolve_type_path(
        &self,
        def_map: &DefMapDb,
        context: TypePathContext,
        path: &Path,
    ) -> SemanticTypePathResolution {
        resolution::resolve_type_path(self, def_map, context, path)
    }

    /// Builds type-resolution context for a function signature/body owner.
    pub fn type_path_context_for_function(
        &self,
        function_ref: FunctionRef,
    ) -> Option<TypePathContext> {
        let function_data = self.function_data(function_ref)?;
        self.type_path_context_for_owner(function_ref.target, function_data.owner)
    }

    /// Builds type-resolution context for any semantic item owner.
    pub fn type_path_context_for_owner(
        &self,
        target: TargetRef,
        owner: ItemOwner,
    ) -> Option<TypePathContext> {
        match owner {
            ItemOwner::Module(module_ref) => Some(TypePathContext::module(module_ref)),
            ItemOwner::Trait(id) => self
                .trait_data(TraitRef { target, id })
                .map(|data| TypePathContext::module(data.owner)),
            ItemOwner::Impl(id) => {
                let impl_ref = ImplRef { target, id };
                self.impl_data(impl_ref).map(|data| TypePathContext {
                    module: data.owner,
                    impl_ref: Some(impl_ref),
                })
            }
        }
    }

    pub fn type_def_for_local_def(&self, def: LocalDefRef) -> Option<TypeDefRef> {
        let item = self
            .target_ir(def.target)?
            .item_for_local_def(def.local_def)?;
        let id = match item {
            ItemId::Struct(id) => TypeDefId::Struct(id),
            ItemId::Enum(id) => TypeDefId::Enum(id),
            ItemId::Union(id) => TypeDefId::Union(id),
            ItemId::Trait(_)
            | ItemId::Function(_)
            | ItemId::TypeAlias(_)
            | ItemId::Const(_)
            | ItemId::Static(_) => return None,
        };

        Some(TypeDefRef {
            target: def.target,
            id,
        })
    }

    pub fn trait_for_local_def(&self, def: LocalDefRef) -> Option<TraitRef> {
        let item = self
            .target_ir(def.target)?
            .item_for_local_def(def.local_def)?;
        let ItemId::Trait(id) = item else {
            return None;
        };

        Some(TraitRef {
            target: def.target,
            id,
        })
    }

    pub fn function_for_local_def(&self, def: LocalDefRef) -> Option<FunctionRef> {
        let item = self
            .target_ir(def.target)?
            .item_for_local_def(def.local_def)?;
        let ItemId::Function(id) = item else {
            return None;
        };

        Some(FunctionRef {
            target: def.target,
            id,
        })
    }

    pub fn local_def_for_type_def(&self, ty: TypeDefRef) -> Option<LocalDefRef> {
        let target_ir = self.target_ir(ty.target)?;
        match ty.id {
            TypeDefId::Struct(id) => Some(target_ir.items().struct_data(id)?.local_def),
            TypeDefId::Enum(id) => Some(target_ir.items().enum_data(id)?.local_def),
            TypeDefId::Union(id) => Some(target_ir.items().union_data(id)?.local_def),
        }
    }

    pub fn impl_data(&self, impl_ref: ImplRef) -> Option<&ImplData> {
        self.target_ir(impl_ref.target)?
            .items()
            .impl_data(impl_ref.id)
    }

    pub fn trait_data(&self, trait_ref: TraitRef) -> Option<&TraitData> {
        self.target_ir(trait_ref.target)?
            .items()
            .trait_data(trait_ref.id)
    }

    pub fn function_data(&self, function_ref: FunctionRef) -> Option<&FunctionData> {
        self.target_ir(function_ref.target)?
            .items()
            .function_data(function_ref.id)
    }

    pub fn type_alias_data(&self, type_alias_ref: TypeAliasRef) -> Option<&TypeAliasData> {
        self.target_ir(type_alias_ref.target)?
            .items()
            .type_alias_data(type_alias_ref.id)
    }

    pub fn const_data(&self, const_ref: ConstRef) -> Option<&ConstData> {
        self.target_ir(const_ref.target)?
            .items()
            .const_data(const_ref.id)
    }

    pub fn static_data(&self, static_ref: StaticRef) -> Option<&StaticData> {
        self.target_ir(static_ref.target)?
            .items()
            .static_data(static_ref.id)
    }

    pub fn fields_for_type(&self, ty: TypeDefRef) -> Vec<FieldRef> {
        let Some(field_count) = self.field_count_for_type(ty) else {
            return Vec::new();
        };

        (0..field_count)
            .map(|index| FieldRef { owner: ty, index })
            .collect()
    }

    pub fn field_for_type(&self, ty: TypeDefRef, key: &FieldKey) -> Option<FieldRef> {
        match key {
            FieldKey::Named(_) => self.fields_for_type(ty).into_iter().find(|field_ref| {
                self.field_data(*field_ref)
                    .is_some_and(|data| data.field.key.as_ref() == Some(key))
            }),
            FieldKey::Tuple(index) => {
                let field_ref = FieldRef {
                    owner: ty,
                    index: *index,
                };
                self.field_data(field_ref)
                    .is_some_and(|data| data.field.key.as_ref() == Some(key))
                    .then_some(field_ref)
            }
        }
    }

    pub fn field_data(&self, field_ref: FieldRef) -> Option<FieldData<'_>> {
        let target_ir = self.target_ir(field_ref.owner.target)?;
        match field_ref.owner.id {
            TypeDefId::Struct(id) => {
                let data = target_ir.items().struct_data(id)?;
                let field = data.fields.fields().get(field_ref.index)?;
                Some(FieldData {
                    owner_module: data.owner,
                    file_id: data.source.file_id,
                    field,
                })
            }
            TypeDefId::Union(id) => {
                let data = target_ir.items().union_data(id)?;
                let field = data.fields.get(field_ref.index)?;
                Some(FieldData {
                    owner_module: data.owner,
                    file_id: data.source.file_id,
                    field,
                })
            }
            TypeDefId::Enum(_) => None,
        }
    }

    pub fn impls_for_type(&self, ty: TypeDefRef) -> Vec<ImplRef> {
        self.impl_refs()
            .into_iter()
            .filter(|impl_ref| {
                self.impl_data(*impl_ref)
                    .is_some_and(|data| data.resolved_self_tys.contains(&ty))
            })
            .collect()
    }

    pub fn inherent_impls_for_type(&self, ty: TypeDefRef) -> Vec<ImplRef> {
        self.impls_for_type(ty)
            .into_iter()
            .filter(|impl_ref| {
                self.impl_data(*impl_ref)
                    .is_some_and(|data| data.trait_ref.is_none())
            })
            .collect()
    }

    pub fn trait_impls_for_type(&self, ty: TypeDefRef) -> Vec<TraitImplRef> {
        let mut trait_impls = Vec::new();

        for impl_ref in self.impls_for_type(ty) {
            let Some(data) = self.impl_data(impl_ref) else {
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

        trait_impls
    }

    pub fn traits_for_type(&self, ty: TypeDefRef) -> Vec<TraitRef> {
        let mut traits = Vec::new();

        for trait_impl in self.trait_impls_for_type(ty) {
            push_unique(&mut traits, trait_impl.trait_ref);
        }

        traits
    }

    pub fn inherent_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef> {
        let mut functions = Vec::new();

        for impl_ref in self.inherent_impls_for_type(ty) {
            let Some(data) = self.impl_data(impl_ref) else {
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

        functions
    }

    pub fn trait_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef> {
        let mut functions = Vec::new();

        for trait_ref in self.traits_for_type(ty) {
            let Some(data) = self.trait_data(trait_ref) else {
                continue;
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
        }

        functions
    }

    pub fn trait_impl_functions_for_type(&self, ty: TypeDefRef) -> Vec<FunctionRef> {
        let mut functions = Vec::new();

        for trait_impl in self.trait_impls_for_type(ty) {
            let Some(data) = self.impl_data(trait_impl.impl_ref) else {
                continue;
            };

            for item in &data.items {
                if let AssocItemId::Function(id) = item {
                    push_unique(
                        &mut functions,
                        FunctionRef {
                            target: trait_impl.impl_ref.target,
                            id: *id,
                        },
                    );
                }
            }
        }

        functions
    }

    fn field_count_for_type(&self, ty: TypeDefRef) -> Option<usize> {
        let target_ir = self.target_ir(ty.target)?;
        match ty.id {
            TypeDefId::Struct(id) => Some(target_ir.items().struct_data(id)?.fields.fields().len()),
            TypeDefId::Union(id) => Some(target_ir.items().union_data(id)?.fields.len()),
            TypeDefId::Enum(_) => None,
        }
    }

    pub(crate) fn impl_refs(&self) -> Vec<ImplRef> {
        self.target_irs()
            .flat_map(|(target, _)| self.impls(target).map(|(impl_ref, _)| impl_ref))
            .collect()
    }

    pub(crate) fn impl_data_mut(&mut self, impl_ref: ImplRef) -> Option<&mut ImplData> {
        self.package_mut(impl_ref.target.package)?
            .target_mut(impl_ref.target.target)?
            .items_mut()
            .impls
            .get_mut(impl_ref.id.0)
    }

    fn package_mut(&mut self, package: PackageSlot) -> Option<&mut PackageIr> {
        self.packages.get_mut(package.0)
    }
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}
