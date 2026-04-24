use std::fmt;

use super::{ItemTreeId, Mutability, TypeBound, TypeRef, VisibilityLevel};

/// Generic parameter data attached to an item declaration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GenericParams {
    pub lifetimes: Vec<LifetimeParamData>,
    pub types: Vec<TypeParamData>,
    pub consts: Vec<ConstParamData>,
    pub where_predicates: Vec<WherePredicate>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionItem {
    pub generics: GenericParams,
    pub params: Vec<ParamItem>,
    pub ret_ty: Option<TypeRef>,
    pub qualifiers: FunctionQualifiers,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnionItem {
    pub generics: GenericParams,
    pub fields: Vec<FieldItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumItem {
    pub generics: GenericParams,
    pub variants: Vec<EnumVariantItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariantItem {
    pub name: String,
    pub fields: FieldList,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldList {
    Named(Vec<FieldItem>),
    Tuple(Vec<FieldItem>),
    Unit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldItem {
    pub name: Option<String>,
    pub visibility: VisibilityLevel,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitItem {
    pub generics: GenericParams,
    pub super_traits: Vec<TypeBound>,
    pub items: Vec<ItemTreeId>,
    pub is_unsafe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplItem {
    pub generics: GenericParams,
    pub trait_ref: Option<TypeRef>,
    pub self_ty: TypeRef,
    pub items: Vec<ItemTreeId>,
    pub is_unsafe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasItem {
    pub generics: GenericParams,
    pub bounds: Vec<TypeBound>,
    pub aliased_ty: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstItem {
    pub generics: GenericParams,
    pub ty: Option<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticItem {
    pub ty: Option<TypeRef>,
    pub mutability: Mutability,
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
