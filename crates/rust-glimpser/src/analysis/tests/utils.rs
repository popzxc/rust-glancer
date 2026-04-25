use std::fmt::Write as _;

use expect_test::Expect;

use crate::{
    Project,
    analysis::{CompletionItem, NavigationTarget},
    body_ir::BodyTy,
    def_map::{ModuleRef, PackageSlot, TargetRef},
    parse::FileId,
    semantic_ir::{TypeDefId, TypeDefRef},
    test_utils::{FixtureMarkers, fixture_crate_with_markers},
    workspace_metadata::TargetKind,
};

pub(super) fn check_analysis_queries(fixture: &str, queries: &[AnalysisQuery], expect: Expect) {
    let (fixture, markers) = fixture_crate_with_markers(fixture);
    let project = fixture.project();
    let renderer = AnalysisQuerySnapshot::new(&project, markers, queries);
    let actual = format!("{}\n", renderer.render().trim_end());
    expect.assert_eq(&actual);
}

pub(super) struct AnalysisQuery {
    title: &'static str,
    marker: &'static str,
    kind: AnalysisQueryKind,
}

impl AnalysisQuery {
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
    GotoDefinition,
    TypeAt,
    CompletionsAtDot,
}

struct AnalysisQuerySnapshot<'a> {
    project: &'a Project,
    markers: FixtureMarkers,
    queries: &'a [AnalysisQuery],
}

impl<'a> AnalysisQuerySnapshot<'a> {
    fn new(project: &'a Project, markers: FixtureMarkers, queries: &'a [AnalysisQuery]) -> Self {
        Self {
            project,
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
            AnalysisQueryKind::GotoDefinition => {
                self.render_targets(
                    self.project
                        .analysis()
                        .goto_definition(target, file_id, offset),
                    &mut dump,
                );
            }
            AnalysisQueryKind::TypeAt => {
                let ty = self.project.analysis().type_at(target, file_id, offset);
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
                    self.project
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
            .project
            .parse_db()
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
                target
                    .span
                    .map(|span| format!(
                        "{}:{}-{}:{}",
                        span.line_column.start.line + 1,
                        span.line_column.start.column + 1,
                        span.line_column.end.line + 1,
                        span.line_column.end.column + 1,
                    ))
                    .unwrap_or_else(|| "<root>".to_string())
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

    fn render_type_def_ref(&self, ty: TypeDefRef) -> String {
        let target_ir = self
            .project
            .semantic_ir_db()
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

    fn render_module_ref(&self, module_ref: ModuleRef) -> String {
        let package = self
            .project
            .parse_db()
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
            .project
            .def_map_db()
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
}
