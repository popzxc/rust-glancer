use rg_memsize::{MemoryRecorder, MemorySize};

use crate::{
    AssocItemId, ConstData, ConstId, ConstRef, ConstSignature, EnumData, EnumId, EnumVariantRef,
    FieldRef, FunctionData, FunctionId, FunctionRef, FunctionSignature, ImplData, ImplId, ImplRef,
    ItemId, ItemOwner, ItemStore, PackageIr, SemanticIrDb, SemanticIrStats,
    SemanticTypePathResolution, StaticData, StaticId, StaticRef, StructData, StructId, TargetIr,
    TraitApplicability, TraitId, TraitImplRef, TraitRef, TypeAliasData, TypeAliasId, TypeAliasRef,
    TypeAliasSignature, TypeDefId, TypeDefRef, TypePathContext, UnionData, UnionId,
    signature::SignatureGenerics,
};

macro_rules! record_fields {
    ($recorder:expr, $owner:expr, $($field:ident),+ $(,)?) => {
        $(
            $recorder.scope(stringify!($field), |recorder| {
                $owner.$field.record_memory_children(recorder);
            });
        )+
    };
}

macro_rules! impl_leaf_memory_size {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl MemorySize for $ty {
                fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
            }
        )+
    };
}

impl MemorySize for SemanticIrDb {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("packages", |recorder| {
            self.packages.record_memory_children(recorder);
        });
    }
}

impl MemorySize for PackageIr {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("targets", |recorder| {
            self.targets.record_memory_children(recorder);
        });
    }
}

impl MemorySize for TargetIr {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, local_items, local_impls, items);
    }
}

impl MemorySize for ItemStore {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            structs,
            unions,
            enums,
            traits,
            impls,
            functions,
            type_aliases,
            consts,
            statics,
        );
    }
}

impl MemorySize for StructData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder, self, local_def, source, owner, name, visibility, docs, generics, fields,
        );
    }
}

impl MemorySize for UnionData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder, self, local_def, source, owner, name, visibility, docs, generics, fields,
        );
    }
}

impl MemorySize for EnumData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder, self, local_def, source, owner, name, visibility, docs, generics, variants,
        );
    }
}

impl MemorySize for crate::TraitData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            local_def,
            source,
            owner,
            name,
            visibility,
            docs,
            generics,
            super_traits,
            items,
            is_unsafe,
        );
    }
}

impl MemorySize for ImplData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            local_impl,
            source,
            owner,
            generics,
            trait_ref,
            self_ty,
            resolved_self_tys,
            resolved_trait_refs,
            items,
            is_unsafe,
        );
    }
}

impl MemorySize for FunctionData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder, self, local_def, source, span, name_span, owner, name, visibility, docs,
            signature,
        );
    }
}

impl MemorySize for FunctionSignature {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, generics, params, ret_ty, qualifiers);
    }
}

impl MemorySize for SignatureGenerics {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Empty => {}
            Self::Present(params) => params.record_memory_children(recorder),
        }
    }
}

impl MemorySize for TypeAliasData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder, self, local_def, source, span, name_span, owner, name, visibility, docs,
            signature,
        );
    }
}

impl MemorySize for TypeAliasSignature {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, generics, bounds, aliased_ty);
    }
}

impl MemorySize for ConstData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder, self, local_def, source, span, name_span, owner, name, visibility, docs,
            signature,
        );
    }
}

impl MemorySize for ConstSignature {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, ty);
    }
}

impl MemorySize for StaticData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder, self, local_def, source, span, name_span, owner, name, visibility, docs, ty,
            mutability,
        );
    }
}

impl_leaf_memory_size!(
    StructId,
    UnionId,
    EnumId,
    TraitId,
    ImplId,
    FunctionId,
    TypeAliasId,
    ConstId,
    StaticId,
    TraitApplicability,
);

impl MemorySize for TypeDefId {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Struct(id) => id.record_memory_children(recorder),
            Self::Enum(id) => id.record_memory_children(recorder),
            Self::Union(id) => id.record_memory_children(recorder),
        }
    }
}

impl MemorySize for TypeDefRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, id);
    }
}

impl MemorySize for TraitRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, id);
    }
}

impl MemorySize for ImplRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, id);
    }
}

impl MemorySize for FunctionRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, id);
    }
}

impl MemorySize for TypeAliasRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, id);
    }
}

impl MemorySize for ConstRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, id);
    }
}

impl MemorySize for StaticRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, id);
    }
}

impl MemorySize for FieldRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, owner, index);
    }
}

impl MemorySize for EnumVariantRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, enum_id, index);
    }
}

impl MemorySize for TraitImplRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, impl_ref, trait_ref);
    }
}

impl MemorySize for ItemId {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Struct(id) => id.record_memory_children(recorder),
            Self::Union(id) => id.record_memory_children(recorder),
            Self::Enum(id) => id.record_memory_children(recorder),
            Self::Trait(id) => id.record_memory_children(recorder),
            Self::Function(id) => id.record_memory_children(recorder),
            Self::TypeAlias(id) => id.record_memory_children(recorder),
            Self::Const(id) => id.record_memory_children(recorder),
            Self::Static(id) => id.record_memory_children(recorder),
        }
    }
}

impl MemorySize for AssocItemId {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Function(id) => id.record_memory_children(recorder),
            Self::TypeAlias(id) => id.record_memory_children(recorder),
            Self::Const(id) => id.record_memory_children(recorder),
        }
    }
}

impl MemorySize for ItemOwner {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Module(module) => module.record_memory_children(recorder),
            Self::Trait(trait_id) => trait_id.record_memory_children(recorder),
            Self::Impl(impl_id) => impl_id.record_memory_children(recorder),
        }
    }
}

impl MemorySize for TypePathContext {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, module, impl_ref);
    }
}

impl MemorySize for SemanticTypePathResolution {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::SelfType(types) | Self::TypeDefs(types) => types.record_memory_children(recorder),
            Self::Traits(traits) => traits.record_memory_children(recorder),
            Self::Unknown => {}
        }
    }
}

impl MemorySize for SemanticIrStats {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            target_count,
            struct_count,
            union_count,
            enum_count,
            trait_count,
            impl_count,
            function_count,
            type_alias_count,
            const_count,
            static_count,
        );
    }
}
