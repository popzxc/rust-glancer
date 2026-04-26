use std::fmt::Write as _;

use expect_test::Expect;

use crate::{
    analysis::{Analysis, CompletionItem, NavigationTarget, SymbolAt},
    body_ir::{BodyIrDb, BodyItemRef, BodyTy, ExprData, ExprKind},
    def_map::{DefMapDb, ModuleRef, PackageSlot, TargetRef},
    item_tree::ItemTreeDb,
    parse::{FileId, ParseDb, span::Span},
    semantic_ir::{FunctionRef, ItemOwner, SemanticIrDb, TraitRef, TypeDefId, TypeDefRef},
    test_fixture::{FixtureMarkers, fixture_crate_with_markers},
    workspace_metadata::{TargetKind, WorkspaceMetadata},
};

pub(super) fn check_analysis_queries(fixture: &str, queries: &[AnalysisQuery], expect: Expect) {
    let (fixture, markers) = fixture_crate_with_markers(fixture);
    let db = AnalysisFixtureDb::build(WorkspaceMetadata::from_cargo(fixture.metadata()));
    let renderer = AnalysisQuerySnapshot::new(&db, markers, queries);
    let actual = format!("{}\n", renderer.render().trim_end());
    expect.assert_eq(&actual);
}

pub(super) struct AnalysisQuery {
    title: &'static str,
    marker: &'static str,
    kind: AnalysisQueryKind,
}

impl AnalysisQuery {
    pub(super) fn symbol(title: &'static str, marker: &'static str) -> Self {
        Self {
            title,
            marker,
            kind: AnalysisQueryKind::SymbolAt,
        }
    }

    pub(super) fn resolve(title: &'static str, marker: &'static str) -> Self {
        Self {
            title,
            marker,
            kind: AnalysisQueryKind::ResolveSymbol,
        }
    }

    pub(super) fn goto(title: &'static str, marker: &'static str) -> Self {
        Self {
            title,
            marker,
            kind: AnalysisQueryKind::GotoDefinition,
        }
    }

    pub(super) fn ty(title: &'static str, marker: &'static str) -> Self {
        Self {
            title,
            marker,
            kind: AnalysisQueryKind::TypeAt,
        }
    }

    pub(super) fn complete(title: &'static str, marker: &'static str) -> Self {
        Self {
            title,
            marker,
            kind: AnalysisQueryKind::CompletionsAtDot,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum AnalysisQueryKind {
    SymbolAt,
    ResolveSymbol,
    GotoDefinition,
    TypeAt,
    CompletionsAtDot,
}

struct AnalysisFixtureDb {
    parse: ParseDb,
    item_tree: ItemTreeDb,
    def_map: DefMapDb,
    semantic_ir: SemanticIrDb,
    body_ir: BodyIrDb,
}

impl AnalysisFixtureDb {
    fn build(workspace: WorkspaceMetadata) -> Self {
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
            item_tree,
            def_map,
            semantic_ir,
            body_ir,
        }
    }

    fn analysis(&self) -> Analysis<'_> {
        Analysis::new(&self.def_map, &self.semantic_ir, &self.body_ir)
    }
}

struct AnalysisQuerySnapshot<'a> {
    db: &'a AnalysisFixtureDb,
    markers: FixtureMarkers,
    queries: &'a [AnalysisQuery],
}

impl<'a> AnalysisQuerySnapshot<'a> {
    fn new(
        db: &'a AnalysisFixtureDb,
        markers: FixtureMarkers,
        queries: &'a [AnalysisQuery],
    ) -> Self {
        Self {
            db,
            markers,
            queries,
        }
    }

    fn render(&self) -> String {
        self.queries
            .iter()
            .map(|query| self.render_query(query).trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn render_query(&self, query: &AnalysisQuery) -> String {
        let (target, file_id, offset) = self.query_location(query.marker);
        let mut dump = query.title.to_string();
        match query.kind {
            AnalysisQueryKind::SymbolAt => {
                self.render_symbol(
                    self.db.analysis().symbol_at(target, file_id, offset),
                    &mut dump,
                );
            }
            AnalysisQueryKind::ResolveSymbol => {
                let Some(symbol) = self.db.analysis().symbol_at(target, file_id, offset) else {
                    self.render_targets(Vec::new(), &mut dump);
                    return dump;
                };
                self.render_targets(self.db.analysis().resolve_symbol(symbol), &mut dump);
            }
            AnalysisQueryKind::GotoDefinition => {
                self.render_targets(
                    self.db.analysis().goto_definition(target, file_id, offset),
                    &mut dump,
                );
            }
            AnalysisQueryKind::TypeAt => {
                let ty = self.db.analysis().type_at(target, file_id, offset);
                writeln!(
                    dump,
                    "\n- {}",
                    ty.as_ref()
                        .map(|ty| self.render_ty(ty))
                        .unwrap_or_else(|| "<none>".to_string())
                )
                .expect("string writes should not fail");
            }
            AnalysisQueryKind::CompletionsAtDot => {
                self.render_completions(
                    self.db
                        .analysis()
                        .completions_at_dot(target, file_id, offset),
                    &mut dump,
                );
            }
        }

        dump
    }

    fn query_location(&self, marker_name: &str) -> (TargetRef, FileId, u32) {
        let marker = self.markers.position(marker_name);
        let (target, root_file) = self.lib_target_for_path(&marker.path);

        assert_eq!(
            marker.path, "src/lib.rs",
            "analysis query tests currently use target-root markers only"
        );

        (target, root_file, marker.offset)
    }

    fn lib_target_for_path(&self, path: &str) -> (TargetRef, FileId) {
        let mut matches = self
            .db
            .parse
            .packages()
            .iter()
            .enumerate()
            .flat_map(|(package_slot, package)| {
                package
                    .targets()
                    .iter()
                    .filter(move |target| {
                        target.kind == TargetKind::Lib && target.src_path.ends_with(path)
                    })
                    .map(move |target| (package_slot, target))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "marker path `{path}` should identify exactly one lib target"
        );
        let (package_slot, target) = matches.pop().expect("one match should be present");

        (
            TargetRef {
                package: PackageSlot(package_slot),
                target: target.id,
            },
            target.root_file,
        )
    }

    fn render_symbol(&self, symbol: Option<SymbolAt>, dump: &mut String) {
        let Some(symbol) = symbol else {
            writeln!(dump, "\n- <none>").expect("string writes should not fail");
            return;
        };

        match symbol {
            SymbolAt::Body { body } => {
                let body_data = self
                    .db
                    .body_ir
                    .body_data(body)
                    .expect("body ref should exist while rendering analysis symbol");
                writeln!(
                    dump,
                    "\n- body @ {}",
                    self.render_source_span(body_data.source.span)
                )
                .expect("string writes should not fail");
            }
            SymbolAt::Binding { body, binding } => {
                let body_data = self
                    .db
                    .body_ir
                    .body_data(body)
                    .expect("body ref should exist while rendering analysis symbol");
                let binding_data = body_data
                    .binding(binding)
                    .expect("binding id should exist while rendering analysis symbol");
                writeln!(
                    dump,
                    "\n- binding {} {} @ {}",
                    binding_data.kind,
                    binding_data.name.as_deref().unwrap_or("<unsupported>"),
                    self.render_source_span(binding_data.source.span)
                )
                .expect("string writes should not fail");
            }
            SymbolAt::BodyPath { ref path, span, .. } => {
                writeln!(
                    dump,
                    "\n- body path {path} @ {}",
                    self.render_source_span(span)
                )
                .expect("string writes should not fail");
            }
            SymbolAt::Def { def, span } => {
                let targets = self
                    .db
                    .analysis()
                    .resolve_symbol(SymbolAt::Def { def, span });
                let label = targets
                    .first()
                    .map(|target| format!("{} {}", target.kind, target.name))
                    .unwrap_or_else(|| "def <unresolved>".to_string());
                writeln!(dump, "\n- {label} @ {}", self.render_source_span(span))
                    .expect("string writes should not fail");
            }
            SymbolAt::Expr { body, expr } => {
                let body_data = self
                    .db
                    .body_ir
                    .body_data(body)
                    .expect("body ref should exist while rendering analysis symbol");
                let expr_data = body_data
                    .expr(expr)
                    .expect("expr id should exist while rendering analysis symbol");
                writeln!(dump, "\n- {}", self.render_expr_symbol(expr_data))
                    .expect("string writes should not fail");
            }
            SymbolAt::Field { field, span } => {
                let targets = self
                    .db
                    .analysis()
                    .resolve_symbol(SymbolAt::Field { field, span });
                let label = targets
                    .first()
                    .map(|target| format!("{} {}", target.kind, target.name))
                    .unwrap_or_else(|| "field <unresolved>".to_string());
                writeln!(dump, "\n- {label} @ {}", self.render_source_span(span))
                    .expect("string writes should not fail");
            }
            SymbolAt::Function { function, span } => {
                let targets = self
                    .db
                    .analysis()
                    .resolve_symbol(SymbolAt::Function { function, span });
                let label = targets
                    .first()
                    .map(|target| format!("{} {}", target.kind, target.name))
                    .unwrap_or_else(|| "fn <unresolved>".to_string());
                writeln!(dump, "\n- {label} @ {}", self.render_source_span(span))
                    .expect("string writes should not fail");
            }
            SymbolAt::LocalItem { item, span } => {
                let label = self.render_body_item_ref(item);
                writeln!(dump, "\n- {label} @ {}", self.render_source_span(span))
                    .expect("string writes should not fail");
            }
            SymbolAt::TypePath { ref path, span, .. }
            | SymbolAt::UsePath { ref path, span, .. } => {
                writeln!(dump, "\n- path {path} @ {}", self.render_source_span(span))
                    .expect("string writes should not fail");
            }
        }
    }

    fn render_expr_symbol(&self, expr: &ExprData) -> String {
        let label = match &expr.kind {
            ExprKind::Block { .. } => "block".to_string(),
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
            ExprKind::Literal { kind, text } => format!("literal {kind} {text}"),
            ExprKind::Unknown { text, .. } => format!("unknown {text}"),
        };

        format!(
            "expr {label} @ {}",
            self.render_source_span(expr.source.span)
        )
    }

    fn render_targets(&self, mut targets: Vec<NavigationTarget>, dump: &mut String) {
        targets.sort_by_key(|target| {
            (
                target.kind,
                target.name.clone(),
                target.file_id.0,
                target.span.map(|span| span.text.start),
            )
        });

        if targets.is_empty() {
            writeln!(dump, "\n- <none>").expect("string writes should not fail");
            return;
        }

        writeln!(dump).expect("string writes should not fail");
        for target in targets {
            writeln!(
                dump,
                "- {} {} @ {}",
                target.kind,
                target.name,
                self.render_optional_span(target.span)
            )
            .expect("string writes should not fail");
        }
    }

    fn render_completions(&self, mut completions: Vec<CompletionItem>, dump: &mut String) {
        completions.sort_by_key(|completion| (completion.label.clone(), completion.kind));

        if completions.is_empty() {
            writeln!(dump, "\n- <none>").expect("string writes should not fail");
            return;
        }

        writeln!(dump).expect("string writes should not fail");
        for completion in completions {
            writeln!(dump, "- {} {}", completion.kind, completion.label)
                .expect("string writes should not fail");
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
        let body = self
            .db
            .body_ir
            .body_data(item_ref.body)
            .expect("body item body should exist while rendering analysis type");
        let item = body
            .local_item(item_ref.item)
            .expect("body item id should exist while rendering analysis type");

        format!(
            "{} {}::{}",
            item.kind,
            self.render_function_ref(body.owner),
            item.name
        )
    }

    fn render_type_def_ref(&self, ty: TypeDefRef) -> String {
        let target_ir = self
            .db
            .semantic_ir
            .target_ir(ty.target)
            .expect("target semantic IR should exist while rendering analysis type");

        match ty.id {
            TypeDefId::Struct(id) => {
                let data = target_ir
                    .items()
                    .struct_data(id)
                    .expect("struct id should exist while rendering analysis type");
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
                    .expect("enum id should exist while rendering analysis type");
                format!("enum {}::{}", self.render_module_ref(data.owner), data.name)
            }
            TypeDefId::Union(id) => {
                let data = target_ir
                    .items()
                    .union_data(id)
                    .expect("union id should exist while rendering analysis type");
                format!(
                    "union {}::{}",
                    self.render_module_ref(data.owner),
                    data.name
                )
            }
        }
    }

    fn render_function_ref(&self, function_ref: FunctionRef) -> String {
        let data = self
            .db
            .semantic_ir
            .function_data(function_ref)
            .expect("function ref should exist while rendering analysis body item");
        let owner = match data.owner {
            ItemOwner::Module(module_ref) => self.render_module_ref(module_ref),
            ItemOwner::Trait(trait_id) => {
                let trait_data = self
                    .db
                    .semantic_ir
                    .trait_data(TraitRef {
                        target: function_ref.target,
                        id: trait_id,
                    })
                    .expect("trait owner should exist while rendering analysis body item");
                format!(
                    "trait {}::{}",
                    self.render_module_ref(trait_data.owner),
                    trait_data.name
                )
            }
            ItemOwner::Impl(_) => "impl".to_string(),
        };

        format!("fn {owner}::{}", data.name)
    }

    fn render_module_ref(&self, module_ref: ModuleRef) -> String {
        let package = self
            .db
            .parse
            .packages()
            .get(module_ref.target.package.0)
            .expect("package slot should exist while rendering analysis module");
        let target = package
            .target(module_ref.target.target)
            .expect("target id should exist while rendering analysis module");

        format!(
            "{}[{}]::{}",
            package.package_name(),
            target.kind,
            self.module_path(module_ref),
        )
    }

    fn module_path(&self, module_ref: ModuleRef) -> String {
        let module = self
            .db
            .def_map
            .def_map(module_ref.target)
            .expect("target def map should exist while rendering analysis module path")
            .module(module_ref.module)
            .expect("module id should exist while rendering analysis module path");

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

    fn render_optional_span(&self, span: Option<Span>) -> String {
        span.map(|span| self.render_source_span(span))
            .unwrap_or_else(|| "<root>".to_string())
    }

    fn render_source_span(&self, span: Span) -> String {
        format!(
            "{}:{}-{}:{}",
            span.line_column.start.line + 1,
            span.line_column.start.column + 1,
            span.line_column.end.line + 1,
            span.line_column.end.column + 1,
        )
    }
}
