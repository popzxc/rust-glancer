use crate::{
    def_map::{DefId, ModuleRef, PackageSlot, TargetRef},
    item_tree::TypeRef,
    parse::{FileId, TargetId, span::Span},
    semantic_ir::{FunctionId, FunctionRef, TypeDefRef},
};

use super::{
    ids::{BindingId, BodyId, BodyRef, ExprId, ScopeId, StmtId},
    lower, resolution,
};

/// Body-level IR for all analyzed packages and targets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BodyIrDb {
    packages: Vec<PackageBodies>,
}

impl BodyIrDb {
    pub(crate) fn build(
        parse: &crate::parse::ParseDb,
        item_tree: &crate::item_tree::ItemTreeDb,
        def_map: &crate::def_map::DefMapDb,
        semantic_ir: &crate::semantic_ir::SemanticIrDb,
    ) -> anyhow::Result<Self> {
        let mut db = lower::build_db(parse, item_tree, semantic_ir)?;
        resolution::resolve_bodies(&mut db, def_map, semantic_ir);
        Ok(db)
    }

    pub(crate) fn new(packages: Vec<PackageBodies>) -> Self {
        Self { packages }
    }

    pub(crate) fn stats(&self) -> BodyIrStats {
        let mut stats = BodyIrStats::default();

        for package in &self.packages {
            for target in package.targets() {
                stats.target_count += 1;
                stats.body_count += target.bodies.len();
                for body in target.bodies() {
                    stats.scope_count += body.scopes.len();
                    stats.binding_count += body.bindings.len();
                    stats.statement_count += body.statements.len();
                    stats.expression_count += body.exprs.len();
                }
            }
        }

        stats
    }
}

#[allow(dead_code)]
impl BodyIrDb {
    /// Returns all package-level body IR sets.
    pub(crate) fn packages(&self) -> &[PackageBodies] {
        &self.packages
    }

    /// Returns one package by package slot.
    pub(crate) fn package(&self, package: PackageSlot) -> Option<&PackageBodies> {
        self.packages.get(package.0)
    }

    /// Returns one target body IR by project-wide target reference.
    pub(crate) fn target_bodies(&self, target: TargetRef) -> Option<&TargetBodies> {
        self.package(target.package)?.target(target.target)
    }

    /// Returns the body associated with a semantic function, if that function has a body.
    pub(crate) fn body_for_function(&self, function: FunctionRef) -> Option<BodyRef> {
        let body = self
            .target_bodies(function.target)?
            .body_for_function(function.id)?;
        Some(BodyRef {
            target: function.target,
            body,
        })
    }

    /// Returns one body by project-wide body reference.
    pub(crate) fn body_data(&self, body_ref: BodyRef) -> Option<&BodyData> {
        self.target_bodies(body_ref.target)?.body(body_ref.body)
    }
}

impl BodyIrDb {
    pub(super) fn packages_mut(&mut self) -> &mut [PackageBodies] {
        &mut self.packages
    }
}

/// Coarse totals for reporting that the Body IR phase produced useful data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct BodyIrStats {
    pub(crate) target_count: usize,
    pub(crate) body_count: usize,
    pub(crate) scope_count: usize,
    pub(crate) binding_count: usize,
    pub(crate) statement_count: usize,
    pub(crate) expression_count: usize,
}

/// Lowered bodies for all targets inside one parsed package.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageBodies {
    targets: Vec<TargetBodies>,
}

impl PackageBodies {
    pub(crate) fn new(targets: Vec<TargetBodies>) -> Self {
        Self { targets }
    }

    pub(crate) fn targets(&self) -> &[TargetBodies] {
        &self.targets
    }

    pub(crate) fn target(&self, target: TargetId) -> Option<&TargetBodies> {
        self.targets.get(target.0)
    }
}

impl PackageBodies {
    pub(super) fn targets_mut(&mut self) -> &mut [TargetBodies] {
        &mut self.targets
    }
}

/// Lowered bodies for one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetBodies {
    function_bodies: Vec<Option<BodyId>>,
    bodies: Vec<BodyData>,
}

impl TargetBodies {
    pub(crate) fn new(function_count: usize) -> Self {
        Self {
            function_bodies: vec![None; function_count],
            bodies: Vec::new(),
        }
    }

    pub(crate) fn body_for_function(&self, function: FunctionId) -> Option<BodyId> {
        self.function_bodies.get(function.0).copied().flatten()
    }

    pub(crate) fn body(&self, body: BodyId) -> Option<&BodyData> {
        self.bodies.get(body.0)
    }

    pub(crate) fn bodies(&self) -> &[BodyData] {
        &self.bodies
    }
}

impl TargetBodies {
    pub(super) fn alloc_body(&mut self, data: BodyData) -> BodyId {
        let body = BodyId(self.bodies.len());
        self.bodies.push(data);
        body
    }

    pub(super) fn set_function_body(&mut self, function: FunctionId, body: BodyId) {
        let slot = self
            .function_bodies
            .get_mut(function.0)
            .expect("function body slot should exist while building body IR");
        *slot = Some(body);
    }

    pub(super) fn bodies_mut(&mut self) -> &mut [BodyData] {
        &mut self.bodies
    }
}

/// Lowered body for one function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyData {
    pub owner: FunctionRef,
    pub owner_module: ModuleRef,
    pub source: BodySource,
    pub param_scope: ScopeId,
    pub root_expr: ExprId,
    pub params: Vec<BindingId>,
    pub scopes: Vec<ScopeData>,
    pub bindings: Vec<BindingData>,
    pub statements: Vec<StmtData>,
    pub exprs: Vec<ExprData>,
}

#[allow(dead_code)]
impl BodyData {
    pub(crate) fn binding(&self, binding: BindingId) -> Option<&BindingData> {
        self.bindings.get(binding.0)
    }

    pub(crate) fn scope(&self, scope: ScopeId) -> Option<&ScopeData> {
        self.scopes.get(scope.0)
    }

    pub(crate) fn statement(&self, statement: StmtId) -> Option<&StmtData> {
        self.statements.get(statement.0)
    }

    pub(crate) fn expr(&self, expr: ExprId) -> Option<&ExprData> {
        self.exprs.get(expr.0)
    }
}

impl BodyData {
    pub(super) fn new(
        owner: FunctionRef,
        owner_module: ModuleRef,
        source: BodySource,
        param_scope: ScopeId,
        root_expr: ExprId,
        params: Vec<BindingId>,
        builder: BodyBuilder,
    ) -> Self {
        Self {
            owner,
            owner_module,
            source,
            param_scope,
            root_expr,
            params,
            scopes: builder.scopes,
            bindings: builder.bindings,
            statements: builder.statements,
            exprs: builder.exprs,
        }
    }
}

/// Mutable store used while one body is being lowered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct BodyBuilder {
    pub(super) scopes: Vec<ScopeData>,
    pub(super) bindings: Vec<BindingData>,
    pub(super) statements: Vec<StmtData>,
    pub(super) exprs: Vec<ExprData>,
}

impl BodyBuilder {
    pub(super) fn alloc_scope(&mut self, parent: Option<ScopeId>) -> ScopeId {
        let scope = ScopeId(self.scopes.len());
        self.scopes.push(ScopeData {
            parent,
            bindings: Vec::new(),
        });
        scope
    }

    pub(super) fn alloc_binding(&mut self, data: BindingData) -> BindingId {
        let binding = BindingId(self.bindings.len());
        let scope = data.scope;
        self.bindings.push(data);
        self.scopes
            .get_mut(scope.0)
            .expect("binding scope should exist while lowering body")
            .bindings
            .push(binding);
        binding
    }

    pub(super) fn alloc_statement(&mut self, data: StmtData) -> StmtId {
        let statement = StmtId(self.statements.len());
        self.statements.push(data);
        statement
    }

    pub(super) fn alloc_expr(&mut self, data: ExprData) -> ExprId {
        let expr = ExprId(self.exprs.len());
        self.exprs.push(data);
        expr
    }
}

/// Source location attached to every body node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodySource {
    pub file_id: FileId,
    pub span: Span,
}

/// One lexical scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeData {
    pub parent: Option<ScopeId>,
    pub bindings: Vec<BindingId>,
}

/// One local binding introduced by a parameter or `let`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingData {
    pub source: BodySource,
    pub scope: ScopeId,
    pub kind: BindingKind,
    pub name: Option<String>,
    pub pat: String,
    pub annotation: Option<TypeRef>,
    pub ty: BodyTy,
}

/// Local binding category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum BindingKind {
    #[display("param")]
    Param,
    #[display("self_param")]
    SelfParam,
    #[display("let")]
    Let,
}

/// One lowered statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StmtData {
    pub source: BodySource,
    pub kind: StmtKind,
}

/// Statement forms that matter for the first Body IR pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StmtKind {
    Let {
        bindings: Vec<BindingId>,
        annotation: Option<TypeRef>,
        initializer: Option<ExprId>,
    },
    Expr {
        expr: ExprId,
        has_semicolon: bool,
    },
    ItemIgnored,
}

/// One lowered expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprData {
    pub source: BodySource,
    pub scope: ScopeId,
    /// Number of body-wide bindings that were visible at this expression's source location.
    ///
    /// Scope data is frozen after lowering, so the resolver needs this boundary to avoid letting a
    /// later `let x` shadow an earlier use of `x`.
    pub visible_bindings: usize,
    pub kind: ExprKind,
    pub resolution: BodyResolution,
    pub ty: BodyTy,
}

/// Expression forms that the first Body IR pass understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprKind {
    Block {
        scope: ScopeId,
        statements: Vec<StmtId>,
        tail: Option<ExprId>,
    },
    Path {
        path: crate::def_map::Path,
    },
    Call {
        callee: Option<ExprId>,
        args: Vec<ExprId>,
    },
    MethodCall {
        receiver: Option<ExprId>,
        dot_span: Option<Span>,
        method_name: String,
        method_name_span: Option<Span>,
        args: Vec<ExprId>,
    },
    Field {
        base: Option<ExprId>,
        dot_span: Option<Span>,
        field_name: String,
        field_name_span: Option<Span>,
    },
    Literal {
        text: String,
        kind: LiteralKind,
    },
    Unknown {
        text: String,
        children: Vec<ExprId>,
    },
}

/// Literal category used for display and future cheap inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum LiteralKind {
    #[display("bool")]
    Bool,
    #[display("char")]
    Char,
    #[display("float")]
    Float,
    #[display("int")]
    Int,
    #[display("string")]
    String,
    #[display("unknown")]
    Unknown,
}

/// Best-effort semantic resolution attached to body expressions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BodyResolution {
    Local(BindingId),
    Item(Vec<DefId>),
    #[default]
    Unknown,
}

/// Small type vocabulary for the first Body IR pass.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BodyTy {
    Unit,
    Never,
    Syntax(TypeRef),
    Nominal(Vec<TypeDefRef>),
    SelfTy(Vec<TypeDefRef>),
    #[default]
    Unknown,
}
