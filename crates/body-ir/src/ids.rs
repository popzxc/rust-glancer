use rg_def_map::TargetRef;

/// Stable identifier for one lowered function body inside a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyId(pub usize);

/// Stable reference to one lowered function body across the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyRef {
    pub target: TargetRef,
    pub body: BodyId,
}

/// Stable identifier for one item declared inside a function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyItemId(pub usize);

/// Stable reference to one item declared inside a function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyItemRef {
    pub body: BodyRef,
    pub item: BodyItemId,
}

/// Stable identifier for one expression inside a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprId(pub usize);

/// Stable identifier for one statement inside a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StmtId(pub usize);

/// Stable identifier for one local binding inside a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingId(pub usize);

/// Stable identifier for one lexical scope inside a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub usize);
