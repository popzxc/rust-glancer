//! Mechanical lowering from function-body AST into Body IR.
//!
//! This pass intentionally does not resolve names. It records the source shape, lexical scopes,
//! and visibility-order binding boundaries so the later resolution pass can stay focused.

use anyhow::Context as _;
use ra_syntax::{
    AstNode as _,
    ast::{self, HasArgList as _, HasName as _},
};

use crate::{
    def_map::{ModuleRef, PackageSlot, Path, PathSegment, TargetRef},
    item_tree::{ItemTreeDb, ItemTreeRef, TypeRef},
    parse::{
        FileId, ParseDb, TargetId,
        span::{LineIndex, Span},
    },
    semantic_ir::{FunctionId, FunctionRef, ImplRef, ItemOwner, SemanticIrDb, TraitRef},
};

use super::{
    data::{
        BindingData, BindingKind, BodyBuilder, BodyData, BodyIrDb, BodyResolution, BodySource,
        BodyTy, ExprData, ExprKind, LiteralKind, PackageBodies, StmtData, StmtKind, TargetBodies,
    },
    ids::{BindingId, ExprId, ScopeId, StmtId},
};

pub(super) fn build_db(
    parse: &ParseDb,
    item_tree: &ItemTreeDb,
    semantic_ir: &SemanticIrDb,
) -> anyhow::Result<BodyIrDb> {
    let mut packages = Vec::with_capacity(semantic_ir.packages().len());

    for (package_idx, package_ir) in semantic_ir.packages().iter().enumerate() {
        let parse_package = parse
            .packages()
            .get(package_idx)
            .with_context(|| format!("while attempting to fetch parse package {package_idx}"))?;
        let item_tree_package = item_tree.package(package_idx).with_context(|| {
            format!("while attempting to fetch item tree package {package_idx}")
        })?;
        let mut targets = Vec::with_capacity(package_ir.targets().len());

        for (target_idx, target_ir) in package_ir.targets().iter().enumerate() {
            let target_ref = TargetRef {
                package: PackageSlot(package_idx),
                target: TargetId(target_idx),
            };
            let function_count = target_ir.items().functions.len();
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

        packages.push(PackageBodies::new(targets));
    }

    Ok(BodyIrDb::new(packages))
}

struct TargetLowering<'a> {
    parse_package: &'a crate::parse::Package,
    item_tree_package: &'a crate::item_tree::Package,
    semantic_ir: &'a SemanticIrDb,
    target_ref: TargetRef,
    target_bodies: TargetBodies,
}

impl<'a> TargetLowering<'a> {
    fn lower(mut self) -> anyhow::Result<TargetBodies> {
        let target_ir = self
            .semantic_ir
            .target_ir(self.target_ref)
            .expect("target semantic IR should exist while lowering body IR");

        for (function_idx, function) in target_ir.items().functions.iter().enumerate() {
            let function_ref = FunctionRef {
                target: self.target_ref,
                id: FunctionId(function_idx),
            };
            let Some(owner_module) = self.owner_module(function_ref) else {
                continue;
            };
            let Some(ast_fn) = self.find_function_ast(function.source)? else {
                continue;
            };
            let Some(body_ast) = ast_fn.body() else {
                continue;
            };

            let line_index = &self
                .parse_package
                .files
                .parsed_file(function.source.file_id)
                .expect("function source file should exist while lowering body")
                .line_index;
            let source = source_for(function.source.file_id, ast_fn.syntax(), line_index);
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
            .files
            .parsed_file(source.file_id)
            .with_context(|| {
                format!(
                    "while attempting to fetch parsed source file {:?}",
                    source.file_id
                )
            })?;

        let expected = item.span.text;
        Ok(parsed_file
            .tree
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
        let annotation = param.ty().map(TypeRef::from_ast);
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
        let annotation = param.ty().map(TypeRef::from_ast);
        match param.pat() {
            Some(pat) => self.lower_pat_bindings(pat, scope, BindingKind::Param, annotation),
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
            ast::Stmt::Item(item) => self.builder.alloc_statement(StmtData {
                source: self.source(item.syntax()),
                kind: StmtKind::ItemIgnored,
            }),
        }
    }

    fn lower_let_statement(&mut self, statement: ast::LetStmt, scope: ScopeId) -> StmtId {
        // Initializers cannot see the binding introduced by their own `let`, so lower the
        // initializer before allocating the binding.
        let initializer = statement
            .initializer()
            .map(|initializer| self.lower_expr(initializer, scope));
        let annotation = statement.ty().map(TypeRef::from_ast);
        let bindings = statement
            .pat()
            .map(|pat| self.lower_pat_bindings(pat, scope, BindingKind::Let, annotation.clone()))
            .unwrap_or_default();

        self.builder.alloc_statement(StmtData {
            source: self.source(statement.syntax()),
            kind: StmtKind::Let {
                bindings,
                annotation,
                initializer,
            },
        })
    }

    fn lower_pat_bindings(
        &mut self,
        pat: ast::Pat,
        scope: ScopeId,
        kind: BindingKind,
        annotation: Option<TypeRef>,
    ) -> Vec<BindingId> {
        let mut bindings = Vec::new();
        self.collect_pat_bindings(pat, scope, kind, annotation, &mut bindings);
        bindings
    }

    fn collect_pat_bindings(
        &mut self,
        pat: ast::Pat,
        scope: ScopeId,
        kind: BindingKind,
        annotation: Option<TypeRef>,
        bindings: &mut Vec<BindingId>,
    ) {
        match pat {
            ast::Pat::BoxPat(pat) => {
                if let Some(inner) = pat.pat() {
                    self.collect_pat_bindings(inner, scope, kind, None, bindings);
                }
            }
            ast::Pat::IdentPat(pat) => {
                if let Some(name) = pat.name().map(|name| name.text().to_string()) {
                    self.push_pat_binding(
                        pat.syntax(),
                        scope,
                        kind,
                        name,
                        normalized_syntax(&pat),
                        annotation,
                        bindings,
                    );
                }
            }
            ast::Pat::OrPat(pat) => {
                for inner in pat.pats() {
                    self.collect_pat_bindings(inner, scope, kind, None, bindings);
                }
            }
            ast::Pat::ParenPat(pat) => {
                if let Some(inner) = pat.pat() {
                    self.collect_pat_bindings(inner, scope, kind, annotation, bindings);
                }
            }
            ast::Pat::RecordPat(pat) => {
                if let Some(field_list) = pat.record_pat_field_list() {
                    for field in field_list.fields() {
                        if let Some(inner) = field.pat() {
                            self.collect_pat_bindings(inner, scope, kind, None, bindings);
                        } else if let Some(name) = field
                            .name_ref()
                            .map(|name| name.syntax().text().to_string())
                        {
                            self.push_pat_binding(
                                field.syntax(),
                                scope,
                                kind,
                                name,
                                normalized_syntax(&field),
                                None,
                                bindings,
                            );
                        }
                    }
                }
            }
            ast::Pat::RefPat(pat) => {
                if let Some(inner) = pat.pat() {
                    self.collect_pat_bindings(inner, scope, kind, None, bindings);
                }
            }
            ast::Pat::SlicePat(pat) => {
                for inner in pat.pats() {
                    self.collect_pat_bindings(inner, scope, kind, None, bindings);
                }
            }
            ast::Pat::TuplePat(pat) => {
                for inner in pat.fields() {
                    self.collect_pat_bindings(inner, scope, kind, None, bindings);
                }
            }
            ast::Pat::TupleStructPat(pat) => {
                for inner in pat.fields() {
                    self.collect_pat_bindings(inner, scope, kind, None, bindings);
                }
            }
            ast::Pat::ConstBlockPat(_)
            | ast::Pat::LiteralPat(_)
            | ast::Pat::MacroPat(_)
            | ast::Pat::PathPat(_)
            | ast::Pat::RangePat(_)
            | ast::Pat::RestPat(_)
            | ast::Pat::WildcardPat(_) => {}
        }
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
    ) {
        if bindings
            .iter()
            .filter_map(|binding| self.builder.bindings.get(binding.0))
            .any(|binding| binding.name.as_deref() == Some(name.as_str()))
        {
            return;
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
    }

    fn lower_expr(&mut self, expr: ast::Expr, scope: ScopeId) -> ExprId {
        match expr {
            ast::Expr::BlockExpr(block) => self.lower_block_expr(block, scope),
            ast::Expr::CallExpr(call) => self.lower_call_expr(call, scope),
            ast::Expr::FieldExpr(field) => self.lower_field_expr(field, scope),
            ast::Expr::Literal(literal) => self.lower_literal(literal, scope),
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
        let name_ref = field.name_ref();
        let field_name = name_ref
            .as_ref()
            .map(|name| name.syntax().text().to_string())
            .unwrap_or_else(|| "<missing>".to_string());
        let field_name_span = name_ref
            .as_ref()
            .map(|name| self.source(name.syntax()).span);

        self.alloc_expr(
            field.syntax(),
            scope,
            ExprKind::Field {
                base,
                dot_span,
                field_name,
                field_name_span,
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
