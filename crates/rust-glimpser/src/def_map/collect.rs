use std::collections::HashMap;

use anyhow::Context as _;

use crate::{
    item_tree::{
        ExternCrateItem, ItemKind, ItemNode, ItemTreeDb, ModuleSource, UseImport, UseItem,
    },
    parse::{Package, Target},
};

use super::{
    DefId, DefMap, ImportBinding, ImportData, ImportId, ImportKind, ImportPath, LocalDefData,
    LocalDefId, LocalDefRef, ModuleData, ModuleId, ModuleOrigin, ModuleRef, ModuleScope,
    PackageSlot, ScopeBinding, TargetRef,
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
    packages: &[Package],
    item_tree: &ItemTreeDb,
    implicit_roots: &[Vec<HashMap<String, ModuleRef>>],
) -> anyhow::Result<Vec<Vec<TargetState>>> {
    let mut states = Vec::with_capacity(packages.len());

    for (package_slot, package) in packages.iter().enumerate() {
        let item_tree_package = item_tree.package(package_slot).with_context(|| {
            format!(
                "while attempting to fetch item tree package for {}",
                package.package_name()
            )
        })?;
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
            let target_tree = item_tree_package.target(target.id).with_context(|| {
                format!(
                    "while attempting to fetch item tree target for {}",
                    target.cargo_target.name
                )
            })?;

            let collector = TargetScopeCollector::new(target_ref, target_roots);
            let state = collector.collect(target, &target_tree.root_items);
            package_states.push(state);
        }

        states.push(package_states);
    }

    Ok(states)
}

struct TargetScopeCollector<'db> {
    target: TargetRef,
    implicit_roots: &'db HashMap<String, ModuleRef>,
    def_map: DefMap,
    base_scopes: Vec<ModuleScope>,
}

impl<'db> TargetScopeCollector<'db> {
    fn new(target: TargetRef, implicit_roots: &'db HashMap<String, ModuleRef>) -> Self {
        Self {
            target,
            implicit_roots,
            def_map: DefMap::default(),
            base_scopes: Vec::new(),
        }
    }

    fn collect(mut self, target: &Target, root_items: &[ItemNode]) -> TargetState {
        let root_module = self.alloc_module(
            None,
            None,
            ModuleOrigin::Root {
                file_id: target.root_file,
            },
        );
        self.def_map.set_root_module(root_module);

        self.collect_items(root_module, root_items);

        TargetState {
            target: self.target,
            target_name: target.cargo_target.name.clone(),
            def_map: self.def_map,
            base_scopes: self.base_scopes,
            implicit_roots: self.implicit_roots.clone(),
        }
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

    fn collect_items(&mut self, module_id: ModuleId, items: &[ItemNode]) {
        for item in items {
            match &item.kind {
                ItemKind::ExternCrate(extern_crate) => {
                    self.collect_extern_crate(module_id, item, extern_crate);
                }
                ItemKind::Module(module_item) => {
                    self.collect_module(module_id, item, &module_item.source);
                }
                ItemKind::Use(use_item) => {
                    self.collect_use(module_id, item, use_item);
                }
                _ => self.collect_local_def(module_id, item),
            }
        }
    }

    fn collect_local_def(&mut self, module_id: ModuleId, item: &ItemNode) {
        let kind = item.kind.tag();
        let Some(namespace) = namespace_for_local_kind(kind) else {
            return;
        };
        let Some(name) = item.name.clone() else {
            return;
        };

        let local_def_id = LocalDefId(self.def_map.local_defs.len());
        self.def_map.local_defs.push(LocalDefData {
            module: module_id,
            name: name.clone(),
            kind,
            visibility: item.visibility.clone(),
            file_id: item.file_id,
            span: item.span,
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
                    visibility: item.visibility.clone(),
                },
            );
    }

    fn collect_module(&mut self, parent_module: ModuleId, item: &ItemNode, source: &ModuleSource) {
        let Some(module_name) = item.name.clone() else {
            return;
        };

        let origin = match source {
            ModuleSource::Inline => ModuleOrigin::Inline {
                declaration_file: item.file_id,
                declaration_span: item.span,
            },
            ModuleSource::OutOfLine { definition_file } => ModuleOrigin::OutOfLine {
                declaration_file: item.file_id,
                declaration_span: item.span,
                definition_file: *definition_file,
            },
        };
        let child_module =
            self.alloc_module(Some(parent_module), Some(module_name.clone()), origin);
        self.link_child_module(
            parent_module,
            child_module,
            &module_name,
            item.visibility.clone(),
        );
        self.collect_items(child_module, &item.children);
    }

    fn link_child_module(
        &mut self,
        parent_module: ModuleId,
        child_module: ModuleId,
        module_name: &str,
        visibility: crate::item_tree::VisibilityLevel,
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

    fn collect_use(&mut self, module_id: ModuleId, item: &ItemNode, use_item: &UseItem) {
        let imports: &[UseImport] = &use_item.imports;

        for import in imports {
            let path = ImportPath::from_use_path(&import.path);
            if path.segments.is_empty() {
                continue;
            }

            let import_id = ImportId(self.def_map.imports.len());
            self.def_map.imports.push(ImportData {
                module: module_id,
                visibility: item.visibility.clone(),
                kind: ImportKind::from_use_kind(import.kind),
                path,
                binding: ImportBinding::from_alias(&import.alias),
            });
            self.def_map
                .modules
                .get_mut(module_id.0)
                .expect("module should exist for lowered import")
                .imports
                .push(import_id);
        }
    }

    fn collect_extern_crate(
        &mut self,
        module_id: ModuleId,
        item: &ItemNode,
        extern_crate: &ExternCrateItem,
    ) {
        let Some(extern_name) = extern_crate.name.clone() else {
            return;
        };
        let Some(binding_name) =
            ImportBinding::from_alias(&extern_crate.alias).resolve(Some(extern_name.clone()))
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

        self.base_scopes
            .get_mut(module_id.0)
            .expect("base scope should exist for extern crate binding")
            .insert_binding(
                &binding_name,
                Namespace::Types,
                ScopeBinding {
                    def: DefId::Module(module_ref),
                    visibility: item.visibility.clone(),
                },
            );
    }
}
