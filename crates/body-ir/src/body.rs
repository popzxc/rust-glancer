use rg_parse::{FileId, Span, TargetId};
use rg_semantic_ir::{FunctionId, FunctionRef};

use crate::{
    expr::ExprData,
    ids::{
        BindingId, BodyFunctionId, BodyFunctionRef, BodyId, BodyImplId, BodyItemId, BodyItemRef,
        BodyRef, ExprId, PatId, ScopeId, StmtId,
    },
    item::{BodyFunctionData, BodyImplData, BodyItemData},
    pat::PatData,
    stmt::{BindingData, StmtData},
};

/// Coarse totals for reporting that the Body IR phase produced useful data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BodyIrStats {
    pub target_count: usize,
    pub built_target_count: usize,
    pub skipped_target_count: usize,
    pub body_count: usize,
    pub scope_count: usize,
    pub local_item_count: usize,
    pub local_impl_count: usize,
    pub local_function_count: usize,
    pub binding_count: usize,
    pub statement_count: usize,
    pub expression_count: usize,
}

/// Lowered bodies for all targets inside one parsed package.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackageBodies {
    pub(crate) targets: Vec<TargetBodies>,
}

impl PackageBodies {
    pub(super) fn new(targets: Vec<TargetBodies>) -> Self {
        Self { targets }
    }

    pub fn targets(&self) -> &[TargetBodies] {
        &self.targets
    }

    pub fn target(&self, target: TargetId) -> Option<&TargetBodies> {
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
    pub(crate) status: TargetBodiesStatus,
    pub(crate) function_bodies: Vec<Option<BodyId>>,
    pub(crate) bodies: Vec<BodyData>,
}

impl TargetBodies {
    pub(super) fn new(function_count: usize) -> Self {
        Self {
            status: TargetBodiesStatus::Built,
            function_bodies: vec![None; function_count],
            bodies: Vec::new(),
        }
    }

    pub(super) fn skipped(function_count: usize) -> Self {
        Self {
            status: TargetBodiesStatus::Skipped,
            function_bodies: vec![None; function_count],
            bodies: Vec::new(),
        }
    }

    pub fn status(&self) -> TargetBodiesStatus {
        self.status
    }

    pub fn body_for_function(&self, function: FunctionId) -> Option<BodyId> {
        self.function_bodies.get(function.0).copied().flatten()
    }

    pub fn body(&self, body: BodyId) -> Option<&BodyData> {
        self.bodies.get(body.0)
    }

    pub fn bodies(&self) -> &[BodyData] {
        &self.bodies
    }
}

/// Whether one target's bodies were eagerly lowered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum TargetBodiesStatus {
    #[display("built")]
    Built,
    #[display("skipped")]
    Skipped,
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
    pub owner_module: rg_def_map::ModuleRef,
    pub source: BodySource,
    pub param_scope: ScopeId,
    pub root_expr: ExprId,
    pub params: Vec<BindingId>,
    pub scopes: Vec<ScopeData>,
    pub local_items: Vec<BodyItemData>,
    pub local_impls: Vec<BodyImplData>,
    pub local_functions: Vec<BodyFunctionData>,
    pub bindings: Vec<BindingData>,
    pub pats: Vec<PatData>,
    pub statements: Vec<StmtData>,
    pub exprs: Vec<ExprData>,
}

#[allow(dead_code)]
impl BodyData {
    pub fn binding(&self, binding: BindingId) -> Option<&BindingData> {
        self.bindings.get(binding.0)
    }

    pub fn pat(&self, pat: PatId) -> Option<&PatData> {
        self.pats.get(pat.0)
    }

    pub fn scope(&self, scope: ScopeId) -> Option<&ScopeData> {
        self.scopes.get(scope.0)
    }

    pub fn local_item(&self, item: BodyItemId) -> Option<&BodyItemData> {
        self.local_items.get(item.0)
    }

    pub fn local_impl(&self, impl_id: BodyImplId) -> Option<&BodyImplData> {
        self.local_impls.get(impl_id.0)
    }

    pub fn local_function(&self, function: BodyFunctionId) -> Option<&BodyFunctionData> {
        self.local_functions.get(function.0)
    }

    pub fn statement(&self, statement: StmtId) -> Option<&StmtData> {
        self.statements.get(statement.0)
    }

    pub fn expr(&self, expr: ExprId) -> Option<&ExprData> {
        self.exprs.get(expr.0)
    }

    pub(super) fn new(
        owner: FunctionRef,
        owner_module: rg_def_map::ModuleRef,
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
            local_items: builder.local_items,
            local_impls: builder.local_impls,
            local_functions: builder.local_functions,
            bindings: builder.bindings,
            pats: builder.pats,
            statements: builder.statements,
            exprs: builder.exprs,
        }
    }

    pub(crate) fn local_impl_mut(&mut self, impl_id: BodyImplId) -> Option<&mut BodyImplData> {
        self.local_impls.get_mut(impl_id.0)
    }

    pub(crate) fn inherent_functions_for_local_type(
        &self,
        body_ref: BodyRef,
        item_ref: BodyItemRef,
    ) -> Vec<BodyFunctionRef> {
        if item_ref.body != body_ref {
            return Vec::new();
        }

        let mut functions = Vec::new();
        for impl_data in &self.local_impls {
            if impl_data.self_item != Some(item_ref) || impl_data.trait_ref.is_some() {
                continue;
            }

            for function in &impl_data.functions {
                functions.push(BodyFunctionRef {
                    body: body_ref,
                    function: *function,
                });
            }
        }

        functions
    }
}

/// Mutable store used while one body is being lowered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct BodyBuilder {
    pub(super) scopes: Vec<ScopeData>,
    pub(super) local_items: Vec<BodyItemData>,
    pub(super) local_impls: Vec<BodyImplData>,
    pub(super) local_functions: Vec<BodyFunctionData>,
    pub(super) bindings: Vec<BindingData>,
    pub(super) pats: Vec<PatData>,
    pub(super) statements: Vec<StmtData>,
    pub(super) exprs: Vec<ExprData>,
}

impl BodyBuilder {
    pub(super) fn alloc_scope(&mut self, parent: Option<ScopeId>) -> ScopeId {
        let scope = ScopeId(self.scopes.len());
        self.scopes.push(ScopeData {
            parent,
            local_items: Vec::new(),
            local_impls: Vec::new(),
            bindings: Vec::new(),
        });
        scope
    }

    pub(super) fn alloc_local_item(&mut self, data: BodyItemData) -> BodyItemId {
        let item = BodyItemId(self.local_items.len());
        let scope = data.scope;
        self.local_items.push(data);
        self.scopes
            .get_mut(scope.0)
            .expect("local item scope should exist while lowering body")
            .local_items
            .push(item);
        item
    }

    pub(super) fn alloc_local_impl(&mut self, data: BodyImplData) -> BodyImplId {
        let impl_id = BodyImplId(self.local_impls.len());
        let scope = data.scope;
        self.local_impls.push(data);
        self.scopes
            .get_mut(scope.0)
            .expect("local impl scope should exist while lowering body")
            .local_impls
            .push(impl_id);
        impl_id
    }

    pub(super) fn alloc_local_function(&mut self, data: BodyFunctionData) -> BodyFunctionId {
        let function = BodyFunctionId(self.local_functions.len());
        self.local_functions.push(data);
        function
    }

    pub(super) fn set_local_impl_functions(
        &mut self,
        impl_id: BodyImplId,
        functions: Vec<BodyFunctionId>,
    ) {
        let impl_data = self
            .local_impls
            .get_mut(impl_id.0)
            .expect("local impl should exist while assigning functions");
        impl_data.functions = functions;
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

    pub(super) fn alloc_pat(&mut self, data: PatData) -> PatId {
        let pat = PatId(self.pats.len());
        self.pats.push(data);
        pat
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
    pub local_items: Vec<BodyItemId>,
    pub local_impls: Vec<BodyImplId>,
    pub bindings: Vec<BindingId>,
}
