use std::fmt;

use ra_syntax::{
    AstNode as _,
    ast::{self, HasGenericArgs},
};

use super::normalized_syntax;

/// Syntax-level mutability marker used by lowered declarations and type refs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutability {
    Shared,
    Mutable,
}

impl Mutability {
    pub(crate) fn from_mut_token(is_mut: bool) -> Self {
        if is_mut { Self::Mutable } else { Self::Shared }
    }
}

impl fmt::Display for Mutability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shared => write!(f, "shared"),
            Self::Mutable => write!(f, "mut"),
        }
    }
}

/// Unresolved type syntax lowered into the item tree.
///
/// This intentionally stops before semantic resolution. `TypeRef` represents what the user wrote
/// in an item declaration; resolving paths to definitions belongs to later IR layers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    Unknown(String),
    Never,
    Unit,
    Infer,
    Path(TypePath),
    Tuple(Vec<TypeRef>),
    Reference {
        lifetime: Option<String>,
        mutability: Mutability,
        inner: Box<TypeRef>,
    },
    RawPointer {
        mutability: Mutability,
        inner: Box<TypeRef>,
    },
    Slice(Box<TypeRef>),
    Array {
        inner: Box<TypeRef>,
        len: Option<String>,
    },
    FnPointer {
        params: Vec<TypeRef>,
        ret: Box<TypeRef>,
    },
    ImplTrait(Vec<TypeBound>),
    DynTrait(Vec<TypeBound>),
}

impl TypeRef {
    pub(crate) fn unknown_from_text(text: impl Into<String>) -> Self {
        Self::Unknown(text.into())
    }

    pub(crate) fn from_ast(ty: ast::Type) -> Self {
        match ty {
            ast::Type::ArrayType(ty) => Self::Array {
                inner: Box::new(
                    ty.ty()
                        .map(Self::from_ast)
                        .unwrap_or_else(|| Self::unknown_from_text(normalized_syntax(&ty))),
                ),
                len: ty.const_arg().map(|arg| normalized_syntax(&arg)),
            },
            ast::Type::DynTraitType(ty) => {
                Self::DynTrait(TypeBound::list_from_ast(ty.type_bound_list()))
            }
            ast::Type::FnPtrType(ty) => Self::FnPointer {
                params: ty
                    .param_list()
                    .into_iter()
                    .flat_map(|param_list| param_list.params())
                    .map(|param| {
                        param
                            .ty()
                            .map(Self::from_ast)
                            .unwrap_or_else(|| Self::Unknown(String::new()))
                    })
                    .collect(),
                ret: Box::new(
                    ty.ret_type()
                        .and_then(|ret_ty| ret_ty.ty())
                        .map(Self::from_ast)
                        .unwrap_or(Self::Unit),
                ),
            },
            ast::Type::ForType(ty) => ty
                .ty()
                .map(Self::from_ast)
                .unwrap_or_else(|| Self::unknown_from_text(normalized_syntax(&ty))),
            ast::Type::ImplTraitType(ty) => {
                Self::ImplTrait(TypeBound::list_from_ast(ty.type_bound_list()))
            }
            ast::Type::InferType(_) => Self::Infer,
            ast::Type::MacroType(ty) => Self::unknown_from_text(normalized_syntax(&ty)),
            ast::Type::NeverType(_) => Self::Never,
            ast::Type::ParenType(ty) => ty
                .ty()
                .map(Self::from_ast)
                .unwrap_or_else(|| Self::unknown_from_text(normalized_syntax(&ty))),
            ast::Type::PathType(ty) => ty
                .path()
                .map(TypePath::from_ast)
                .map(Self::Path)
                .unwrap_or_else(|| Self::unknown_from_text(normalized_syntax(&ty))),
            ast::Type::PtrType(ty) => Self::RawPointer {
                mutability: Mutability::from_mut_token(ty.mut_token().is_some()),
                inner: Box::new(
                    ty.ty()
                        .map(Self::from_ast)
                        .unwrap_or_else(|| Self::unknown_from_text(normalized_syntax(&ty))),
                ),
            },
            ast::Type::RefType(ty) => Self::Reference {
                lifetime: ty.lifetime().map(|lifetime| normalized_syntax(&lifetime)),
                mutability: Mutability::from_mut_token(ty.mut_token().is_some()),
                inner: Box::new(
                    ty.ty()
                        .map(Self::from_ast)
                        .unwrap_or_else(|| Self::unknown_from_text(normalized_syntax(&ty))),
                ),
            },
            ast::Type::SliceType(ty) => Self::Slice(Box::new(
                ty.ty()
                    .map(Self::from_ast)
                    .unwrap_or_else(|| Self::unknown_from_text(normalized_syntax(&ty))),
            )),
            ast::Type::TupleType(ty) => {
                let fields = ty.fields().map(Self::from_ast).collect::<Vec<_>>();
                if fields.is_empty() {
                    Self::Unit
                } else {
                    Self::Tuple(fields)
                }
            }
        }
    }
}

impl fmt::Display for TypeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(text) if text.is_empty() => write!(f, "<unknown>"),
            Self::Unknown(text) => write!(f, "<unsupported:{text}>"),
            Self::Never => write!(f, "!"),
            Self::Unit => write!(f, "()"),
            Self::Infer => write!(f, "_"),
            Self::Path(path) => write!(f, "{path}"),
            Self::Tuple(types) => {
                write!(f, "(")?;
                for (idx, ty) in types.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{ty}")?;
                }
                if types.len() == 1 {
                    write!(f, ",")?;
                }
                write!(f, ")")
            }
            Self::Reference {
                lifetime,
                mutability,
                inner,
            } => {
                write!(f, "&")?;
                if let Some(lifetime) = lifetime {
                    write!(f, "{lifetime} ")?;
                }
                if matches!(mutability, Mutability::Mutable) {
                    write!(f, "mut ")?;
                }
                write!(f, "{inner}")
            }
            Self::RawPointer { mutability, inner } => match mutability {
                Mutability::Shared => write!(f, "*const {inner}"),
                Mutability::Mutable => write!(f, "*mut {inner}"),
            },
            Self::Slice(inner) => write!(f, "[{inner}]"),
            Self::Array { inner, len } => {
                write!(f, "[{inner}; ")?;
                match len {
                    Some(len) => write!(f, "{len}")?,
                    None => write!(f, "<unknown>")?,
                }
                write!(f, "]")
            }
            Self::FnPointer { params, ret } => {
                write!(f, "fn(")?;
                for (idx, param) in params.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{param}")?;
                }
                write!(f, ")")?;
                if !matches!(ret.as_ref(), TypeRef::Unit) {
                    write!(f, " -> {ret}")?;
                }
                Ok(())
            }
            Self::ImplTrait(bounds) => write_bounds(f, "impl ", bounds),
            Self::DynTrait(bounds) => write_bounds(f, "dyn ", bounds),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypePath {
    pub absolute: bool,
    pub segments: Vec<TypePathSegment>,
}

impl TypePath {
    pub(crate) fn from_ast(path: ast::Path) -> Self {
        let absolute = path
            .first_segment()
            .is_some_and(|segment| segment.coloncolon_token().is_some());
        let mut segments = Vec::new();
        Self::collect_segments(&path, &mut segments);

        Self { absolute, segments }
    }

    fn collect_segments(path: &ast::Path, segments: &mut Vec<TypePathSegment>) {
        if let Some(qualifier) = path.qualifier() {
            Self::collect_segments(&qualifier, segments);
        }

        if let Some(segment) = path.segment() {
            segments.push(TypePathSegment::from_ast(&segment));
        }
    }
}

impl fmt::Display for TypePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.absolute {
            write!(f, "::")?;
        }

        for (idx, segment) in self.segments.iter().enumerate() {
            if idx > 0 {
                write!(f, "::")?;
            }
            write!(f, "{segment}")?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypePathSegment {
    pub name: String,
    pub args: Vec<GenericArg>,
}

impl TypePathSegment {
    fn from_ast(segment: &ast::PathSegment) -> Self {
        let name = segment
            .name_ref()
            .map(|name| name.syntax().text().to_string())
            .unwrap_or_else(|| normalized_syntax(segment));
        let mut args = Vec::new();

        if let Some(arg_list) = segment.generic_arg_list() {
            args.extend(arg_list.generic_args().map(GenericArg::from_ast));
        }

        if let Some(parenthesized_args) = segment.parenthesized_arg_list() {
            args.push(GenericArg::Unsupported(normalized_syntax(
                &parenthesized_args,
            )));
        }

        Self { name, args }
    }
}

impl fmt::Display for TypePathSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if !self.args.is_empty() {
            write!(f, "<")?;
            for (idx, arg) in self.args.iter().enumerate() {
                if idx > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{arg}")?;
            }
            write!(f, ">")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenericArg {
    Type(TypeRef),
    Lifetime(String),
    Const(String),
    AssocType { name: String, ty: Option<TypeRef> },
    Unsupported(String),
}

impl GenericArg {
    fn from_ast(arg: ast::GenericArg) -> Self {
        match arg {
            ast::GenericArg::AssocTypeArg(arg) => Self::AssocType {
                name: arg
                    .name_ref()
                    .map(|name| name.syntax().text().to_string())
                    .unwrap_or_else(|| "<missing>".to_string()),
                ty: arg.ty().map(TypeRef::from_ast),
            },
            ast::GenericArg::ConstArg(arg) => Self::Const(normalized_syntax(&arg)),
            ast::GenericArg::LifetimeArg(arg) => arg
                .lifetime()
                .map(|lifetime| Self::Lifetime(normalized_syntax(&lifetime)))
                .unwrap_or_else(|| Self::Unsupported(normalized_syntax(&arg))),
            ast::GenericArg::TypeArg(arg) => arg
                .ty()
                .map(TypeRef::from_ast)
                .map(Self::Type)
                .unwrap_or_else(|| Self::Unsupported(normalized_syntax(&arg))),
        }
    }
}

impl fmt::Display for GenericArg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Type(ty) => write!(f, "{ty}"),
            Self::Lifetime(lifetime) => write!(f, "{lifetime}"),
            Self::Const(value) => write!(f, "{value}"),
            Self::AssocType { name, ty } => match ty {
                Some(ty) => write!(f, "{name} = {ty}"),
                None => write!(f, "{name}"),
            },
            Self::Unsupported(text) => write!(f, "<unsupported:{text}>"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeBound {
    Trait(TypeRef),
    Lifetime(String),
    Unsupported(String),
}

impl TypeBound {
    pub(crate) fn list_from_ast(bound_list: Option<ast::TypeBoundList>) -> Vec<Self> {
        bound_list
            .into_iter()
            .flat_map(|bound_list| bound_list.bounds())
            .map(Self::from_ast)
            .collect()
    }

    fn from_ast(bound: ast::TypeBound) -> Self {
        if let Some(lifetime) = bound.lifetime() {
            return Self::Lifetime(normalized_syntax(&lifetime));
        }

        if let Some(ty) = bound.ty() {
            return Self::Trait(TypeRef::from_ast(ty));
        }

        Self::Unsupported(normalized_syntax(&bound))
    }
}

impl fmt::Display for TypeBound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trait(ty) => write!(f, "{ty}"),
            Self::Lifetime(lifetime) => write!(f, "{lifetime}"),
            Self::Unsupported(text) => write!(f, "<unsupported:{text}>"),
        }
    }
}

fn write_bounds(f: &mut fmt::Formatter<'_>, prefix: &str, bounds: &[TypeBound]) -> fmt::Result {
    write!(f, "{prefix}")?;
    for (idx, bound) in bounds.iter().enumerate() {
        if idx > 0 {
            write!(f, " + ")?;
        }
        write!(f, "{bound}")?;
    }
    Ok(())
}
