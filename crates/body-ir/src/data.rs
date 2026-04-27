use rg_def_map::{DefId, DefMapDb, ModuleRef, PackageSlot, Path, TargetRef};
use rg_item_tree::{FieldItem, FieldKey, FieldList, FunctionItem, ParamKind, TypeRef};
use rg_parse::{FileId, Span, TargetId};
use rg_semantic_ir::{FieldRef, FunctionId, FunctionRef, SemanticIrDb, TraitRef, TypeDefRef};

use super::{
    ids::{
        BindingId, BodyFieldRef, BodyFunctionId, BodyFunctionRef, BodyId, BodyImplId, BodyItemId,
        BodyItemRef, BodyRef, ExprId, ScopeId, StmtId,
    },
    lower, resolution,
};

/// Body-level IR for all analyzed packages and targets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BodyIrDb {
    packages: Vec<PackageBodies>,
}

impl BodyIrDb {
    /// Builds Body IR using the default editor-oriented policy.
    ///
    /// By default we lower bodies only for workspace packages. Dependency signatures remain
    /// available through Semantic IR, but dependency body internals are skipped to keep the eager
    /// analysis cheaper.
    pub fn build(
        parse: &rg_parse::ParseDb,
        item_tree: &rg_item_tree::ItemTreeDb,
        def_map: &rg_def_map::DefMapDb,
        semantic_ir: &rg_semantic_ir::SemanticIrDb,
    ) -> anyhow::Result<Self> {
        Self::build_with_policy(
            parse,
            item_tree,
            def_map,
            semantic_ir,
            BodyIrBuildPolicy::default(),
        )
    }

    /// Builds Body IR using an explicit package selection policy.
    pub fn build_with_policy(
        parse: &rg_parse::ParseDb,
        item_tree: &rg_item_tree::ItemTreeDb,
        def_map: &rg_def_map::DefMapDb,
        semantic_ir: &rg_semantic_ir::SemanticIrDb,
        policy: BodyIrBuildPolicy,
    ) -> anyhow::Result<Self> {
        let mut db = lower::build_db(parse, item_tree, semantic_ir, policy)?;
        resolution::resolve_bodies(&mut db, def_map, semantic_ir);
        Ok(db)
    }

    pub(super) fn new(packages: Vec<PackageBodies>) -> Self {
        Self { packages }
    }

    pub fn stats(&self) -> BodyIrStats {
        let mut stats = BodyIrStats::default();

        for package in &self.packages {
            for target in package.targets() {
                stats.target_count += 1;
                match target.status() {
                    TargetBodiesStatus::Built => stats.built_target_count += 1,
                    TargetBodiesStatus::Skipped => stats.skipped_target_count += 1,
                }
                stats.body_count += target.bodies.len();
                for body in target.bodies() {
                    stats.scope_count += body.scopes.len();
                    stats.local_item_count += body.local_items.len();
                    stats.local_impl_count += body.local_impls.len();
                    stats.local_function_count += body.local_functions.len();
                    stats.binding_count += body.bindings.len();
                    stats.statement_count += body.statements.len();
                    stats.expression_count += body.exprs.len();
                }
            }
        }

        stats
    }
}

/// Controls which packages get function-body lowering during eager Body IR construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyIrBuildPolicy {
    package_scope: BodyIrPackageScope,
}

impl BodyIrBuildPolicy {
    /// Lowers only workspace packages.
    pub fn workspace_packages() -> Self {
        Self {
            package_scope: BodyIrPackageScope::WorkspacePackages,
        }
    }

    /// Lowers every parsed package, including dependencies and sysroot crates.
    pub fn all_packages() -> Self {
        Self {
            package_scope: BodyIrPackageScope::AllPackages,
        }
    }

    pub(super) fn should_lower_package(&self, package: &rg_parse::Package) -> bool {
        match self.package_scope {
            BodyIrPackageScope::WorkspacePackages => package.is_workspace_member(),
            BodyIrPackageScope::AllPackages => true,
        }
    }
}

impl Default for BodyIrBuildPolicy {
    fn default() -> Self {
        Self::workspace_packages()
    }
}

/// Package-set selector for eager body lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyIrPackageScope {
    WorkspacePackages,
    AllPackages,
}

#[allow(dead_code)]
impl BodyIrDb {
    /// Returns all package-level body IR sets.
    pub fn packages(&self) -> &[PackageBodies] {
        &self.packages
    }

    /// Returns one package by package slot.
    pub fn package(&self, package: PackageSlot) -> Option<&PackageBodies> {
        self.packages.get(package.0)
    }

    /// Returns one target body IR by project-wide target reference.
    pub fn target_bodies(&self, target: TargetRef) -> Option<&TargetBodies> {
        self.package(target.package)?.target(target.target)
    }

    /// Returns the body associated with a semantic function, if that function has a body.
    pub fn body_for_function(&self, function: FunctionRef) -> Option<BodyRef> {
        let body = self
            .target_bodies(function.target)?
            .body_for_function(function.id)?;
        Some(BodyRef {
            target: function.target,
            body,
        })
    }

    /// Returns one body by project-wide body reference.
    pub fn body_data(&self, body_ref: BodyRef) -> Option<&BodyData> {
        self.target_bodies(body_ref.target)?.body(body_ref.body)
    }

    pub fn resolve_type_path_in_scope(
        &self,
        def_map: &DefMapDb,
        semantic_ir: &SemanticIrDb,
        body_ref: BodyRef,
        scope: ScopeId,
        path: &Path,
    ) -> BodyTypePathResolution {
        resolution::resolve_type_path_in_scope(self, def_map, semantic_ir, body_ref, scope, path)
    }

    pub fn ty_for_field(
        &self,
        def_map: &DefMapDb,
        semantic_ir: &SemanticIrDb,
        field_ref: FieldRef,
    ) -> Option<BodyTy> {
        resolution::ty_for_field(def_map, semantic_ir, field_ref)
    }

    pub fn fields_for_local_type(&self, item_ref: BodyItemRef) -> Vec<BodyFieldRef> {
        let Some(body) = self.body_data(item_ref.body) else {
            return Vec::new();
        };
        let Some(item) = body.local_item(item_ref.item) else {
            return Vec::new();
        };

        (0..item.fields.fields().len())
            .map(|index| BodyFieldRef {
                item: item_ref,
                index,
            })
            .collect()
    }

    pub fn local_field_data(&self, field_ref: BodyFieldRef) -> Option<BodyFieldData<'_>> {
        let body = self.body_data(field_ref.item.body)?;
        let item = body.local_item(field_ref.item.item)?;
        let field = item.field(field_ref.index)?;

        Some(BodyFieldData { item, field })
    }

    pub fn inherent_functions_for_local_type(&self, item_ref: BodyItemRef) -> Vec<BodyFunctionRef> {
        let Some(body) = self.body_data(item_ref.body) else {
            return Vec::new();
        };

        body.inherent_functions_for_local_type(item_ref.body, item_ref)
    }

    pub fn local_function_data(&self, function_ref: BodyFunctionRef) -> Option<&BodyFunctionData> {
        self.body_data(function_ref.body)?
            .local_function(function_ref.function)
    }
}

impl BodyIrDb {
    pub(super) fn packages_mut(&mut self) -> &mut [PackageBodies] {
        &mut self.packages
    }
}

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
    targets: Vec<TargetBodies>,
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
    status: TargetBodiesStatus,
    function_bodies: Vec<Option<BodyId>>,
    bodies: Vec<BodyData>,
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
    pub owner_module: ModuleRef,
    pub source: BodySource,
    pub param_scope: ScopeId,
    pub root_expr: ExprId,
    pub params: Vec<BindingId>,
    pub scopes: Vec<ScopeData>,
    pub local_items: Vec<BodyItemData>,
    pub local_impls: Vec<BodyImplData>,
    pub local_functions: Vec<BodyFunctionData>,
    pub bindings: Vec<BindingData>,
    pub statements: Vec<StmtData>,
    pub exprs: Vec<ExprData>,
}

#[allow(dead_code)]
impl BodyData {
    pub fn binding(&self, binding: BindingId) -> Option<&BindingData> {
        self.bindings.get(binding.0)
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
            local_items: builder.local_items,
            local_impls: builder.local_impls,
            local_functions: builder.local_functions,
            bindings: builder.bindings,
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

/// One item declared inside a function body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyItemData {
    pub source: BodySource,
    pub name_source: BodySource,
    pub scope: ScopeId,
    pub kind: BodyItemKind,
    pub name: String,
    pub fields: FieldList,
}

impl BodyItemData {
    pub fn field(&self, index: usize) -> Option<&FieldItem> {
        self.fields.fields().get(index)
    }

    pub(crate) fn field_index(&self, key: &FieldKey) -> Option<usize> {
        self.fields
            .fields()
            .iter()
            .position(|field| field.key.as_ref() == Some(key))
    }
}

/// Resolved access to one field declared on a body-local item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyFieldData<'a> {
    pub item: &'a BodyItemData,
    pub field: &'a FieldItem,
}

/// One impl block declared inside a function body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyImplData {
    pub source: BodySource,
    pub scope: ScopeId,
    pub trait_ref: Option<TypeRef>,
    pub self_ty: TypeRef,
    pub self_item: Option<BodyItemRef>,
    pub functions: Vec<BodyFunctionId>,
}

/// One function-like declaration inside a function body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyFunctionData {
    pub source: BodySource,
    pub name_source: BodySource,
    pub owner: BodyFunctionOwner,
    pub name: String,
    pub declaration: FunctionItem,
}

impl BodyFunctionData {
    pub fn has_self_receiver(&self) -> bool {
        self.declaration
            .params
            .first()
            .is_some_and(|param| matches!(param.kind, ParamKind::SelfParam))
    }
}

/// Owner of a body-local function-like declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyFunctionOwner {
    LocalImpl(BodyImplId),
}

/// Stable field identity across module-level Semantic IR and body-local declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolvedFieldRef {
    Semantic(FieldRef),
    BodyLocal(BodyFieldRef),
}

/// Stable function identity across module-level Semantic IR and body-local declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolvedFunctionRef {
    Semantic(FunctionRef),
    BodyLocal(BodyFunctionRef),
}

/// Body-local item category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum BodyItemKind {
    #[display("struct")]
    Struct,
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
        scope: ScopeId,
        bindings: Vec<BindingId>,
        annotation: Option<TypeRef>,
        initializer: Option<ExprId>,
    },
    Expr {
        expr: ExprId,
        has_semicolon: bool,
    },
    Item {
        item: BodyItemId,
    },
    Impl {
        impl_id: BodyImplId,
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
        path: Path,
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
        field: Option<FieldKey>,
        field_span: Option<Span>,
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
    LocalItem(BodyItemRef),
    Item(Vec<DefId>),
    Field(Vec<ResolvedFieldRef>),
    Method(Vec<ResolvedFunctionRef>),
    #[default]
    Unknown,
}

/// Body-scoped type path resolution result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyTypePathResolution {
    BodyLocal(BodyItemRef),
    SelfType(Vec<TypeDefRef>),
    TypeDefs(Vec<TypeDefRef>),
    Traits(Vec<TraitRef>),
    Unknown,
}

/// Small type vocabulary for the first Body IR pass.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BodyTy {
    Unit,
    Never,
    Syntax(TypeRef),
    LocalNominal(Vec<BodyItemRef>),
    Nominal(Vec<TypeDefRef>),
    SelfTy(Vec<TypeDefRef>),
    #[default]
    Unknown,
}

impl BodyTy {
    pub fn local_items(&self) -> &[BodyItemRef] {
        match self {
            Self::LocalNominal(items) => items,
            Self::Unit
            | Self::Never
            | Self::Syntax(_)
            | Self::Nominal(_)
            | Self::SelfTy(_)
            | Self::Unknown => &[],
        }
    }

    pub fn type_defs(&self) -> &[TypeDefRef] {
        match self {
            Self::Nominal(types) | Self::SelfTy(types) => types,
            Self::Unit | Self::Never | Self::Syntax(_) | Self::LocalNominal(_) | Self::Unknown => {
                &[]
            }
        }
    }
}
