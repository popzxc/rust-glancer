use std::fmt::Write as _;

use expect_test::Expect;

use crate::{
    body_ir::{
        BodyIrDb,
        data::{
            BindingData, BodyData, BodyItemData, BodyResolution, BodySource, BodyTy, ExprData,
            ExprKind, StmtKind,
        },
        ids::{BindingId, BodyId, BodyItemId, BodyItemRef, ExprId, StmtId},
    },
    def_map::{DefId, DefMapDb, LocalDefRef, ModuleRef, TargetRef},
    item_tree::ItemTreeDb,
    parse::ParseDb,
    semantic_ir::{
        FieldRef, FunctionRef, ImplRef, ItemId, ItemOwner, SemanticIrDb, TraitRef, TypeDefId,
        TypeDefRef,
    },
    test_fixture::fixture_crate,
    test_utils::snapshot,
    workspace_metadata::WorkspaceMetadata,
};

pub(super) fn check_project_body_ir(fixture: &str, expect: Expect) {
    let db = BodyIrFixtureDb::build(fixture);
    let actual = ProjectBodyIrSnapshot::new(&db).render();
    let actual = format!("{}\n", actual.trim_end());
    expect.assert_eq(&actual);
}

struct BodyIrFixtureDb {
    parse: ParseDb,
    def_map: DefMapDb,
    semantic_ir: SemanticIrDb,
    body_ir: BodyIrDb,
}

impl BodyIrFixtureDb {
    fn build(fixture: &str) -> Self {
        let fixture = fixture_crate(fixture);
        let workspace = WorkspaceMetadata::from_cargo(fixture.metadata());
        let mut parse = ParseDb::build(&workspace).expect("fixture parse db should build");
        let item_tree = ItemTreeDb::build(&mut parse).expect("fixture item tree db should build");
        let def_map = DefMapDb::build(&workspace, &parse, &item_tree)
            .expect("fixture def map db should build");
        let semantic_ir =
            SemanticIrDb::build(&item_tree, &def_map).expect("fixture semantic ir db should build");
        let body_ir = BodyIrDb::build(&parse, &item_tree, &def_map, &semantic_ir)
            .expect("fixture body ir db should build");

        Self {
            parse,
            def_map,
            semantic_ir,
            body_ir,
        }
    }

    fn parse_db(&self) -> &ParseDb {
        &self.parse
    }

    fn def_map_db(&self) -> &DefMapDb {
        &self.def_map
    }

    fn semantic_ir_db(&self) -> &SemanticIrDb {
        &self.semantic_ir
    }

    fn body_ir_db(&self) -> &BodyIrDb {
        &self.body_ir
    }
}

struct ProjectBodyIrSnapshot<'a> {
    project: &'a BodyIrFixtureDb,
}

impl<'a> ProjectBodyIrSnapshot<'a> {
    fn new(project: &'a BodyIrFixtureDb) -> Self {
        Self { project }
    }

    fn render(&self) -> String {
        snapshot::sorted_packages(self.project.parse_db())
            .into_iter()
            .map(|(package_slot, package)| {
                let target_dumps = snapshot::sorted_targets(package)
                    .into_iter()
                    .map(|target| {
                        TargetBodyIrSnapshot {
                            project: self.project,
                            target_ref: TargetRef {
                                package: crate::def_map::PackageSlot(package_slot),
                                target: target.id,
                            },
                            target_name: &target.name,
                            target_kind: target.kind.to_string(),
                        }
                        .render()
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");

                format!("package {}\n\n{target_dumps}", package.package_name())
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

struct TargetBodyIrSnapshot<'a> {
    project: &'a BodyIrFixtureDb,
    target_ref: TargetRef,
    target_name: &'a str,
    target_kind: String,
}

impl TargetBodyIrSnapshot<'_> {
    fn render(&self) -> String {
        let mut dump = format!("{} [{}]", self.target_name, self.target_kind);
        let Some(target_bodies) = self.project.body_ir_db().target_bodies(self.target_ref) else {
            return dump;
        };

        let mut bodies = target_bodies
            .bodies()
            .iter()
            .enumerate()
            .map(|(idx, body)| (self.render_function_ref(body.owner), BodyId(idx)))
            .collect::<Vec<_>>();
        bodies.sort_by(|left, right| left.0.cmp(&right.0));

        for (idx, (_, body_id)) in bodies.into_iter().enumerate() {
            if idx == 0 {
                dump.push('\n');
            } else {
                dump.push_str("\n\n");
            }

            let body = target_bodies
                .body(body_id)
                .expect("body id should exist while rendering body IR");
            self.render_body(body, body_id, &mut dump);
        }

        dump
    }

    fn render_body(&self, body: &BodyData, body_id: BodyId, dump: &mut String) {
        writeln!(
            dump,
            "body b{} {} @ {}",
            body_id.0,
            self.render_function_ref(body.owner),
            self.render_source(body.source),
        )
        .expect("string writes should not fail");

        writeln!(dump, "scopes").expect("string writes should not fail");
        for (idx, scope) in body.scopes.iter().enumerate() {
            let parent = scope
                .parent
                .map(|scope| format!("s{}", scope.0))
                .unwrap_or_else(|| "<none>".to_string());
            let bindings = if scope.bindings.is_empty() {
                "<none>".to_string()
            } else {
                scope
                    .bindings
                    .iter()
                    .map(|binding| format!("v{}", binding.0))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let items = if scope.local_items.is_empty() {
                String::new()
            } else {
                format!(
                    "; items {}",
                    scope
                        .local_items
                        .iter()
                        .map(|item| format!("i{}", item.0))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            writeln!(dump, "- s{idx} parent {parent}: {bindings}{items}")
                .expect("string writes should not fail");
        }

        if !body.local_items.is_empty() {
            writeln!(dump, "items").expect("string writes should not fail");
            for (idx, item) in body.local_items.iter().enumerate() {
                self.render_local_item(BodyItemId(idx), item, dump);
            }
        }

        writeln!(dump, "bindings").expect("string writes should not fail");
        for (idx, binding) in body.bindings.iter().enumerate() {
            self.render_binding(body, BindingId(idx), binding, dump);
        }

        writeln!(dump, "body").expect("string writes should not fail");
        self.render_expr(body, body.root_expr, 0, dump);
    }

    fn render_local_item(&self, id: BodyItemId, item: &BodyItemData, dump: &mut String) {
        writeln!(
            dump,
            "- i{} {} {} @ {}",
            id.0,
            item.kind,
            item.name,
            self.render_source(item.source),
        )
        .expect("string writes should not fail");
    }

    fn render_binding(
        &self,
        body: &BodyData,
        id: BindingId,
        binding: &BindingData,
        dump: &mut String,
    ) {
        let annotation = binding
            .annotation
            .as_ref()
            .map(|ty| format!(": {ty}"))
            .unwrap_or_default();
        let name = binding.name.as_deref().unwrap_or("<unsupported>");

        writeln!(
            dump,
            "- v{} {} {} `{}`{} => {} @ {}",
            id.0,
            binding.kind,
            name,
            binding.pat,
            annotation,
            self.render_ty(&binding.ty),
            self.render_source(binding.source),
        )
        .expect("string writes should not fail");

        assert!(
            body.scope(binding.scope).is_some(),
            "binding scope should exist while rendering"
        );
    }

    fn render_statement(
        &self,
        body: &BodyData,
        statement: StmtId,
        depth: usize,
        dump: &mut String,
    ) {
        let data = body
            .statement(statement)
            .expect("statement id should exist while rendering body IR");

        match &data.kind {
            StmtKind::Let {
                scope: _,
                bindings,
                annotation,
                initializer,
            } => {
                let bindings = bindings
                    .iter()
                    .map(|binding| format!("v{}", binding.0))
                    .collect::<Vec<_>>()
                    .join(", ");
                let annotation = annotation
                    .as_ref()
                    .map(|ty| format!(": {ty}"))
                    .unwrap_or_default();
                writeln!(
                    dump,
                    "{}stmt s{} let {bindings}{annotation} @ {}",
                    indent(depth),
                    statement.0,
                    self.render_source(data.source),
                )
                .expect("string writes should not fail");
                if let Some(initializer) = initializer {
                    writeln!(dump, "{}initializer", indent(depth + 1))
                        .expect("string writes should not fail");
                    self.render_expr(body, *initializer, depth + 2, dump);
                }
            }
            StmtKind::Expr {
                expr,
                has_semicolon,
            } => {
                let suffix = if *has_semicolon { ";" } else { "" };
                writeln!(
                    dump,
                    "{}stmt s{} expr{suffix} @ {}",
                    indent(depth),
                    statement.0,
                    self.render_source(data.source),
                )
                .expect("string writes should not fail");
                self.render_expr(body, *expr, depth + 1, dump);
            }
            StmtKind::Item { item } => {
                writeln!(
                    dump,
                    "{}stmt s{} item i{} @ {}",
                    indent(depth),
                    statement.0,
                    item.0,
                    self.render_source(data.source),
                )
                .expect("string writes should not fail");
            }
            StmtKind::ItemIgnored => {
                writeln!(
                    dump,
                    "{}stmt s{} item <ignored> @ {}",
                    indent(depth),
                    statement.0,
                    self.render_source(data.source),
                )
                .expect("string writes should not fail");
            }
        }
    }

    fn render_expr(&self, body: &BodyData, expr: ExprId, depth: usize, dump: &mut String) {
        let data = body
            .expr(expr)
            .expect("expr id should exist while rendering body IR");
        writeln!(
            dump,
            "{}expr e{} {}{} => {} @ {}",
            indent(depth),
            expr.0,
            self.render_expr_head(data),
            self.render_resolution(&data.resolution),
            self.render_ty(&data.ty),
            self.render_source(data.source),
        )
        .expect("string writes should not fail");

        match &data.kind {
            ExprKind::Block {
                statements, tail, ..
            } => {
                for statement in statements {
                    self.render_statement(body, *statement, depth + 1, dump);
                }
                if let Some(tail) = tail {
                    writeln!(dump, "{}tail", indent(depth + 1))
                        .expect("string writes should not fail");
                    self.render_expr(body, *tail, depth + 2, dump);
                }
            }
            ExprKind::Call { callee, args } => {
                if let Some(callee) = callee {
                    writeln!(dump, "{}callee", indent(depth + 1))
                        .expect("string writes should not fail");
                    self.render_expr(body, *callee, depth + 2, dump);
                }
                for arg in args {
                    writeln!(dump, "{}arg", indent(depth + 1))
                        .expect("string writes should not fail");
                    self.render_expr(body, *arg, depth + 2, dump);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                if let Some(receiver) = receiver {
                    writeln!(dump, "{}receiver", indent(depth + 1))
                        .expect("string writes should not fail");
                    self.render_expr(body, *receiver, depth + 2, dump);
                }
                for arg in args {
                    writeln!(dump, "{}arg", indent(depth + 1))
                        .expect("string writes should not fail");
                    self.render_expr(body, *arg, depth + 2, dump);
                }
            }
            ExprKind::Field { base, .. } => {
                if let Some(base) = base {
                    writeln!(dump, "{}base", indent(depth + 1))
                        .expect("string writes should not fail");
                    self.render_expr(body, *base, depth + 2, dump);
                }
            }
            ExprKind::Unknown { children, .. } => {
                for child in children {
                    writeln!(dump, "{}child", indent(depth + 1))
                        .expect("string writes should not fail");
                    self.render_expr(body, *child, depth + 2, dump);
                }
            }
            ExprKind::Path { .. } | ExprKind::Literal { .. } => {}
        }
    }

    fn render_expr_head(&self, data: &ExprData) -> String {
        match &data.kind {
            ExprKind::Block { scope, .. } => format!("block s{}", scope.0),
            ExprKind::Path { path } => format!("path {path}"),
            ExprKind::Call { .. } => "call".to_string(),
            ExprKind::MethodCall { method_name, .. } => {
                format!("method_call {method_name}")
            }
            ExprKind::Field { field, .. } => {
                let field = field
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "<missing>".to_string());
                format!("field {field}")
            }
            ExprKind::Literal { text, kind } => format!("literal {kind} `{text}`"),
            ExprKind::Unknown { text, .. } => format!("unknown `{text}`"),
        }
    }

    fn render_resolution(&self, resolution: &BodyResolution) -> String {
        match resolution {
            BodyResolution::Local(binding) => format!(" -> local v{}", binding.0),
            BodyResolution::LocalItem(item) => {
                format!(" -> local item {}", self.render_body_item_ref(*item))
            }
            BodyResolution::Item(defs) if defs.is_empty() => " -> item <unresolved>".to_string(),
            BodyResolution::Item(defs) => {
                let mut defs = defs
                    .iter()
                    .map(|def| self.render_def(*def))
                    .collect::<Vec<_>>();
                defs.sort();
                format!(" -> item {}", defs.join(" | "))
            }
            BodyResolution::Field(fields) => {
                let mut fields = fields
                    .iter()
                    .map(|field| self.render_field_ref(*field))
                    .collect::<Vec<_>>();
                fields.sort();
                format!(" -> {}", fields.join(" | "))
            }
            BodyResolution::Unknown => String::new(),
        }
    }

    fn render_ty(&self, ty: &BodyTy) -> String {
        match ty {
            BodyTy::Unit => "()".to_string(),
            BodyTy::Never => "!".to_string(),
            BodyTy::Syntax(ty) => format!("syntax {ty}"),
            BodyTy::LocalNominal(items) => {
                let mut items = items
                    .iter()
                    .map(|item| self.render_body_item_ref(*item))
                    .collect::<Vec<_>>();
                items.sort();
                format!("local nominal {}", items.join(" | "))
            }
            BodyTy::Nominal(types) => {
                let mut types = types
                    .iter()
                    .map(|ty| self.render_type_def_ref(*ty))
                    .collect::<Vec<_>>();
                types.sort();
                format!("nominal {}", types.join(" | "))
            }
            BodyTy::SelfTy(types) => {
                let mut types = types
                    .iter()
                    .map(|ty| self.render_type_def_ref(*ty))
                    .collect::<Vec<_>>();
                types.sort();
                format!("Self {}", types.join(" | "))
            }
            BodyTy::Unknown => "<unknown>".to_string(),
        }
    }

    fn render_body_item_ref(&self, item_ref: BodyItemRef) -> String {
        let Some(body) = self.project.body_ir_db().body_data(item_ref.body) else {
            return "<missing>".to_string();
        };
        let Some(item) = body.local_item(item_ref.item) else {
            return "<missing>".to_string();
        };

        format!(
            "{} {}::{} @ {}",
            item.kind,
            self.render_function_ref(body.owner),
            item.name,
            self.render_source(item.source),
        )
    }

    fn render_def(&self, def: DefId) -> String {
        match def {
            DefId::Module(module_ref) => format!("module {}", self.render_module_ref(module_ref)),
            DefId::Local(local_def) => self.render_local_def(local_def),
        }
    }

    fn render_local_def(&self, local_def: LocalDefRef) -> String {
        let Some(target_ir) = self.project.semantic_ir_db().target_ir(local_def.target) else {
            return "<missing>".to_string();
        };
        let Some(item_id) = target_ir.item_for_local_def(local_def.local_def) else {
            return "<unsupported>".to_string();
        };

        match item_id {
            ItemId::Struct(id) => {
                let data = target_ir
                    .items()
                    .struct_data(id)
                    .expect("struct id should exist while rendering body IR");
                format!(
                    "struct {}::{}",
                    self.render_module_ref(data.owner),
                    data.name
                )
            }
            ItemId::Union(id) => {
                let data = target_ir
                    .items()
                    .union_data(id)
                    .expect("union id should exist while rendering body IR");
                format!(
                    "union {}::{}",
                    self.render_module_ref(data.owner),
                    data.name
                )
            }
            ItemId::Enum(id) => {
                let data = target_ir
                    .items()
                    .enum_data(id)
                    .expect("enum id should exist while rendering body IR");
                format!("enum {}::{}", self.render_module_ref(data.owner), data.name)
            }
            ItemId::Trait(id) => {
                let data = target_ir
                    .items()
                    .trait_data(id)
                    .expect("trait id should exist while rendering body IR");
                format!(
                    "trait {}::{}",
                    self.render_module_ref(data.owner),
                    data.name
                )
            }
            ItemId::Function(id) => self.render_function_ref(FunctionRef {
                target: local_def.target,
                id,
            }),
            ItemId::TypeAlias(id) => {
                let data = target_ir
                    .items()
                    .type_alias_data(id)
                    .expect("type alias id should exist while rendering body IR");
                format!(
                    "type {}::{}",
                    self.render_owner(data.owner, local_def.target),
                    data.name
                )
            }
            ItemId::Const(id) => {
                let data = target_ir
                    .items()
                    .const_data(id)
                    .expect("const id should exist while rendering body IR");
                format!(
                    "const {}::{}",
                    self.render_owner(data.owner, local_def.target),
                    data.name
                )
            }
            ItemId::Static(id) => {
                let data = target_ir
                    .items()
                    .static_data(id)
                    .expect("static id should exist while rendering body IR");
                format!(
                    "static {}::{}",
                    self.render_module_ref(data.owner),
                    data.name
                )
            }
        }
    }

    fn render_type_def_ref(&self, ty: TypeDefRef) -> String {
        let target_ir = self
            .project
            .semantic_ir_db()
            .target_ir(ty.target)
            .expect("target semantic IR should exist while rendering body type");

        match ty.id {
            TypeDefId::Struct(id) => {
                let data = target_ir
                    .items()
                    .struct_data(id)
                    .expect("struct id should exist while rendering body type");
                format!(
                    "struct {}::{}",
                    self.render_module_ref(data.owner),
                    data.name
                )
            }
            TypeDefId::Enum(id) => {
                let data = target_ir
                    .items()
                    .enum_data(id)
                    .expect("enum id should exist while rendering body type");
                format!("enum {}::{}", self.render_module_ref(data.owner), data.name)
            }
            TypeDefId::Union(id) => {
                let data = target_ir
                    .items()
                    .union_data(id)
                    .expect("union id should exist while rendering body type");
                format!(
                    "union {}::{}",
                    self.render_module_ref(data.owner),
                    data.name
                )
            }
        }
    }

    fn render_field_ref(&self, field_ref: FieldRef) -> String {
        let data = self
            .project
            .semantic_ir_db()
            .field_data(field_ref)
            .expect("field ref should exist while rendering body IR");
        let name = data
            .field
            .key
            .as_ref()
            .map(|key| key.declaration_label())
            .unwrap_or_else(|| "<missing>".to_string());

        format!(
            "field {}::{name}",
            self.render_type_def_ref(field_ref.owner)
        )
    }

    fn render_function_ref(&self, function_ref: FunctionRef) -> String {
        let data = self
            .project
            .semantic_ir_db()
            .function_data(function_ref)
            .expect("function id should exist while rendering body IR");
        let owner = self.render_owner(data.owner, function_ref.target);

        format!("fn {owner}::{}", data.name)
    }

    fn render_owner(&self, owner: ItemOwner, target: TargetRef) -> String {
        match owner {
            ItemOwner::Module(module_ref) => self.render_module_ref(module_ref),
            ItemOwner::Trait(trait_id) => self.render_trait_ref(TraitRef {
                target,
                id: trait_id,
            }),
            ItemOwner::Impl(impl_id) => self.render_impl_ref(ImplRef {
                target,
                id: impl_id,
            }),
        }
    }

    fn render_trait_ref(&self, trait_ref: TraitRef) -> String {
        let data = self
            .project
            .semantic_ir_db()
            .trait_data(trait_ref)
            .expect("trait id should exist while rendering body IR");

        format!(
            "trait {}::{}",
            self.render_module_ref(data.owner),
            data.name
        )
    }

    fn render_impl_ref(&self, impl_ref: ImplRef) -> String {
        let data = self
            .project
            .semantic_ir_db()
            .impl_data(impl_ref)
            .expect("impl id should exist while rendering body IR");

        match &data.trait_ref {
            Some(trait_ref) => format!("impl {trait_ref} for {}", data.self_ty),
            None => format!("impl {}", data.self_ty),
        }
    }

    fn render_module_ref(&self, module_ref: ModuleRef) -> String {
        let package = self
            .project
            .parse_db()
            .packages()
            .get(module_ref.target.package.0)
            .expect("package slot should exist while rendering body IR module");
        let target = package
            .target(module_ref.target.target)
            .expect("target id should exist while rendering body IR module");

        format!(
            "{}[{}]::{}",
            package.package_name(),
            target.kind,
            self.module_path(module_ref),
        )
    }

    fn module_path(&self, module_ref: ModuleRef) -> String {
        let module = self
            .project
            .def_map_db()
            .def_map(module_ref.target)
            .expect("target def map should exist while rendering body IR module path")
            .module(module_ref.module)
            .expect("module id should exist while rendering body IR module path");

        match module.parent {
            Some(parent) => {
                let parent_path = self.module_path(ModuleRef {
                    target: module_ref.target,
                    module: parent,
                });
                let name = module
                    .name
                    .as_deref()
                    .expect("non-root modules should have names");
                format!("{parent_path}::{name}")
            }
            None => "crate".to_string(),
        }
    }

    fn render_source(&self, source: BodySource) -> String {
        format!(
            "{}:{}-{}:{}",
            source.span.line_column.start.line + 1,
            source.span.line_column.start.column + 1,
            source.span.line_column.end.line + 1,
            source.span.line_column.end.column + 1,
        )
    }
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}
