use std::collections::{HashMap, HashSet};

use anyhow::Context as _;
use ra_syntax::{
    AstNode as _,
    ast::{self, HasModuleItem, HasName, HasVisibility},
};

use crate::parse::{
    file::{FileId, ParseDb},
    item::{ItemKind, VisibilityLevel},
    package::PackageIndex,
    span::{LineIndex, Span},
    target::{TargetIndex, resolve_module_file},
};

use super::{
    DefId, DefMap, ImportBinding, ImportData, ImportId, ImportKind, ImportPath, LocalDefData,
    LocalDefId, LocalDefRef, ModuleData, ModuleId, ModuleOrigin, ModuleRef, ModuleScope,
    PackageSlot, PathSegment, ScopeBinding, TargetRef,
    data::{Namespace, namespace_for_local_kind},
};

pub(super) struct TargetState {
    pub(super) target: TargetRef,
    pub(super) target_name: String,
    pub(super) def_map: DefMap,
    pub(super) base_scopes: Vec<ModuleScope>,
    pub(super) implicit_roots: HashMap<String, ModuleRef>,
}

pub(super) fn collect_target_states(
    packages: &mut [PackageIndex],
    implicit_roots: &[Vec<HashMap<String, ModuleRef>>],
) -> anyhow::Result<Vec<Vec<TargetState>>> {
    let mut states = Vec::with_capacity(packages.len());

    for (package_slot, package) in packages.iter_mut().enumerate() {
        let mut package_states = Vec::with_capacity(package.targets.len());

        for target in &package.targets {
            let target_ref = TargetRef {
                package: PackageSlot(package_slot),
                target: target.id,
            };
            let target_roots = implicit_roots
                .get(package_slot)
                .and_then(|package_roots| package_roots.get(target.id.0))
                .expect("implicit roots should exist for every parsed target");

            let collector = TargetScopeCollector::new(target_ref, &mut package.db, target_roots);
            let state = collector.collect(target).with_context(|| {
                format!(
                    "while attempting to collect target scope for {}",
                    target.cargo_target.name
                )
            })?;
            package_states.push(state);
        }

        states.push(package_states);
    }

    Ok(states)
}

struct TargetScopeCollector<'db> {
    target: TargetRef,
    parse_db: &'db mut ParseDb,
    implicit_roots: &'db HashMap<String, ModuleRef>,
    active_files: HashSet<FileId>,
    def_map: DefMap,
    base_scopes: Vec<ModuleScope>,
}

impl<'db> TargetScopeCollector<'db> {
    fn new(
        target: TargetRef,
        parse_db: &'db mut ParseDb,
        implicit_roots: &'db HashMap<String, ModuleRef>,
    ) -> Self {
        Self {
            target,
            parse_db,
            implicit_roots,
            active_files: HashSet::default(),
            def_map: DefMap::default(),
            base_scopes: Vec::new(),
        }
    }

    fn collect(mut self, target: &TargetIndex) -> anyhow::Result<TargetState> {
        let root_module = self.alloc_module(
            None,
            None,
            ModuleOrigin::Root {
                file_id: target.root_file,
            },
        );
        self.def_map.set_root_module(root_module);

        self.collect_file_items(root_module, target.root_file)
            .with_context(|| {
                format!(
                    "while attempting to collect source items for {}",
                    target.cargo_target.name
                )
            })?;

        Ok(TargetState {
            target: self.target,
            target_name: target.cargo_target.name.clone(),
            def_map: self.def_map,
            base_scopes: self.base_scopes,
            implicit_roots: self.implicit_roots.clone(),
        })
    }

    fn alloc_module(
        &mut self,
        parent: Option<ModuleId>,
        name: Option<String>,
        origin: ModuleOrigin,
    ) -> ModuleId {
        let module_id = ModuleId(self.def_map.modules.len());
        self.def_map.modules.push(ModuleData {
            name,
            parent,
            children: Vec::new(),
            local_defs: Vec::new(),
            imports: Vec::new(),
            scope: ModuleScope::default(),
            origin,
        });
        self.base_scopes.push(ModuleScope::default());
        module_id
    }

    fn collect_file_items(&mut self, module_id: ModuleId, file_id: FileId) -> anyhow::Result<()> {
        if !self.active_files.insert(file_id) {
            return Ok(());
        }

        let (items, line_index) = {
            let parsed_file = self
                .parse_db
                .parsed_file(file_id)
                .with_context(|| format!("while attempting to fetch parsed file {:?}", file_id))?;
            (
                parsed_file.tree.items().collect::<Vec<_>>(),
                parsed_file.line_index.clone(),
            )
        };

        self.collect_items(module_id, items, file_id, &line_index)
            .with_context(|| format!("while attempting to collect file items for {:?}", file_id))?;

        self.active_files.remove(&file_id);
        Ok(())
    }

    fn collect_items(
        &mut self,
        module_id: ModuleId,
        items: Vec<ast::Item>,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> anyhow::Result<()> {
        for item in items {
            match item {
                ast::Item::AsmExpr(_) => {}
                ast::Item::Const(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Const,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Enum(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Enum,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::ExternBlock(_) => {}
                ast::Item::ExternCrate(item) => self.collect_extern_crate(module_id, item),
                ast::Item::Fn(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Function,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Impl(_) => {}
                ast::Item::MacroCall(_) => {}
                ast::Item::MacroDef(item) => self.collect_local_def(
                    module_id,
                    ItemKind::MacroDefinition,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::MacroRules(item) => self.collect_local_def(
                    module_id,
                    ItemKind::MacroDefinition,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Module(item) => self
                    .collect_module(module_id, item, file_id, line_index)
                    .context("while attempting to collect module item")?,
                ast::Item::Static(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Static,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Struct(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Struct,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Trait(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Trait,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::TypeAlias(item) => self.collect_local_def(
                    module_id,
                    ItemKind::TypeAlias,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Union(item) => self.collect_local_def(
                    module_id,
                    ItemKind::Union,
                    item.name().map(|name| name.text().to_string()),
                    VisibilityLevel::from_ast(item.visibility()),
                    file_id,
                    Span::from_text_range(item.syntax().text_range(), line_index),
                ),
                ast::Item::Use(item) => self.collect_use(module_id, item),
            }
        }

        Ok(())
    }

    fn collect_local_def(
        &mut self,
        module_id: ModuleId,
        kind: ItemKind,
        name: Option<String>,
        visibility: VisibilityLevel,
        file_id: FileId,
        span: Span,
    ) {
        let Some(name) = name else {
            return;
        };
        let Some(namespace) = namespace_for_local_kind(kind) else {
            return;
        };

        let local_def_id = LocalDefId(self.def_map.local_defs.len());
        self.def_map.local_defs.push(LocalDefData {
            module: module_id,
            name: name.clone(),
            kind,
            visibility: visibility.clone(),
            file_id,
            span,
        });
        self.def_map
            .modules
            .get_mut(module_id.0)
            .expect("module should exist for collected local definition")
            .local_defs
            .push(local_def_id);
        self.base_scopes
            .get_mut(module_id.0)
            .expect("base scope should exist for collected local definition")
            .insert_binding(
                &name,
                namespace,
                ScopeBinding {
                    def: DefId::Local(LocalDefRef {
                        target: self.target,
                        local_def: local_def_id,
                    }),
                    visibility,
                },
            );
    }

    fn collect_module(
        &mut self,
        parent_module: ModuleId,
        item: ast::Module,
        file_id: FileId,
        line_index: &LineIndex,
    ) -> anyhow::Result<()> {
        let Some(module_name) = item.name().map(|name| name.text().to_string()) else {
            return Ok(());
        };
        let visibility = VisibilityLevel::from_ast(item.visibility());
        let declaration_span = Span::from_text_range(item.syntax().text_range(), line_index);

        if let Some(item_list) = item.item_list() {
            let child_module = self.alloc_module(
                Some(parent_module),
                Some(module_name.clone()),
                ModuleOrigin::Inline {
                    declaration_file: file_id,
                    declaration_span,
                },
            );
            self.link_child_module(parent_module, child_module, &module_name, visibility);

            let inline_items = item_list.items().collect::<Vec<_>>();
            self.collect_items(child_module, inline_items, file_id, line_index)
                .with_context(|| {
                    format!("while attempting to collect inline module {module_name}")
                })?;
            return Ok(());
        }

        let current_file_path = self
            .parse_db
            .file_path(file_id)
            .with_context(|| format!("while attempting to resolve current file {:?}", file_id))?;
        let module_file_path = resolve_module_file(current_file_path, &module_name);
        let definition_file = if let Some(module_file_path) = module_file_path {
            Some(
                self.parse_db
                    .get_or_parse_file(&module_file_path)
                    .with_context(|| {
                        format!(
                            "while attempting to parse module file {}",
                            module_file_path.display()
                        )
                    })?,
            )
        } else {
            None
        };

        let child_module = self.alloc_module(
            Some(parent_module),
            Some(module_name.clone()),
            ModuleOrigin::OutOfLine {
                declaration_file: file_id,
                declaration_span,
                definition_file,
            },
        );
        self.link_child_module(parent_module, child_module, &module_name, visibility);

        if let Some(definition_file) = definition_file {
            self.collect_file_items(child_module, definition_file)
                .with_context(|| {
                    format!("while attempting to collect out-of-line module {module_name}")
                })?;
        }

        Ok(())
    }

    fn link_child_module(
        &mut self,
        parent_module: ModuleId,
        child_module: ModuleId,
        module_name: &str,
        visibility: VisibilityLevel,
    ) {
        self.def_map
            .modules
            .get_mut(parent_module.0)
            .expect("parent module should exist for child link")
            .children
            .push((module_name.to_string(), child_module));
        self.base_scopes
            .get_mut(parent_module.0)
            .expect("base scope should exist for child link")
            .insert_binding(
                module_name,
                Namespace::Types,
                ScopeBinding {
                    def: DefId::Module(ModuleRef {
                        target: self.target,
                        module: child_module,
                    }),
                    visibility,
                },
            );
    }

    fn collect_use(&mut self, module_id: ModuleId, item: ast::Use) {
        let visibility = VisibilityLevel::from_ast(item.visibility());
        let Some(use_tree) = item.use_tree() else {
            return;
        };

        self.lower_use_tree(module_id, &ImportPath::empty(), visibility, use_tree);
    }

    fn lower_use_tree(
        &mut self,
        module_id: ModuleId,
        prefix: &ImportPath,
        visibility: VisibilityLevel,
        use_tree: ast::UseTree,
    ) {
        let path = use_tree
            .path()
            .and_then(|path| lower_path(&path))
            .unwrap_or_else(ImportPath::empty);
        let path = prefix.joined(&path);

        if let Some(use_tree_list) = use_tree.use_tree_list() {
            for child_use_tree in use_tree_list.use_trees() {
                self.lower_use_tree(module_id, &path, visibility.clone(), child_use_tree);
            }
            return;
        }

        let binding = ImportBinding::from_rename(use_tree.rename());

        let (kind, path) = if use_tree.star_token().is_some() {
            (ImportKind::Glob, path)
        } else if path.ends_with_self() {
            // `use foo::{self}` should resolve the `foo` module and bind it under `foo`
            // (or an explicit alias), so the lowered import keeps the module path.
            (ImportKind::SelfImport, path.without_trailing_self())
        } else {
            (ImportKind::Named, path)
        };

        if path.segments.is_empty() {
            return;
        }

        let import_id = ImportId(self.def_map.imports.len());
        self.def_map.imports.push(ImportData {
            module: module_id,
            visibility,
            kind,
            path,
            binding,
        });
        self.def_map
            .modules
            .get_mut(module_id.0)
            .expect("module should exist for lowered import")
            .imports
            .push(import_id);
    }

    fn collect_extern_crate(&mut self, module_id: ModuleId, item: ast::ExternCrate) {
        let Some(extern_name) = item.name_ref().map(|name_ref| name_ref.text().to_string()) else {
            return;
        };
        let Some(binding_name) =
            ImportBinding::from_rename(item.rename()).resolve(Some(extern_name.clone()))
        else {
            return;
        };

        let module_ref = if extern_name == "self" {
            ModuleRef {
                target: self.target,
                module: self
                    .def_map
                    .root_module()
                    .expect("root module should exist before extern crate collection"),
            }
        } else {
            let Some(module_ref) = self.implicit_roots.get(&extern_name).copied() else {
                return;
            };
            module_ref
        };

        let visibility = VisibilityLevel::from_ast(item.visibility());
        self.base_scopes
            .get_mut(module_id.0)
            .expect("base scope should exist for extern crate binding")
            .insert_binding(
                &binding_name,
                Namespace::Types,
                ScopeBinding {
                    def: DefId::Module(module_ref),
                    visibility,
                },
            );
    }
}

fn lower_path(path: &ast::Path) -> Option<ImportPath> {
    let mut segments = Vec::new();

    for segment in path.segments() {
        let lowered_segment = match segment.kind()? {
            ast::PathSegmentKind::Name(name_ref) => PathSegment::Name(name_ref.text().to_string()),
            ast::PathSegmentKind::SelfKw => PathSegment::SelfKw,
            ast::PathSegmentKind::SuperKw => PathSegment::SuperKw,
            ast::PathSegmentKind::CrateKw => PathSegment::CrateKw,
            ast::PathSegmentKind::SelfTypeKw | ast::PathSegmentKind::Type { .. } => return None,
        };
        segments.push(lowered_segment);
    }

    Some(ImportPath {
        absolute: path
            .first_segment()
            .is_some_and(|segment| segment.coloncolon_token().is_some()),
        segments,
    })
}
