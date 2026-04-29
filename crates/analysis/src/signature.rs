//! Compact Rust-ish declaration labels for hover and related UI surfaces.
//!
//! The renderer deliberately stays syntactic. It formats the declaration facts our IR already
//! stores instead of trying to reconstruct rustc-perfect signatures.

use rg_body_ir::{BindingData, BodyFieldData, BodyFunctionData, BodyItemData, BodyTy};
use rg_semantic_ir::{
    ConstData, ConstItem, EnumData, EnumVariantData, FieldData, FieldItem, FieldKey, FunctionData,
    FunctionItem, GenericParams, Mutability, ParamItem, StaticData, StructData, TraitData,
    TypeAliasData, TypeAliasItem, TypeBound, TypeRef, UnionData, VisibilityLevel, WherePredicate,
};

use super::{Analysis, type_render::TypeRenderer};

pub(super) struct SignatureRenderer<'a, 'db>(&'a Analysis<'db>);

impl<'a, 'db> SignatureRenderer<'a, 'db> {
    pub(super) fn new(analysis: &'a Analysis<'db>) -> Self {
        Self(analysis)
    }

    pub(super) fn struct_signature(&self, data: &StructData) -> String {
        format!(
            "{}struct {}{}{}",
            visibility_prefix(&data.visibility),
            data.name,
            generic_params(&data.generics),
            where_clause(&data.generics)
        )
    }

    pub(super) fn union_signature(&self, data: &UnionData) -> String {
        format!(
            "{}union {}{}{}",
            visibility_prefix(&data.visibility),
            data.name,
            generic_params(&data.generics),
            where_clause(&data.generics)
        )
    }

    pub(super) fn enum_signature(&self, data: &EnumData) -> String {
        format!(
            "{}enum {}{}{}",
            visibility_prefix(&data.visibility),
            data.name,
            generic_params(&data.generics),
            where_clause(&data.generics)
        )
    }

    pub(super) fn trait_signature(&self, data: &TraitData) -> String {
        let unsafe_prefix = data.is_unsafe.then_some("unsafe ").unwrap_or_default();
        let super_traits = if data.super_traits.is_empty() {
            String::new()
        } else {
            format!(
                ": {}",
                data.super_traits
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" + ")
            )
        };
        format!(
            "{}{unsafe_prefix}trait {}{}{}{}",
            visibility_prefix(&data.visibility),
            data.name,
            generic_params(&data.generics),
            super_traits,
            where_clause(&data.generics)
        )
    }

    pub(super) fn function_signature(&self, data: &FunctionData) -> String {
        format!(
            "{}{}",
            visibility_prefix(&data.visibility),
            function_signature(&data.name, &data.declaration)
        )
    }

    pub(super) fn type_alias_signature(&self, data: &TypeAliasData) -> String {
        format!(
            "{}{}",
            visibility_prefix(&data.visibility),
            type_alias_signature(&data.name, &data.declaration)
        )
    }

    pub(super) fn const_signature(&self, data: &ConstData) -> String {
        format!(
            "{}{}",
            visibility_prefix(&data.visibility),
            const_signature(&data.name, &data.declaration)
        )
    }

    pub(super) fn static_signature(&self, data: &StaticData) -> String {
        format!(
            "{}{}",
            visibility_prefix(&data.visibility),
            static_signature(&data.name, data.mutability, data.ty.as_ref())
        )
    }

    pub(super) fn field_signature(&self, data: FieldData<'_>) -> Option<String> {
        field_signature(data.field)
    }

    pub(super) fn enum_variant_signature(&self, data: EnumVariantData<'_>) -> String {
        let owner = self
            .0
            .semantic_ir
            .type_def_name(data.owner)
            .unwrap_or("<enum>");
        format!("variant {owner}::{}", data.variant.name)
    }

    pub(super) fn local_item_signature(&self, data: &BodyItemData) -> String {
        format!(
            "{} {}{}{}",
            data.kind,
            data.name,
            generic_params(&data.generics),
            where_clause(&data.generics)
        )
    }

    pub(super) fn local_function_signature(&self, data: &BodyFunctionData) -> String {
        function_signature(&data.name, &data.declaration)
    }

    pub(super) fn local_field_signature(&self, data: BodyFieldData<'_>) -> Option<String> {
        field_signature(data.field)
    }

    pub(super) fn binding_signature(&self, data: &BindingData) -> String {
        let name = data.name.as_deref().unwrap_or(data.pat.as_str());
        let ty = TypeRenderer::new(self.0)
            .render(&data.ty)
            .or_else(|| data.annotation.as_ref().map(ToString::to_string))
            .unwrap_or_else(|| "_".to_string());

        format!("let {name}: {ty}")
    }

    pub(super) fn ty_signature(&self, ty: &BodyTy) -> Option<String> {
        TypeRenderer::new(self.0).render(ty)
    }
}

fn visibility_prefix(visibility: &VisibilityLevel) -> String {
    if matches!(visibility, VisibilityLevel::Private) {
        String::new()
    } else {
        format!("{visibility} ")
    }
}

fn function_signature(name: &str, item: &FunctionItem) -> String {
    let mut signature = String::new();
    if item.qualifiers.is_const {
        signature.push_str("const ");
    }
    if item.qualifiers.is_unsafe {
        signature.push_str("unsafe ");
    }
    if item.qualifiers.is_async {
        signature.push_str("async ");
    }

    signature.push_str("fn ");
    signature.push_str(name);
    signature.push_str(&generic_params(&item.generics));
    signature.push('(');
    signature.push_str(
        &item
            .params
            .iter()
            .map(param_signature)
            .collect::<Vec<_>>()
            .join(", "),
    );
    signature.push(')');
    if let Some(ret_ty) = &item.ret_ty
        && !matches!(ret_ty, TypeRef::Unit)
    {
        signature.push_str(" -> ");
        signature.push_str(&ret_ty.to_string());
    }
    signature.push_str(&where_clause(&item.generics));

    signature
}

fn param_signature(param: &ParamItem) -> String {
    match &param.ty {
        Some(ty) => format!("{}: {ty}", param.pat),
        None => param.pat.clone(),
    }
}

fn type_alias_signature(name: &str, item: &TypeAliasItem) -> String {
    let mut signature = format!(
        "type {name}{}{}",
        generic_params(&item.generics),
        where_clause(&item.generics)
    );
    if let Some(ty) = &item.aliased_ty {
        signature.push_str(" = ");
        signature.push_str(&ty.to_string());
    }
    signature
}

fn const_signature(name: &str, item: &ConstItem) -> String {
    match &item.ty {
        Some(ty) => format!("const {name}: {ty}"),
        None => format!("const {name}: _"),
    }
}

fn static_signature(name: &str, mutability: Mutability, ty: Option<&TypeRef>) -> String {
    let mut_prefix = matches!(mutability, Mutability::Mutable)
        .then_some("mut ")
        .unwrap_or_default();
    match ty {
        Some(ty) => format!("static {mut_prefix}{name}: {ty}"),
        None => format!("static {mut_prefix}{name}: _"),
    }
}

fn field_signature(field: &FieldItem) -> Option<String> {
    let key = field.key.as_ref()?;
    let label = match key {
        FieldKey::Named(name) => name.clone(),
        FieldKey::Tuple(index) => index.to_string(),
    };
    Some(format!(
        "{}{}: {}",
        visibility_prefix(&field.visibility),
        label,
        field.ty
    ))
}

fn generic_params(generics: &GenericParams) -> String {
    let mut params = Vec::new();

    params.extend(generics.lifetimes.iter().map(|param| {
        if param.bounds.is_empty() {
            param.name.clone()
        } else {
            format!("{}: {}", param.name, param.bounds.join(" + "))
        }
    }));
    params.extend(generics.types.iter().map(|param| {
        let mut text = param.name.clone();
        if !param.bounds.is_empty() {
            text.push_str(": ");
            text.push_str(&type_bounds(&param.bounds));
        }
        if let Some(default) = &param.default {
            text.push_str(" = ");
            text.push_str(&default.to_string());
        }
        text
    }));
    params.extend(generics.consts.iter().map(|param| {
        let mut text = format!("const {}", param.name);
        if let Some(ty) = &param.ty {
            text.push_str(": ");
            text.push_str(&ty.to_string());
        }
        if let Some(default) = &param.default {
            text.push_str(" = ");
            text.push_str(default);
        }
        text
    }));

    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

fn where_clause(generics: &GenericParams) -> String {
    if generics.where_predicates.is_empty() {
        return String::new();
    }

    format!(
        " where {}",
        generics
            .where_predicates
            .iter()
            .map(where_predicate)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn where_predicate(predicate: &WherePredicate) -> String {
    match predicate {
        WherePredicate::Type { ty, bounds } => {
            if bounds.is_empty() {
                ty.to_string()
            } else {
                format!("{ty}: {}", type_bounds(bounds))
            }
        }
        WherePredicate::Lifetime { lifetime, bounds } => {
            format!("{lifetime}: {}", bounds.join(" + "))
        }
        WherePredicate::Unsupported(text) => format!("<unsupported:{text}>"),
    }
}

fn type_bounds(bounds: &[TypeBound]) -> String {
    bounds
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" + ")
}
