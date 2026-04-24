use std::fmt::Write as _;

use expect_test::expect;

use crate::{
    Project,
    def_map::{ModuleId, TargetRef},
    item_tree::{FieldItem, FieldList, ParamKind, VisibilityLevel},
    test_utils::fixture_crate,
};

use super::ids::{AssocItemId, ItemId};

#[test]
fn dumps_semantic_ir_signatures() {
    check_project_semantic_ir(
        r#"
//- /Cargo.toml
[package]
name = "semantic_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct User<T> {
    pub id: UserId,
    payload: Option<T>,
}

pub struct UserId(u64);

pub enum LoadState<E> {
    Empty,
    Loaded(User),
    Failed { error: E },
}

pub trait Repository<T>
where
    T: Clone,
{
    type Error;
    const KIND: &'static str;
    fn get(&self, id: UserId) -> Result<T, Self::Error>;
}

pub struct DbRepository<T>(T);

impl<T> Repository<T> for DbRepository<T>
where
    T: Clone,
{
    type Error = DbError;
    const KIND: &'static str = "db";
    fn get(&self, id: UserId) -> Result<T, DbError> {
        todo!()
    }
}

pub struct DbError;

pub type UserResult<T> = Result<User<T>, DbError>;
pub const DEFAULT_ID: UserId = UserId(0);
pub static mut CACHE_READY: bool = false;
"#,
        expect![[r#"
            package semantic_fixture

            semantic_fixture [lib]
            crate
            - pub struct User<T>
              - pub field id: UserId
              - field payload: Option<T>
            - pub struct UserId
              - field #0: u64
            - pub enum LoadState<E>
              - variant Empty
              - variant Loaded
                - field #0: User
              - variant Failed
                - field error: E
            - pub trait Repository<T> where T: Clone
              - type Error
              - const KIND: &'static str
              - fn get(&self, id: UserId) -> Result<T, Self::Error>
            - pub struct DbRepository<T>
              - field #0: T
            - pub struct DbError
            - pub type UserResult<T> = Result<User<T>, DbError>
            - pub const DEFAULT_ID: UserId
            - pub static mut CACHE_READY: bool
            - impl<T> Repository<T> for DbRepository<T> where T: Clone
              - type Error = DbError
              - const KIND: &'static str
              - fn get(&self, id: UserId) -> Result<T, DbError>
        "#]],
    );
}

#[test]
fn preserves_absolute_type_path_prefixes() {
    check_project_semantic_ir(
        r#"
//- /Cargo.toml
[package]
name = "absolute_type_fixture"
version = "0.1.0"
edition = "2024"

//- /src/lib.rs
pub struct Root;
pub struct UsesAbsolute(::absolute_type_fixture::Root);
pub type AbsoluteAlias = ::absolute_type_fixture::Root;
"#,
        expect![[r#"
            package absolute_type_fixture

            absolute_type_fixture [lib]
            crate
            - pub struct Root
            - pub struct UsesAbsolute
              - field #0: ::absolute_type_fixture::Root
            - pub type AbsoluteAlias = ::absolute_type_fixture::Root
        "#]],
    );
}

fn check_project_semantic_ir(fixture: &str, expect: expect_test::Expect) {
    let project = fixture_crate!(fixture).project();
    let actual = ProjectSemanticIrSnapshot::new(&project).render();
    let actual = format!("{}\n", actual.trim_end());
    expect.assert_eq(&actual);
}

struct ProjectSemanticIrSnapshot<'a> {
    project: &'a Project,
}

impl<'a> ProjectSemanticIrSnapshot<'a> {
    fn new(project: &'a Project) -> Self {
        Self { project }
    }

    fn render(&self) -> String {
        let mut packages = self
            .project
            .parse_db()
            .packages()
            .iter()
            .enumerate()
            .collect::<Vec<_>>();
        packages.sort_by(|left, right| left.1.package_name().cmp(right.1.package_name()));

        packages
            .into_iter()
            .map(|(package_slot, package)| {
                let mut targets = package.targets().iter().collect::<Vec<_>>();
                targets.sort_by(|left, right| {
                    (
                        left.kind.sort_order(),
                        left.name.as_str(),
                        left.src_path.as_path(),
                    )
                        .cmp(&(
                            right.kind.sort_order(),
                            right.name.as_str(),
                            right.src_path.as_path(),
                        ))
                });

                let target_dumps = targets
                    .into_iter()
                    .map(|target| {
                        TargetSemanticIrSnapshot {
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

struct TargetSemanticIrSnapshot<'a> {
    project: &'a Project,
    target_ref: TargetRef,
    target_name: &'a str,
    target_kind: String,
}

impl TargetSemanticIrSnapshot<'_> {
    fn render(&self) -> String {
        let mut dump = format!("{} [{}]\n", self.target_name, self.target_kind);
        let def_map = self
            .project
            .def_map_db()
            .def_map(self.target_ref)
            .expect("target def map should exist while rendering semantic IR");
        let target_ir = self
            .project
            .semantic_ir_db()
            .target_ir(self.target_ref)
            .expect("target semantic IR should exist while rendering");

        for (idx, (module_path, module_id)) in self.sorted_modules().into_iter().enumerate() {
            if idx > 0 {
                dump.push('\n');
            }

            writeln!(&mut dump, "{module_path}").expect("string writes should not fail");
            let module = def_map
                .module(module_id)
                .expect("module id should exist while rendering semantic IR");

            for local_def in &module.local_defs {
                let Some(item_id) = target_ir.item_for_local_def(*local_def) else {
                    continue;
                };
                self.render_item(item_id, 0, &mut dump);
            }

            for local_impl in &module.impls {
                let impl_id = target_ir
                    .impls()
                    .get(local_impl.0)
                    .copied()
                    .expect("local impl id should map to semantic impl id");
                self.render_impl(impl_id, 0, &mut dump);
            }
        }

        dump
    }

    fn render_item(&self, item_id: ItemId, depth: usize, dump: &mut String) {
        match item_id {
            ItemId::Struct(id) => {
                let data = self
                    .items()
                    .struct_data(id)
                    .expect("struct id should exist while rendering");
                writeln!(
                    dump,
                    "{}- {}struct {}{}",
                    indent(depth),
                    visibility_prefix(&data.visibility),
                    data.name,
                    generic_params(&data.generics),
                )
                .expect("string writes should not fail");
                self.render_fields(&data.fields, depth + 1, dump);
            }
            ItemId::Union(id) => {
                let data = self
                    .items()
                    .union_data(id)
                    .expect("union id should exist while rendering");
                writeln!(
                    dump,
                    "{}- {}union {}{}",
                    indent(depth),
                    visibility_prefix(&data.visibility),
                    data.name,
                    generic_params(&data.generics),
                )
                .expect("string writes should not fail");
                self.render_named_fields(&data.fields, depth + 1, dump);
            }
            ItemId::Enum(id) => {
                let data = self
                    .items()
                    .enum_data(id)
                    .expect("enum id should exist while rendering");
                writeln!(
                    dump,
                    "{}- {}enum {}{}",
                    indent(depth),
                    visibility_prefix(&data.visibility),
                    data.name,
                    generic_params(&data.generics),
                )
                .expect("string writes should not fail");
                for variant in &data.variants {
                    writeln!(dump, "{}- variant {}", indent(depth + 1), variant.name)
                        .expect("string writes should not fail");
                    self.render_fields(&variant.fields, depth + 2, dump);
                }
            }
            ItemId::Trait(id) => {
                let data = self
                    .items()
                    .trait_data(id)
                    .expect("trait id should exist while rendering");
                let super_traits = if data.super_traits.is_empty() {
                    String::new()
                } else {
                    format!(
                        ": {}",
                        data.super_traits
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(" + ")
                    )
                };
                writeln!(
                    dump,
                    "{}- {}trait {}{}{}{}",
                    indent(depth),
                    visibility_prefix(&data.visibility),
                    data.name,
                    generic_params(&data.generics),
                    super_traits,
                    where_clause(&data.generics),
                )
                .expect("string writes should not fail");
                for assoc_item in &data.items {
                    self.render_assoc_item(*assoc_item, depth + 1, dump);
                }
            }
            ItemId::Function(id) => self.render_function(id, depth, dump),
            ItemId::TypeAlias(id) => self.render_type_alias(id, depth, dump),
            ItemId::Const(id) => self.render_const(id, depth, dump),
            ItemId::Static(id) => {
                let data = self
                    .items()
                    .static_data(id)
                    .expect("static id should exist while rendering");
                let mutability = match data.mutability {
                    crate::item_tree::Mutability::Shared => "",
                    crate::item_tree::Mutability::Mutable => "mut ",
                };
                let ty = data
                    .ty
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "<unknown>".to_string());
                writeln!(
                    dump,
                    "{}- {}static {mutability}{}: {ty}",
                    indent(depth),
                    visibility_prefix(&data.visibility),
                    data.name,
                )
                .expect("string writes should not fail");
            }
        }
    }

    fn render_impl(&self, id: super::ids::ImplId, depth: usize, dump: &mut String) {
        let data = self
            .items()
            .impl_data(id)
            .expect("impl id should exist while rendering");
        match &data.trait_ref {
            Some(trait_ref) => writeln!(
                dump,
                "{}- impl{} {} for {}{}",
                indent(depth),
                generic_params(&data.generics),
                trait_ref,
                data.self_ty,
                where_clause(&data.generics),
            )
            .expect("string writes should not fail"),
            None => writeln!(
                dump,
                "{}- impl{} {}{}",
                indent(depth),
                generic_params(&data.generics),
                data.self_ty,
                where_clause(&data.generics),
            )
            .expect("string writes should not fail"),
        }
        for assoc_item in &data.items {
            self.render_assoc_item(*assoc_item, depth + 1, dump);
        }
    }

    fn render_assoc_item(&self, item_id: AssocItemId, depth: usize, dump: &mut String) {
        match item_id {
            AssocItemId::Function(id) => self.render_function(id, depth, dump),
            AssocItemId::TypeAlias(id) => self.render_type_alias(id, depth, dump),
            AssocItemId::Const(id) => self.render_const(id, depth, dump),
        }
    }

    fn render_function(&self, id: super::ids::FunctionId, depth: usize, dump: &mut String) {
        let data = self
            .items()
            .function_data(id)
            .expect("function id should exist while rendering");
        let params = data
            .declaration
            .params
            .iter()
            .map(render_param)
            .collect::<Vec<_>>()
            .join(", ");
        let ret_ty = data
            .declaration
            .ret_ty
            .as_ref()
            .map(|ty| format!(" -> {ty}"))
            .unwrap_or_default();
        writeln!(
            dump,
            "{}- {}fn {}{}({params}){ret_ty}{}",
            indent(depth),
            visibility_prefix(&data.visibility),
            data.name,
            generic_params(&data.declaration.generics),
            where_clause(&data.declaration.generics),
        )
        .expect("string writes should not fail");
    }

    fn render_type_alias(&self, id: super::ids::TypeAliasId, depth: usize, dump: &mut String) {
        let data = self
            .items()
            .type_alias_data(id)
            .expect("type alias id should exist while rendering");
        let bounds = if data.declaration.bounds.is_empty() {
            String::new()
        } else {
            format!(
                ": {}",
                data.declaration
                    .bounds
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" + ")
            )
        };
        let aliased_ty = data
            .declaration
            .aliased_ty
            .as_ref()
            .map(|ty| format!(" = {ty}"))
            .unwrap_or_default();
        writeln!(
            dump,
            "{}- {}type {}{}{}{}{}",
            indent(depth),
            visibility_prefix(&data.visibility),
            data.name,
            generic_params(&data.declaration.generics),
            bounds,
            where_clause(&data.declaration.generics),
            aliased_ty,
        )
        .expect("string writes should not fail");
    }

    fn render_const(&self, id: super::ids::ConstId, depth: usize, dump: &mut String) {
        let data = self
            .items()
            .const_data(id)
            .expect("const id should exist while rendering");
        let ty = data
            .declaration
            .ty
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "<unknown>".to_string());
        writeln!(
            dump,
            "{}- {}const {}: {ty}",
            indent(depth),
            visibility_prefix(&data.visibility),
            data.name,
        )
        .expect("string writes should not fail");
    }

    fn render_fields(&self, fields: &FieldList, depth: usize, dump: &mut String) {
        match fields {
            FieldList::Named(fields) => self.render_named_fields(fields, depth, dump),
            FieldList::Tuple(fields) => {
                for (idx, field) in fields.iter().enumerate() {
                    writeln!(
                        dump,
                        "{}- {}field #{idx}: {}",
                        indent(depth),
                        visibility_prefix(&field.visibility),
                        field.ty,
                    )
                    .expect("string writes should not fail");
                }
            }
            FieldList::Unit => {}
        }
    }

    fn render_named_fields(&self, fields: &[FieldItem], depth: usize, dump: &mut String) {
        for field in fields {
            writeln!(
                dump,
                "{}- {}field {}: {}",
                indent(depth),
                visibility_prefix(&field.visibility),
                field.name.as_deref().unwrap_or("<missing>"),
                field.ty,
            )
            .expect("string writes should not fail");
        }
    }

    fn sorted_modules(&self) -> Vec<(String, ModuleId)> {
        let def_map = self
            .project
            .def_map_db()
            .def_map(self.target_ref)
            .expect("target def map should exist while sorting semantic IR modules");
        let mut modules = def_map
            .modules()
            .iter()
            .enumerate()
            .map(|(idx, _)| {
                let module_id = ModuleId(idx);
                (self.module_path(module_id), module_id)
            })
            .collect::<Vec<_>>();
        modules.sort_by(|left, right| left.0.cmp(&right.0));
        modules
    }

    fn module_path(&self, module_id: ModuleId) -> String {
        let module = self
            .project
            .def_map_db()
            .def_map(self.target_ref)
            .expect("target def map should exist while rendering module path")
            .module(module_id)
            .expect("module id should exist while rendering module path");

        match module.parent {
            Some(parent) => {
                let parent_path = self.module_path(parent);
                let name = module
                    .name
                    .as_deref()
                    .expect("non-root modules should have names");
                format!("{parent_path}::{name}")
            }
            None => "crate".to_string(),
        }
    }

    fn items(&self) -> &super::data::ItemStore {
        self.project
            .semantic_ir_db()
            .target_ir(self.target_ref)
            .expect("target semantic IR should exist while rendering items")
            .items()
    }
}

fn render_param(param: &crate::item_tree::ParamItem) -> String {
    match (param.kind, &param.ty) {
        (ParamKind::SelfParam, _) => param.pat.clone(),
        (ParamKind::Normal, Some(ty)) => format!("{}: {ty}", param.pat),
        (ParamKind::Normal, None) => param.pat.clone(),
    }
}

fn generic_params(generics: &crate::item_tree::GenericParams) -> String {
    let mut generics = generics.clone();
    generics.where_predicates.clear();
    generics.to_string()
}

fn where_clause(generics: &crate::item_tree::GenericParams) -> String {
    if generics.where_predicates.is_empty() {
        return String::new();
    }

    format!(
        " where {}",
        generics
            .where_predicates
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn visibility_prefix(visibility: &VisibilityLevel) -> String {
    match visibility {
        VisibilityLevel::Private => String::new(),
        _ => format!("{visibility} "),
    }
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}
