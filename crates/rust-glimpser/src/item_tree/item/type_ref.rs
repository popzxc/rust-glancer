use std::fmt;

/// Syntax-level mutability marker used by lowered declarations and type refs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutability {
    Shared,
    Mutable,
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
