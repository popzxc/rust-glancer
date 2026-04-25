use std::fmt;

use ra_syntax::{
    AstNode as _,
    ast::{self, HasGenericParams, HasName, HasTypeBounds, HasVisibility},
};

use super::{ItemTreeId, Mutability, TypeBound, TypeRef, VisibilityLevel, normalized_syntax};

/// Generic parameter data attached to an item declaration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GenericParams {
    pub lifetimes: Vec<LifetimeParamData>,
    pub types: Vec<TypeParamData>,
    pub consts: Vec<ConstParamData>,
    pub where_predicates: Vec<WherePredicate>,
}

impl GenericParams {
    pub(crate) fn from_ast<T>(item: &T) -> Self
    where
        T: HasGenericParams,
    {
        let mut params = Self::default();

        if let Some(param_list) = item.generic_param_list() {
            for param in param_list.generic_params() {
                match param {
                    ast::GenericParam::ConstParam(param) => {
                        params.consts.push(ConstParamData {
                            name: param
                                .name()
                                .map(|name| name.text().to_string())
                                .unwrap_or_else(|| "<missing>".to_string()),
                            ty: param.ty().map(TypeRef::from_ast),
                            default: param.default_val().map(|value| normalized_syntax(&value)),
                        });
                    }
                    ast::GenericParam::LifetimeParam(param) => {
                        params.lifetimes.push(LifetimeParamData {
                            name: param
                                .lifetime()
                                .map(|lifetime| normalized_syntax(&lifetime))
                                .unwrap_or_else(|| "<missing>".to_string()),
                            bounds: lifetime_bounds_from_ast(param.type_bound_list()),
                        });
                    }
                    ast::GenericParam::TypeParam(param) => {
                        params.types.push(TypeParamData {
                            name: param
                                .name()
                                .map(|name| name.text().to_string())
                                .unwrap_or_else(|| "<missing>".to_string()),
                            bounds: TypeBound::list_from_ast(param.type_bound_list()),
                            default: param.default_type().map(TypeRef::from_ast),
                        });
                    }
                }
            }
        }

        if let Some(where_clause) = item.where_clause() {
            params.where_predicates = where_clause
                .predicates()
                .map(WherePredicate::from_ast)
                .collect();
        }

        params
    }
}

impl fmt::Display for GenericParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut params = Vec::new();

        params.extend(self.lifetimes.iter().map(|param| {
            if param.bounds.is_empty() {
                param.name.clone()
            } else {
                format!("{}: {}", param.name, param.bounds.join(" + "))
            }
        }));
        params.extend(self.types.iter().map(|param| {
            let mut text = param.name.clone();
            if !param.bounds.is_empty() {
                text.push_str(": ");
                text.push_str(
                    &param
                        .bounds
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" + "),
                );
            }
            if let Some(default) = &param.default {
                text.push_str(" = ");
                text.push_str(&default.to_string());
            }
            text
        }));
        params.extend(self.consts.iter().map(|param| {
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

        if !params.is_empty() {
            write!(f, "<{}>", params.join(", "))?;
        }

        if !self.where_predicates.is_empty() {
            write!(f, " where ")?;
            for (idx, predicate) in self.where_predicates.iter().enumerate() {
                if idx > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{predicate}")?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifetimeParamData {
    pub name: String,
    pub bounds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParamData {
    pub name: String,
    pub bounds: Vec<TypeBound>,
    pub default: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstParamData {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WherePredicate {
    Type {
        ty: TypeRef,
        bounds: Vec<TypeBound>,
    },
    Lifetime {
        lifetime: String,
        bounds: Vec<String>,
    },
    Unsupported(String),
}

impl WherePredicate {
    fn from_ast(predicate: ast::WherePred) -> Self {
        if let Some(lifetime) = predicate.lifetime() {
            return Self::Lifetime {
                lifetime: normalized_syntax(&lifetime),
                bounds: lifetime_bounds_from_ast(predicate.type_bound_list()),
            };
        }

        if let Some(ty) = predicate.ty() {
            return Self::Type {
                ty: TypeRef::from_ast(ty),
                bounds: TypeBound::list_from_ast(predicate.type_bound_list()),
            };
        }

        Self::Unsupported(normalized_syntax(&predicate))
    }
}

impl fmt::Display for WherePredicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Type { ty, bounds } => write_bound_list(f, &ty.to_string(), bounds),
            Self::Lifetime { lifetime, bounds } => {
                write!(f, "{lifetime}: {}", bounds.join(" + "))
            }
            Self::Unsupported(text) => write!(f, "<unsupported:{text}>"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionItem {
    pub generics: GenericParams,
    pub params: Vec<ParamItem>,
    pub ret_ty: Option<TypeRef>,
    pub qualifiers: FunctionQualifiers,
}

impl FunctionItem {
    pub(crate) fn from_ast(item: &ast::Fn) -> Self {
        Self {
            generics: GenericParams::from_ast(item),
            params: ParamItem::list_from_ast(item.param_list()),
            ret_ty: item
                .ret_type()
                .and_then(|ret_ty| ret_ty.ty())
                .map(TypeRef::from_ast),
            qualifiers: FunctionQualifiers {
                is_async: item.async_token().is_some(),
                is_const: item.const_token().is_some(),
                is_unsafe: item.unsafe_token().is_some(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FunctionQualifiers {
    pub is_async: bool,
    pub is_const: bool,
    pub is_unsafe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamItem {
    pub pat: String,
    pub ty: Option<TypeRef>,
    pub kind: ParamKind,
}

impl ParamItem {
    pub(crate) fn list_from_ast(param_list: Option<ast::ParamList>) -> Vec<Self> {
        let Some(param_list) = param_list else {
            return Vec::new();
        };

        let mut params = Vec::new();

        if let Some(self_param) = param_list.self_param() {
            params.push(Self {
                pat: normalized_syntax(&self_param),
                ty: self_param.ty().map(TypeRef::from_ast),
                kind: ParamKind::SelfParam,
            });
        }

        for param in param_list.params() {
            params.push(Self {
                pat: param
                    .pat()
                    .map(|pat| normalized_syntax(&pat))
                    .unwrap_or_else(|| "<missing>".to_string()),
                ty: param.ty().map(TypeRef::from_ast),
                kind: ParamKind::Normal,
            });
        }

        params
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    SelfParam,
    Normal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructItem {
    pub generics: GenericParams,
    pub fields: FieldList,
}

impl StructItem {
    pub(crate) fn from_ast(item: &ast::Struct) -> Self {
        Self {
            generics: GenericParams::from_ast(item),
            fields: FieldList::from_ast(item.field_list()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnionItem {
    pub generics: GenericParams,
    pub fields: Vec<FieldItem>,
}

impl UnionItem {
    pub(crate) fn from_ast(item: &ast::Union) -> Self {
        Self {
            generics: GenericParams::from_ast(item),
            fields: item
                .record_field_list()
                .map(FieldItem::record_list_from_ast)
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumItem {
    pub generics: GenericParams,
    pub variants: Vec<EnumVariantItem>,
}

impl EnumItem {
    pub(crate) fn from_ast(item: &ast::Enum) -> Self {
        Self {
            generics: GenericParams::from_ast(item),
            variants: item
                .variant_list()
                .map(|variant_list| {
                    variant_list
                        .variants()
                        .map(EnumVariantItem::from_ast)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariantItem {
    pub name: String,
    pub fields: FieldList,
}

impl EnumVariantItem {
    fn from_ast(variant: ast::Variant) -> Self {
        Self {
            name: variant
                .name()
                .map(|name| name.text().to_string())
                .unwrap_or_else(|| "<missing>".to_string()),
            fields: FieldList::from_ast(variant.field_list()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldList {
    Named(Vec<FieldItem>),
    Tuple(Vec<FieldItem>),
    Unit,
}

impl FieldList {
    pub(crate) fn from_ast(field_list: Option<ast::FieldList>) -> Self {
        match field_list {
            Some(ast::FieldList::RecordFieldList(fields)) => {
                Self::Named(FieldItem::record_list_from_ast(fields))
            }
            Some(ast::FieldList::TupleFieldList(fields)) => {
                Self::Tuple(FieldItem::tuple_list_from_ast(fields))
            }
            None => Self::Unit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldItem {
    pub name: Option<String>,
    pub visibility: VisibilityLevel,
    pub ty: TypeRef,
}

impl FieldItem {
    fn record_list_from_ast(fields: ast::RecordFieldList) -> Vec<Self> {
        fields
            .fields()
            .map(|field| Self {
                name: field.name().map(|name| name.text().to_string()),
                visibility: VisibilityLevel::from_ast(field.visibility()),
                ty: field
                    .ty()
                    .map(TypeRef::from_ast)
                    .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(&field))),
            })
            .collect()
    }

    fn tuple_list_from_ast(fields: ast::TupleFieldList) -> Vec<Self> {
        fields
            .fields()
            .map(|field| Self {
                name: None,
                visibility: VisibilityLevel::from_ast(field.visibility()),
                ty: field
                    .ty()
                    .map(TypeRef::from_ast)
                    .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(&field))),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitItem {
    pub generics: GenericParams,
    pub super_traits: Vec<TypeBound>,
    pub items: Vec<ItemTreeId>,
    pub is_unsafe: bool,
}

impl TraitItem {
    pub(crate) fn from_ast(item: &ast::Trait, items: Vec<ItemTreeId>) -> Self {
        Self {
            generics: GenericParams::from_ast(item),
            super_traits: TypeBound::list_from_ast(item.type_bound_list()),
            items,
            is_unsafe: item.unsafe_token().is_some(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplItem {
    pub generics: GenericParams,
    pub trait_ref: Option<TypeRef>,
    pub self_ty: TypeRef,
    pub items: Vec<ItemTreeId>,
    pub is_unsafe: bool,
}

impl ImplItem {
    pub(crate) fn from_ast(item: &ast::Impl, items: Vec<ItemTreeId>) -> Self {
        let (trait_ref, self_ty) = Self::header_from_ast(item);

        Self {
            generics: GenericParams::from_ast(item),
            trait_ref,
            self_ty,
            items,
            is_unsafe: item.unsafe_token().is_some(),
        }
    }

    fn header_from_ast(item: &ast::Impl) -> (Option<TypeRef>, TypeRef) {
        let types = item
            .syntax()
            .children()
            .filter_map(ast::Type::cast)
            .collect::<Vec<_>>();

        if item.for_token().is_some() {
            let trait_ref = types.first().cloned().map(TypeRef::from_ast);
            let self_ty = types
                .get(1)
                .cloned()
                .map(TypeRef::from_ast)
                .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(item)));
            return (trait_ref, self_ty);
        }

        let self_ty = types
            .first()
            .cloned()
            .map(TypeRef::from_ast)
            .unwrap_or_else(|| TypeRef::unknown_from_text(normalized_syntax(item)));
        (None, self_ty)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasItem {
    pub generics: GenericParams,
    pub bounds: Vec<TypeBound>,
    pub aliased_ty: Option<TypeRef>,
}

impl TypeAliasItem {
    pub(crate) fn from_ast(item: &ast::TypeAlias) -> Self {
        Self {
            generics: GenericParams::from_ast(item),
            bounds: TypeBound::list_from_ast(item.type_bound_list()),
            aliased_ty: item.ty().map(TypeRef::from_ast),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstItem {
    pub generics: GenericParams,
    pub ty: Option<TypeRef>,
}

impl ConstItem {
    pub(crate) fn from_ast(item: &ast::Const) -> Self {
        Self {
            generics: GenericParams::from_ast(item),
            ty: item.ty().map(TypeRef::from_ast),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticItem {
    pub ty: Option<TypeRef>,
    pub mutability: Mutability,
}

impl StaticItem {
    pub(crate) fn from_ast(item: &ast::Static) -> Self {
        Self {
            ty: item.ty().map(TypeRef::from_ast),
            mutability: Mutability::from_mut_token(item.mut_token().is_some()),
        }
    }
}

fn lifetime_bounds_from_ast(bound_list: Option<ast::TypeBoundList>) -> Vec<String> {
    bound_list
        .into_iter()
        .flat_map(|bound_list| bound_list.bounds())
        .filter_map(|bound| {
            bound
                .lifetime()
                .map(|lifetime| normalized_syntax(&lifetime))
        })
        .collect()
}

fn write_bound_list(
    f: &mut fmt::Formatter<'_>,
    subject: &str,
    bounds: &[TypeBound],
) -> fmt::Result {
    write!(f, "{subject}")?;
    if !bounds.is_empty() {
        write!(f, ": ")?;
        for (idx, bound) in bounds.iter().enumerate() {
            if idx > 0 {
                write!(f, " + ")?;
            }
            write!(f, "{bound}")?;
        }
    }
    Ok(())
}
