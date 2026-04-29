use rg_item_tree::TypeRef;
use rg_semantic_ir::TypeDefRef;

use crate::ids::BodyItemRef;

/// Small type vocabulary for the first Body IR pass.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BodyTy {
    Unit,
    Never,
    Syntax(TypeRef),
    Reference(Box<BodyTy>),
    LocalNominal(Vec<BodyLocalNominalTy>),
    Nominal(Vec<BodyNominalTy>),
    SelfTy(Vec<BodyNominalTy>),
    #[default]
    Unknown,
}

/// Body-local nominal type together with the generic arguments visible at use site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyLocalNominalTy {
    pub item: BodyItemRef,
    pub args: Vec<BodyGenericArg>,
}

impl BodyLocalNominalTy {
    pub fn bare(item: BodyItemRef) -> Self {
        Self {
            item,
            args: Vec::new(),
        }
    }
}

/// Module-level nominal type together with the generic arguments visible at use site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyNominalTy {
    pub def: TypeDefRef,
    pub args: Vec<BodyGenericArg>,
}

impl BodyNominalTy {
    pub fn bare(def: TypeDefRef) -> Self {
        Self {
            def,
            args: Vec::new(),
        }
    }
}

/// Generic argument as understood by the intentionally small Body IR type model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyGenericArg {
    Type(Box<BodyTy>),
    Lifetime(String),
    Const(String),
    AssocType {
        name: String,
        ty: Option<Box<BodyTy>>,
    },
    Unsupported(String),
}

impl BodyTy {
    pub fn reference(inner: BodyTy) -> Self {
        if matches!(inner, Self::Unknown) {
            return Self::Unknown;
        }

        Self::Reference(Box::new(inner))
    }

    pub fn peel_references(&self) -> &Self {
        match self {
            Self::Reference(inner) => inner.peel_references(),
            Self::Unit
            | Self::Never
            | Self::Syntax(_)
            | Self::LocalNominal(_)
            | Self::Nominal(_)
            | Self::SelfTy(_)
            | Self::Unknown => self,
        }
    }

    pub fn local_nominals(&self) -> &[BodyLocalNominalTy] {
        match self.peel_references() {
            Self::LocalNominal(types) => types,
            Self::Unit
            | Self::Never
            | Self::Syntax(_)
            | Self::Reference(_)
            | Self::Nominal(_)
            | Self::SelfTy(_)
            | Self::Unknown => &[],
        }
    }

    pub fn nominal_tys(&self) -> &[BodyNominalTy] {
        match self.peel_references() {
            Self::Nominal(types) | Self::SelfTy(types) => types,
            Self::Unit
            | Self::Never
            | Self::Syntax(_)
            | Self::Reference(_)
            | Self::LocalNominal(_)
            | Self::Unknown => &[],
        }
    }

    pub fn local_items(&self) -> Vec<BodyItemRef> {
        self.local_nominals().iter().map(|ty| ty.item).collect()
    }

    pub fn type_defs(&self) -> Vec<TypeDefRef> {
        self.nominal_tys().iter().map(|ty| ty.def).collect()
    }
}
