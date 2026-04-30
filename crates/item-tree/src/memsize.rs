use rg_memsize::{MemoryRecorder, MemorySize};

use crate::{
    ConstItem, Documentation, EnumItem, EnumVariantItem, ExternCrateItem, FieldItem, FieldKey,
    FieldList, FunctionItem, GenericArg, GenericParams, ImplItem, ImportAlias, ItemKind, ItemNode,
    ItemTag, ItemTreeDb, ItemTreeId, ItemTreeRef, ModuleItem, ModuleSource, Mutability, Package,
    ParamItem, ParamKind, StaticItem, StructItem, TargetRoot, TraitItem, TypeAliasItem, TypeBound,
    TypePath, TypePathSegment, TypeRef, UnionItem, UseImport, UseImportKind, UseItem, UsePath,
    UsePathSegment, UsePathSegmentKind, VisibilityLevel, WherePredicate,
    item::{ConstParamData, FunctionQualifiers, LifetimeParamData, TypeParamData},
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

impl MemorySize for ItemTreeDb {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("packages", |recorder| {
            self.packages.record_memory_children(recorder);
        });
    }
}

impl MemorySize for Package {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, files, target_roots);
    }
}

impl MemorySize for crate::FileTree {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, file, docs, top_level, items);
    }
}

impl MemorySize for TargetRoot {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, target, root_file);
    }
}

impl MemorySize for ItemTreeId {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for ItemTreeRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, file_id, item);
    }
}

impl MemorySize for ItemNode {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder, self, kind, name, name_span, visibility, docs, file_id, span,
        );
    }
}

impl MemorySize for Documentation {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        self.text.record_memory_children(recorder);
    }
}

impl MemorySize for GenericParams {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, lifetimes, types, consts, where_predicates,);
    }
}

impl MemorySize for LifetimeParamData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, name, bounds);
    }
}

impl MemorySize for TypeParamData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, name, bounds, default);
    }
}

impl MemorySize for ConstParamData {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, name, ty, default);
    }
}

impl MemorySize for WherePredicate {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Type { ty, bounds } => {
                recorder.scope("ty", |recorder| ty.record_memory_children(recorder));
                recorder.scope("bounds", |recorder| bounds.record_memory_children(recorder));
            }
            Self::Lifetime { lifetime, bounds } => {
                recorder.scope("lifetime", |recorder| {
                    lifetime.record_memory_children(recorder);
                });
                recorder.scope("bounds", |recorder| bounds.record_memory_children(recorder));
            }
            Self::Unsupported(text) => text.record_memory_children(recorder),
        }
    }
}

impl MemorySize for FunctionItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, generics, params, ret_ty, qualifiers);
    }
}

impl MemorySize for FunctionQualifiers {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, is_async, is_const, is_unsafe);
    }
}

impl MemorySize for ParamItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, pat, ty, kind);
    }
}

impl MemorySize for ParamKind {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for StructItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, generics, fields);
    }
}

impl MemorySize for UnionItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, generics, fields);
    }
}

impl MemorySize for EnumItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, generics, variants);
    }
}

impl MemorySize for EnumVariantItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, name, span, name_span, docs, fields);
    }
}

impl MemorySize for FieldList {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Named(fields) | Self::Tuple(fields) => fields.record_memory_children(recorder),
            Self::Unit => {}
        }
    }
}

impl MemorySize for FieldItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, key, visibility, ty, span, docs);
    }
}

impl MemorySize for FieldKey {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Named(name) => name.record_memory_children(recorder),
            Self::Tuple(index) => index.record_memory_children(recorder),
        }
    }
}

impl MemorySize for TraitItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, generics, super_traits, items, is_unsafe,);
    }
}

impl MemorySize for ImplItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder, self, generics, trait_ref, self_ty, items, is_unsafe,
        );
    }
}

impl MemorySize for TypeAliasItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, generics, bounds, aliased_ty);
    }
}

impl MemorySize for ConstItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, generics, ty);
    }
}

impl MemorySize for StaticItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, ty, mutability);
    }
}

impl MemorySize for TypeRef {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Unknown(text) => text.record_memory_children(recorder),
            Self::Never | Self::Unit | Self::Infer => {}
            Self::Path(path) => path.record_memory_children(recorder),
            Self::Tuple(types) => types.record_memory_children(recorder),
            Self::Reference {
                lifetime,
                mutability,
                inner,
            } => {
                recorder.scope("lifetime", |recorder| {
                    lifetime.record_memory_children(recorder);
                });
                recorder.scope("mutability", |recorder| {
                    mutability.record_memory_children(recorder);
                });
                recorder.scope("inner", |recorder| inner.record_memory_children(recorder));
            }
            Self::RawPointer { mutability, inner } => {
                recorder.scope("mutability", |recorder| {
                    mutability.record_memory_children(recorder);
                });
                recorder.scope("inner", |recorder| inner.record_memory_children(recorder));
            }
            Self::Slice(inner) => inner.record_memory_children(recorder),
            Self::Array { inner, len } => {
                recorder.scope("inner", |recorder| inner.record_memory_children(recorder));
                recorder.scope("len", |recorder| len.record_memory_children(recorder));
            }
            Self::FnPointer { params, ret } => {
                recorder.scope("params", |recorder| params.record_memory_children(recorder));
                recorder.scope("ret", |recorder| ret.record_memory_children(recorder));
            }
            Self::ImplTrait(bounds) | Self::DynTrait(bounds) => {
                bounds.record_memory_children(recorder);
            }
        }
    }
}

impl MemorySize for Mutability {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for TypePath {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, absolute, segments);
    }
}

impl MemorySize for TypePathSegment {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, name, args, span);
    }
}

impl MemorySize for GenericArg {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Type(ty) => ty.record_memory_children(recorder),
            Self::Lifetime(lifetime) | Self::Const(lifetime) | Self::Unsupported(lifetime) => {
                lifetime.record_memory_children(recorder);
            }
            Self::AssocType { name, ty } => {
                recorder.scope("name", |recorder| name.record_memory_children(recorder));
                recorder.scope("ty", |recorder| ty.record_memory_children(recorder));
            }
        }
    }
}

impl MemorySize for TypeBound {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Trait(ty) => ty.record_memory_children(recorder),
            Self::Lifetime(lifetime) | Self::Unsupported(lifetime) => {
                lifetime.record_memory_children(recorder);
            }
        }
    }
}

impl MemorySize for ExternCrateItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, name, alias);
    }
}

impl MemorySize for UseItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("imports", |recorder| {
            self.imports.record_memory_children(recorder);
        });
    }
}

impl MemorySize for UseImport {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, kind, path, alias);
    }
}

impl MemorySize for UseImportKind {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for ImportAlias {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Inferred | Self::Hidden => {}
            Self::Explicit { name, span } => {
                recorder.scope("name", |recorder| name.record_memory_children(recorder));
                recorder.scope("span", |recorder| span.record_memory_children(recorder));
            }
        }
    }
}

impl MemorySize for UsePath {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, absolute, segments);
    }
}

impl MemorySize for UsePathSegment {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, kind, span);
    }
}

impl MemorySize for UsePathSegmentKind {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Name(name) => name.record_memory_children(recorder),
            Self::SelfKw | Self::SuperKw | Self::CrateKw => {}
        }
    }
}

impl MemorySize for ItemKind {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::AsmExpr | Self::ExternBlock | Self::MacroDefinition => {}
            Self::Const(item) => item.record_memory_children(recorder),
            Self::Enum(item) => item.record_memory_children(recorder),
            Self::ExternCrate(item) => item.record_memory_children(recorder),
            Self::Function(item) => item.record_memory_children(recorder),
            Self::Impl(item) => item.record_memory_children(recorder),
            Self::Module(item) => item.record_memory_children(recorder),
            Self::Static(item) => item.record_memory_children(recorder),
            Self::Struct(item) => item.record_memory_children(recorder),
            Self::Trait(item) => item.record_memory_children(recorder),
            Self::TypeAlias(item) => item.record_memory_children(recorder),
            Self::Union(item) => item.record_memory_children(recorder),
            Self::Use(item) => item.record_memory_children(recorder),
        }
    }
}

impl MemorySize for ItemTag {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for ModuleItem {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, inner_docs, source);
    }
}

impl MemorySize for ModuleSource {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Inline { items } => items.record_memory_children(recorder),
            Self::OutOfLine { definition_file } => definition_file.record_memory_children(recorder),
        }
    }
}

impl MemorySize for VisibilityLevel {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Private | Self::Public | Self::Crate | Self::Super | Self::Self_ => {}
            Self::Restricted(path) | Self::Unknown(path) => path.record_memory_children(recorder),
        }
    }
}
