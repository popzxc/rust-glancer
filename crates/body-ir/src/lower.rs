//! Mechanical lowering from function-body AST into Body IR.
//!
//! This pass intentionally does not resolve names. It records the source shape, lexical scopes,
//! and visibility-order binding boundaries so the later resolution pass can stay focused.

use anyhow::Context as _;
use ra_syntax::{
    AstNode as _,
    ast::{self, HasArgList as _, HasName as _},
};

use rg_def_map::{ModuleRef, PackageSlot, Path, PathSegment, TargetRef};
use rg_item_tree::{
    FieldKey, FieldList, FunctionItem, GenericParams, ImplItem, ItemTreeDb, ItemTreeRef, TypeRef,
};
use rg_parse::{FileId, LineIndex, ParseDb, Span, TargetId};
use rg_semantic_ir::{FunctionRef, ImplRef, ItemOwner, SemanticIrDb, TraitRef};

use super::{
    BodyIrBuildPolicy, BodyIrDb,
    body::{BodyBuilder, BodyData, BodySource, PackageBodies, TargetBodies},
    expr::{ExprData, ExprKind, LiteralKind, MatchArmData},
    ids::{BindingId, BodyFunctionId, BodyImplId, BodyItemId, ExprId, PatId, ScopeId, StmtId},
    item::{BodyFunctionData, BodyFunctionOwner, BodyImplData, BodyItemData, BodyItemKind},
    pat::{PatData, PatKind, RecordPatField},
    resolved::BodyResolution,
    stmt::{BindingData, BindingKind, StmtData, StmtKind},
    ty::BodyTy,
};

pub(super) fn build_db(
    parse: &ParseDb,
    item_tree: &ItemTreeDb,
    semantic_ir: &SemanticIrDb,
    policy: BodyIrBuildPolicy,
) -> anyhow::Result<BodyIrDb> {
    let mut packages = Vec::with_capacity(semantic_ir.packages().len());

    for (package_idx, package_ir) in semantic_ir.packages().iter().enumerate() {
        packages.push(build_package(
            parse,
            item_tree,
            semantic_ir,
            policy,
            package_idx,
            package_ir.targets().len(),
        )?);
    }

    Ok(BodyIrDb::new(packages))
}

pub(super) fn build_package(
    parse: &ParseDb,
    item_tree: &ItemTreeDb,
    semantic_ir: &SemanticIrDb,
    policy: BodyIrBuildPolicy,
    package_idx: usize,
    target_count: usize,
) -> anyhow::Result<PackageBodies> {
    let parse_package = parse
        .package(package_idx)
        .with_context(|| format!("while attempting to fetch parse package {package_idx}"))?;
    let item_tree_package = item_tree
        .package(package_idx)
        .with_context(|| format!("while attempting to fetch item tree package {package_idx}"))?;
    let mut targets = Vec::with_capacity(target_count);

    for target_idx in 0..target_count {
        let target_ref = TargetRef {
            package: PackageSlot(package_idx),
            target: TargetId(target_idx),
        };
        let function_count = semantic_ir.function_count(target_ref);
        if !policy.should_lower_package(parse_package) {
            targets.push(TargetBodies::skipped(function_count));
            continue;
        }

        targets.push(
            TargetLowering {
                parse_package,
                item_tree_package,
                semantic_ir,
                target_ref,
                target_bodies: TargetBodies::new(function_count),
            }
            .lower()
            .with_context(|| {
                format!("while attempting to lower body IR for target {target_idx}")
            })?,
        );
    }

    Ok(PackageBodies::new(targets))
}

struct TargetLowering<'a> {
    parse_package: &'a rg_parse::Package,
    item_tree_package: &'a rg_item_tree::Package,
    semantic_ir: &'a SemanticIrDb,
    target_ref: TargetRef,
    target_bodies: TargetBodies,
}

impl<'a> TargetLowering<'a> {
    fn lower(mut self) -> anyhow::Result<TargetBodies> {
        let functions = self
            .semantic_ir
            .functions(self.target_ref)
            .map(|(function_ref, function)| (function_ref, function.source))
            .collect::<Vec<_>>();

        for (function_ref, function_source) in functions {
            let Some(owner_module) = self.owner_module(function_ref) else {
                continue;
            };
            let Some(ast_fn) = self.find_function_ast(function_source)? else {
                continue;
            };
            let Some(body_ast) = ast_fn.body() else {
                continue;
            };

            let line_index = self
                .parse_package
                .parsed_file(function_source.file_id)
                .expect("function source file should exist while lowering body")
                .line_index();
            let source = source_for(function_source.file_id, ast_fn.syntax(), line_index);
            let body = FunctionBodyLowering::new(function_ref, owner_module, source, line_index)
                .lower(ast_fn, body_ast);
            let body_id = self.target_bodies.alloc_body(body);
            self.target_bodies
                .set_function_body(function_ref.id, body_id);
        }

        Ok(self.target_bodies)
    }

    fn owner_module(&self, function: FunctionRef) -> Option<ModuleRef> {
        let function_data = self.semantic_ir.function_data(function)?;
        match function_data.owner {
            ItemOwner::Module(module_ref) => Some(module_ref),
            ItemOwner::Trait(trait_id) => self
                .semantic_ir
                .trait_data(TraitRef {
                    target: function.target,
                    id: trait_id,
                })
                .map(|data| data.owner),
            ItemOwner::Impl(impl_id) => self
                .semantic_ir
                .impl_data(ImplRef {
                    target: function.target,
                    id: impl_id,
                })
                .map(|data| data.owner),
        }
    }

    fn find_function_ast(&self, source: ItemTreeRef) -> anyhow::Result<Option<ast::Fn>> {
        let item = self.item_tree_package.item(source).with_context(|| {
            format!(
                "while attempting to fetch item-tree node {:?} in {:?}",
                source.item, source.file_id
            )
        })?;
        let parsed_file = self
            .parse_package
            .parsed_file(source.file_id)
            .with_context(|| {
                format!(
                    "while attempting to fetch parsed source file {:?}",
                    source.file_id
                )
            })?;

        let expected = item.span.text;
        Ok(parsed_file
            .syntax()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .find(|function| {
                let range = function.syntax().text_range();
                u32::from(range.start()) == expected.start && u32::from(range.end()) == expected.end
            }))
    }
}

struct FunctionBodyLowering<'a> {
    owner: FunctionRef,
    owner_module: ModuleRef,
    function_source: BodySource,
    line_index: &'a LineIndex,
    builder: BodyBuilder,
}

impl<'a> FunctionBodyLowering<'a> {
    fn new(
        owner: FunctionRef,
        owner_module: ModuleRef,
        function_source: BodySource,
        line_index: &'a LineIndex,
    ) -> Self {
        Self {
            owner,
            owner_module,
            function_source,
            line_index,
            builder: BodyBuilder::default(),
        }
    }

    fn lower(mut self, function: ast::Fn, body: ast::BlockExpr) -> BodyData {
        // Parameters live in the function's outer lexical scope. The body block gets a child scope
        // so locals do not appear before the function boundary.
        let param_scope = self.builder.alloc_scope(None);
        let params = self.lower_params(function.param_list(), param_scope);
        let root_expr = self.lower_block_expr(body, param_scope);

        BodyData::new(
            self.owner,
            self.owner_module,
            self.function_source,
            param_scope,
            root_expr,
            params,
            self.builder,
        )
    }

    fn lower_params(
        &mut self,
        param_list: Option<ast::ParamList>,
        param_scope: ScopeId,
    ) -> Vec<BindingId> {
        let Some(param_list) = param_list else {
            return Vec::new();
        };

        let mut params = Vec::new();
        if let Some(self_param) = param_list.self_param() {
            params.push(self.lower_self_param(self_param, param_scope));
        }

        params.extend(
            param_list
                .params()
                .flat_map(|param| self.lower_param(param, param_scope)),
        );

        params
    }

    fn lower_self_param(&mut self, param: ast::SelfParam, scope: ScopeId) -> BindingId {
        let source = self.source(param.syntax());
        let annotation = param.ty().map(|ty| TypeRef::from_ast(ty, self.line_index));
        self.builder.alloc_binding(BindingData {
            source,
            scope,
            kind: BindingKind::SelfParam,
            name: Some("self".to_string()),
            pat: normalized_syntax(&param),
            annotation,
            ty: BodyTy::Unknown,
        })
    }

    fn lower_param(&mut self, param: ast::Param, scope: ScopeId) -> Vec<BindingId> {
        let annotation = param.ty().map(|ty| TypeRef::from_ast(ty, self.line_index));
        match param.pat() {
            Some(pat) => self.lower_pat(pat, scope, BindingKind::Param, annotation).1,
            None => vec![self.builder.alloc_binding(BindingData {
                source: self.source(param.syntax()),
                scope,
                kind: BindingKind::Param,
                name: None,
                pat: "<missing>".to_string(),
                annotation,
                ty: BodyTy::Unknown,
            })],
        }
    }

    fn lower_block_expr(&mut self, block: ast::BlockExpr, parent_scope: ScopeId) -> ExprId {
        let block_scope = self.builder.alloc_scope(Some(parent_scope));
        let mut statements = Vec::new();
        let mut tail = None;

        if let Some(stmt_list) = block.stmt_list() {
            statements.extend(
                stmt_list
                    .statements()
                    .map(|statement| self.lower_statement(statement, block_scope)),
            );
            tail = stmt_list
                .tail_expr()
                .map(|tail_expr| self.lower_expr(tail_expr, block_scope));
        }

        self.alloc_expr(
            block.syntax(),
            block_scope,
            ExprKind::Block {
                scope: block_scope,
                statements,
                tail,
            },
        )
    }

    fn lower_statement(&mut self, statement: ast::Stmt, scope: ScopeId) -> StmtId {
        match statement {
            ast::Stmt::LetStmt(statement) => self.lower_let_statement(statement, scope),
            ast::Stmt::ExprStmt(statement) => {
                let expr = statement.expr().map(|expr| self.lower_expr(expr, scope));
                self.builder.alloc_statement(StmtData {
                    source: self.source(statement.syntax()),
                    kind: match expr {
                        Some(expr) => StmtKind::Expr {
                            expr,
                            has_semicolon: statement.semicolon_token().is_some(),
                        },
                        None => StmtKind::ItemIgnored,
                    },
                })
            }
            ast::Stmt::Item(item) => self.lower_item_statement(item, scope),
        }
    }

    fn lower_item_statement(&mut self, item: ast::Item, scope: ScopeId) -> StmtId {
        let source = self.source(item.syntax());
        // Body IR only keeps local items that can affect current editor queries. Other item
        // statements remain represented as ignored statements so source layout stays stable.
        let kind = match item {
            ast::Item::Struct(item) => self
                .lower_local_struct_item(item, scope)
                .map(|item| StmtKind::Item { item })
                .unwrap_or(StmtKind::ItemIgnored),
            ast::Item::Impl(item) => self
                .lower_local_impl_item(item, scope)
                .map(|impl_id| StmtKind::Impl { impl_id })
                .unwrap_or(StmtKind::ItemIgnored),
            _ => StmtKind::ItemIgnored,
        };

        self.builder.alloc_statement(StmtData { source, kind })
    }

    fn lower_local_struct_item(&mut self, item: ast::Struct, scope: ScopeId) -> Option<BodyItemId> {
        let name = item.name()?;
        let fields = FieldList::from_ast(item.field_list(), self.line_index);

        Some(self.builder.alloc_local_item(BodyItemData {
            source: self.source(item.syntax()),
            name_source: self.source(name.syntax()),
            scope,
            kind: BodyItemKind::Struct,
            name: name.text().to_string(),
            generics: GenericParams::from_ast(&item, self.line_index),
            fields,
        }))
    }

    fn lower_local_impl_item(&mut self, item: ast::Impl, scope: ScopeId) -> Option<BodyImplId> {
        let impl_item = ImplItem::from_ast(&item, Vec::new(), self.line_index);
        let impl_id = self.builder.alloc_local_impl(BodyImplData {
            source: self.source(item.syntax()),
            scope,
            generics: impl_item.generics,
            trait_ref: impl_item.trait_ref,
            self_ty: impl_item.self_ty,
            self_item: None,
            functions: Vec::new(),
        });

        let functions = item
            .assoc_item_list()
            .into_iter()
            .flat_map(|item_list| item_list.assoc_items())
            .filter_map(|item| self.lower_local_assoc_function(item, impl_id))
            .collect::<Vec<_>>();
        self.builder.set_local_impl_functions(impl_id, functions);

        Some(impl_id)
    }

    fn lower_local_assoc_function(
        &mut self,
        item: ast::AssocItem,
        impl_id: BodyImplId,
    ) -> Option<BodyFunctionId> {
        let ast::AssocItem::Fn(function) = item else {
            return None;
        };
        let name = function.name()?;

        Some(self.builder.alloc_local_function(BodyFunctionData {
            source: self.source(function.syntax()),
            name_source: self.source(name.syntax()),
            owner: BodyFunctionOwner::LocalImpl(impl_id),
            name: name.text().to_string(),
            declaration: FunctionItem::from_ast(&function, self.line_index),
        }))
    }

    fn lower_let_statement(&mut self, statement: ast::LetStmt, scope: ScopeId) -> StmtId {
        // Initializers cannot see the binding introduced by their own `let`, so lower the
        // initializer before allocating the binding.
        let initializer = statement
            .initializer()
            .map(|initializer| self.lower_expr(initializer, scope));
        let annotation = statement
            .ty()
            .map(|ty| TypeRef::from_ast(ty, self.line_index));
        let bindings = statement
            .pat()
            .map(|pat| self.lower_pat(pat, scope, BindingKind::Let, annotation.clone()))
            .unwrap_or_default();
        let (pat, bindings) = bindings;

        self.builder.alloc_statement(StmtData {
            source: self.source(statement.syntax()),
            kind: StmtKind::Let {
                scope,
                pat,
                bindings,
                annotation,
                initializer,
            },
        })
    }

    fn lower_pat(
        &mut self,
        pat: ast::Pat,
        scope: ScopeId,
        kind: BindingKind,
        annotation: Option<TypeRef>,
    ) -> (Option<PatId>, Vec<BindingId>) {
        let mut bindings = Vec::new();
        let pat = self.lower_pat_inner(pat, scope, kind, annotation, &mut bindings);
        (Some(pat), bindings)
    }

    fn lower_pat_inner(
        &mut self,
        pat: ast::Pat,
        scope: ScopeId,
        kind: BindingKind,
        annotation: Option<TypeRef>,
        bindings: &mut Vec<BindingId>,
    ) -> PatId {
        let source = self.source(pat.syntax());
        let pat_kind = match pat {
            ast::Pat::BoxPat(pat) => {
                let Some(inner) = pat.pat() else {
                    return self.alloc_unsupported_pat(pat.syntax());
                };
                PatKind::Box {
                    pat: self.lower_pat_inner(inner, scope, kind, None, bindings),
                }
            }
            ast::Pat::IdentPat(pat) => {
                let Some(name) = pat.name().map(|name| name.text().to_string()) else {
                    return self.alloc_unsupported_pat(pat.syntax());
                };
                let subpat = pat
                    .pat()
                    .map(|pat| self.lower_pat_inner(pat, scope, kind, None, bindings));
                if is_capitalized_bare_pat(&name, &pat, subpat) {
                    PatKind::Path {
                        path: Some(Path {
                            absolute: false,
                            segments: vec![PathSegment::Name(name)],
                        }),
                    }
                } else {
                    let binding = self.push_pat_binding(
                        pat.syntax(),
                        scope,
                        kind,
                        name,
                        normalized_syntax(&pat),
                        annotation.clone(),
                        bindings,
                    );
                    PatKind::Binding { binding, subpat }
                }
            }
            ast::Pat::OrPat(pat) => {
                let pats = pat
                    .pats()
                    .map(|inner| self.lower_pat_inner(inner, scope, kind, None, bindings))
                    .collect();
                PatKind::Or { pats }
            }
            ast::Pat::ParenPat(pat) => {
                let Some(inner) = pat.pat() else {
                    return self.alloc_unsupported_pat(pat.syntax());
                };
                return self.lower_pat_inner(inner, scope, kind, annotation, bindings);
            }
            ast::Pat::RecordPat(pat) => {
                let fields = pat
                    .record_pat_field_list()
                    .into_iter()
                    .flat_map(|field_list| field_list.fields())
                    .filter_map(|field| {
                        let name = field.field_name()?.text().to_string();
                        let key = FieldKey::Named(name.clone());
                        let pat = if let Some(inner) = field.pat() {
                            self.lower_pat_inner(inner, scope, kind, None, bindings)
                        } else {
                            self.lower_record_shorthand_pat(
                                field.syntax(),
                                scope,
                                kind,
                                name,
                                bindings,
                            )
                        };
                        Some(RecordPatField { key, pat })
                    })
                    .collect();
                PatKind::Record {
                    path: pat.path().map(path_from_ast),
                    fields,
                }
            }
            ast::Pat::RefPat(pat) => {
                let Some(inner) = pat.pat() else {
                    return self.alloc_unsupported_pat(pat.syntax());
                };
                PatKind::Ref {
                    pat: self.lower_pat_inner(inner, scope, kind, None, bindings),
                }
            }
            ast::Pat::SlicePat(pat) => {
                let fields = pat
                    .pats()
                    .map(|inner| self.lower_pat_inner(inner, scope, kind, None, bindings))
                    .collect();
                PatKind::Slice { fields }
            }
            ast::Pat::TuplePat(pat) => {
                let fields = pat
                    .fields()
                    .map(|inner| self.lower_pat_inner(inner, scope, kind, None, bindings))
                    .collect();
                PatKind::Tuple { fields }
            }
            ast::Pat::TupleStructPat(pat) => {
                let fields = pat
                    .fields()
                    .map(|inner| self.lower_pat_inner(inner, scope, kind, None, bindings))
                    .collect();
                PatKind::TupleStruct {
                    path: pat.path().map(path_from_ast),
                    fields,
                }
            }
            ast::Pat::PathPat(pat) => PatKind::Path {
                path: pat.path().map(path_from_ast),
            },
            ast::Pat::RestPat(_) | ast::Pat::WildcardPat(_) => PatKind::Wildcard,
            unsupported @ (ast::Pat::ConstBlockPat(_)
            | ast::Pat::LiteralPat(_)
            | ast::Pat::MacroPat(_)
            | ast::Pat::RangePat(_)) => PatKind::Unsupported {
                text: normalized_syntax(&unsupported),
            },
        };

        self.builder.alloc_pat(PatData {
            source,
            kind: pat_kind,
        })
    }

    fn push_pat_binding(
        &mut self,
        syntax: &ra_syntax::SyntaxNode,
        scope: ScopeId,
        kind: BindingKind,
        name: String,
        pat: String,
        annotation: Option<TypeRef>,
        bindings: &mut Vec<BindingId>,
    ) -> Option<BindingId> {
        // Multiple bindings with the same textual name can appear in or-patterns. Keep the first
        // lowered binding so downstream snapshots and type propagation have one stable target.
        if bindings
            .iter()
            .filter_map(|binding| self.builder.bindings.get(binding.0))
            .any(|binding| binding.name.as_deref() == Some(name.as_str()))
        {
            return None;
        }

        let binding = self.builder.alloc_binding(BindingData {
            source: self.source(syntax),
            scope,
            kind,
            name: Some(name),
            pat,
            annotation,
            ty: BodyTy::Unknown,
        });
        bindings.push(binding);
        Some(binding)
    }

    fn lower_record_shorthand_pat(
        &mut self,
        syntax: &ra_syntax::SyntaxNode,
        scope: ScopeId,
        kind: BindingKind,
        name: String,
        bindings: &mut Vec<BindingId>,
    ) -> PatId {
        let binding = self.push_pat_binding(
            syntax,
            scope,
            kind,
            name,
            normalized_syntax_node(syntax),
            None,
            bindings,
        );
        self.builder.alloc_pat(PatData {
            source: self.source(syntax),
            kind: PatKind::Binding {
                binding,
                subpat: None,
            },
        })
    }

    fn alloc_unsupported_pat(&mut self, syntax: &ra_syntax::SyntaxNode) -> PatId {
        self.builder.alloc_pat(PatData {
            source: self.source(syntax),
            kind: PatKind::Unsupported {
                text: normalized_syntax_node(syntax),
            },
        })
    }

    fn lower_expr(&mut self, expr: ast::Expr, scope: ScopeId) -> ExprId {
        match expr {
            ast::Expr::BlockExpr(block) => self.lower_block_expr(block, scope),
            ast::Expr::CallExpr(call) => self.lower_call_expr(call, scope),
            ast::Expr::FieldExpr(field) => self.lower_field_expr(field, scope),
            ast::Expr::Literal(literal) => self.lower_literal(literal, scope),
            ast::Expr::MatchExpr(match_expr) => self.lower_match_expr(match_expr, scope),
            ast::Expr::MethodCallExpr(method_call) => {
                self.lower_method_call_expr(method_call, scope)
            }
            ast::Expr::ParenExpr(paren) => match paren.expr() {
                Some(inner) => self.lower_passthrough_unknown(paren.syntax(), vec![inner], scope),
                None => self.lower_unknown_expr(paren.syntax(), scope),
            },
            ast::Expr::PathExpr(path) => self.lower_path_expr(path, scope),
            ast::Expr::PrefixExpr(prefix) => match prefix.expr() {
                Some(inner) => self.lower_passthrough_unknown(prefix.syntax(), vec![inner], scope),
                None => self.lower_unknown_expr(prefix.syntax(), scope),
            },
            ast::Expr::RefExpr(ref_expr) => match ref_expr.expr() {
                Some(inner) => {
                    self.lower_passthrough_unknown(ref_expr.syntax(), vec![inner], scope)
                }
                None => self.lower_unknown_expr(ref_expr.syntax(), scope),
            },
            ast::Expr::ReturnExpr(return_expr) => match return_expr.expr() {
                Some(inner) => {
                    self.lower_passthrough_unknown(return_expr.syntax(), vec![inner], scope)
                }
                None => self.lower_unknown_expr(return_expr.syntax(), scope),
            },
            // Unsupported expressions still lower their direct expression children so cursor and
            // type queries can work inside syntax the IR does not model yet.
            expr => self.lower_unknown_with_direct_children(expr.syntax(), scope),
        }
    }

    fn lower_call_expr(&mut self, call: ast::CallExpr, scope: ScopeId) -> ExprId {
        let callee = call.expr().map(|callee| self.lower_expr(callee, scope));
        let args = call
            .arg_list()
            .into_iter()
            .flat_map(|args| args.args())
            .map(|arg| self.lower_expr(arg, scope))
            .collect();

        self.alloc_expr(call.syntax(), scope, ExprKind::Call { callee, args })
    }

    fn lower_match_expr(&mut self, match_expr: ast::MatchExpr, scope: ScopeId) -> ExprId {
        let scrutinee = match_expr
            .expr()
            .map(|scrutinee| self.lower_expr(scrutinee, scope));
        let arms = match_expr
            .match_arm_list()
            .into_iter()
            .flat_map(|arm_list| arm_list.arms())
            .map(|arm| self.lower_match_arm(arm, scope))
            .collect();

        self.alloc_expr(
            match_expr.syntax(),
            scope,
            ExprKind::Match { scrutinee, arms },
        )
    }

    fn lower_match_arm(&mut self, arm: ast::MatchArm, parent_scope: ScopeId) -> MatchArmData {
        let scope = self.builder.alloc_scope(Some(parent_scope));
        let pat = arm
            .pat()
            .map(|pat| self.lower_pat(pat, scope, BindingKind::Let, None).0)
            .unwrap_or_default();
        let expr = arm.expr().map(|expr| self.lower_expr(expr, scope));

        MatchArmData { pat, scope, expr }
    }

    fn lower_method_call_expr(
        &mut self,
        method_call: ast::MethodCallExpr,
        scope: ScopeId,
    ) -> ExprId {
        let receiver = method_call
            .receiver()
            .map(|receiver| self.lower_expr(receiver, scope));
        let dot_span = method_call
            .dot_token()
            .map(|dot| Span::from_text_range(dot.text_range(), self.line_index));
        let name_ref = method_call.name_ref();
        let method_name = name_ref
            .as_ref()
            .map(|name| name.syntax().text().to_string())
            .unwrap_or_else(|| "<missing>".to_string());
        let method_name_span = name_ref
            .as_ref()
            .map(|name| self.source(name.syntax()).span);
        let args = method_call
            .arg_list()
            .into_iter()
            .flat_map(|args| args.args())
            .map(|arg| self.lower_expr(arg, scope))
            .collect();

        self.alloc_expr(
            method_call.syntax(),
            scope,
            ExprKind::MethodCall {
                receiver,
                dot_span,
                method_name,
                method_name_span,
                args,
            },
        )
    }

    fn lower_field_expr(&mut self, field: ast::FieldExpr, scope: ScopeId) -> ExprId {
        let base = field.expr().map(|base| self.lower_expr(base, scope));
        let dot_span = field
            .dot_token()
            .map(|dot| Span::from_text_range(dot.text_range(), self.line_index));
        let (field_key, field_span) = if let Some(index) = field.index_token() {
            (
                index.text().parse::<usize>().ok().map(FieldKey::Tuple),
                Some(Span::from_text_range(index.text_range(), self.line_index)),
            )
        } else if let Some(name) = field.name_ref() {
            let field_key = name
                .as_tuple_field()
                .map(FieldKey::Tuple)
                .unwrap_or_else(|| FieldKey::Named(name.syntax().text().to_string()));
            (Some(field_key), Some(self.source(name.syntax()).span))
        } else {
            (None, None)
        };

        self.alloc_expr(
            field.syntax(),
            scope,
            ExprKind::Field {
                base,
                dot_span,
                field: field_key,
                field_span,
            },
        )
    }

    fn lower_literal(&mut self, literal: ast::Literal, scope: ScopeId) -> ExprId {
        let text = normalized_syntax(&literal);
        let kind = LiteralKind::from_text(&text);

        self.alloc_expr(literal.syntax(), scope, ExprKind::Literal { text, kind })
    }

    fn lower_path_expr(&mut self, expr: ast::PathExpr, scope: ScopeId) -> ExprId {
        let Some(path) = expr.path().map(path_from_ast) else {
            return self.lower_unknown_expr(expr.syntax(), scope);
        };

        self.alloc_expr(expr.syntax(), scope, ExprKind::Path { path })
    }

    fn lower_passthrough_unknown(
        &mut self,
        syntax: &ra_syntax::SyntaxNode,
        children: Vec<ast::Expr>,
        scope: ScopeId,
    ) -> ExprId {
        let children = children
            .into_iter()
            .map(|child| self.lower_expr(child, scope))
            .collect();

        self.alloc_expr(
            syntax,
            scope,
            ExprKind::Unknown {
                text: normalized_syntax_node(syntax),
                children,
            },
        )
    }

    fn lower_unknown_with_direct_children(
        &mut self,
        syntax: &ra_syntax::SyntaxNode,
        scope: ScopeId,
    ) -> ExprId {
        let children = syntax
            .children()
            .filter_map(ast::Expr::cast)
            .map(|child| self.lower_expr(child, scope))
            .collect();

        self.alloc_expr(
            syntax,
            scope,
            ExprKind::Unknown {
                text: normalized_syntax_node(syntax),
                children,
            },
        )
    }

    fn lower_unknown_expr(&mut self, syntax: &ra_syntax::SyntaxNode, scope: ScopeId) -> ExprId {
        self.alloc_expr(
            syntax,
            scope,
            ExprKind::Unknown {
                text: normalized_syntax_node(syntax),
                children: Vec::new(),
            },
        )
    }

    fn alloc_expr(
        &mut self,
        syntax: &ra_syntax::SyntaxNode,
        scope: ScopeId,
        kind: ExprKind,
    ) -> ExprId {
        // Name resolution uses this boundary to avoid seeing bindings introduced after the
        // expression, while still allowing earlier bindings in the same lexical scope.
        let visible_bindings = self.builder.bindings.len();
        self.builder.alloc_expr(ExprData {
            source: self.source(syntax),
            scope,
            visible_bindings,
            kind,
            resolution: BodyResolution::Unknown,
            ty: BodyTy::Unknown,
        })
    }

    fn source(&self, syntax: &ra_syntax::SyntaxNode) -> BodySource {
        source_for(self.function_source.file_id, syntax, self.line_index)
    }
}

impl LiteralKind {
    fn from_text(text: &str) -> Self {
        if matches!(text, "true" | "false") {
            return Self::Bool;
        }

        if text.starts_with('"') || text.starts_with("r#") || text.starts_with("br#") {
            return Self::String;
        }

        if text.starts_with('\'') {
            return Self::Char;
        }

        if text.contains('.') {
            return Self::Float;
        }

        if text
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            return Self::Int;
        }

        Self::Unknown
    }
}

fn is_capitalized_bare_pat(name: &str, pat: &ast::IdentPat, subpat: Option<PatId>) -> bool {
    // The syntax tree represents bare unit-variant patterns such as `None` as identifier
    // patterns. Until Body IR has true pattern name resolution, this avoids treating the common
    // capitalized unit-variant shape as a local binding.
    subpat.is_none()
        && pat.ref_token().is_none()
        && pat.mut_token().is_none()
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_uppercase())
}

fn path_from_ast(path: ast::Path) -> Path {
    let absolute = path
        .first_segment()
        .is_some_and(|segment| segment.coloncolon_token().is_some());
    let mut segments = Vec::new();
    collect_path_segments(&path, &mut segments);

    Path { absolute, segments }
}

fn collect_path_segments(path: &ast::Path, segments: &mut Vec<PathSegment>) {
    if let Some(qualifier) = path.qualifier() {
        collect_path_segments(&qualifier, segments);
    }

    if let Some(segment) = path.segment() {
        let Some(name) = segment
            .name_ref()
            .map(|name| name.syntax().text().to_string())
        else {
            segments.push(PathSegment::Name(normalized_syntax(&segment)));
            return;
        };

        segments.push(match name.as_str() {
            "self" => PathSegment::SelfKw,
            "super" => PathSegment::SuperKw,
            "crate" => PathSegment::CrateKw,
            name => PathSegment::Name(name.to_string()),
        });
    }
}

fn source_for(
    file_id: FileId,
    syntax: &ra_syntax::SyntaxNode,
    line_index: &LineIndex,
) -> BodySource {
    BodySource {
        file_id,
        span: Span::from_text_range(syntax.text_range(), line_index),
    }
}

fn normalized_syntax(node: &impl ra_syntax::AstNode) -> String {
    normalized_syntax_node(node.syntax())
}

fn normalized_syntax_node(node: &ra_syntax::SyntaxNode) -> String {
    node.text()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
