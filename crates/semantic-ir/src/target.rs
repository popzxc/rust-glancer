use rg_def_map::LocalDefId;

use crate::{ImplId, ItemId, ItemStore};

/// Semantic IR for one target root.
///
/// The target keeps two indexes back into DefMap collection results:
/// local defs map to semantic item ids, and local impls map to semantic impl ids. Those links let
/// later phases move from name resolution into semantic signatures without re-lowering source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIr {
    pub(crate) local_items: Vec<Option<ItemId>>,
    pub(crate) local_impls: Vec<ImplId>,
    pub(crate) items: ItemStore,
}

impl TargetIr {
    pub(crate) fn new(local_def_count: usize) -> Self {
        Self {
            local_items: vec![None; local_def_count],
            local_impls: Vec::new(),
            items: ItemStore::default(),
        }
    }

    /// Returns the semantic item lowered from one DefMap local definition.
    pub fn item_for_local_def(&self, local_def: LocalDefId) -> Option<ItemId> {
        self.local_items.get(local_def.0).copied().flatten()
    }

    /// Returns semantic impl ids in the same order as target-local impl lowering.
    pub fn impls(&self) -> &[ImplId] {
        &self.local_impls
    }

    /// Returns target-local semantic item storage.
    pub fn items(&self) -> &ItemStore {
        &self.items
    }

    pub(crate) fn set_local_item(&mut self, local_def: LocalDefId, item: ItemId) {
        let slot = self
            .local_items
            .get_mut(local_def.0)
            .expect("local item slot should exist while building semantic IR");
        *slot = Some(item);
    }

    pub(crate) fn push_local_impl(&mut self, impl_id: ImplId) {
        self.local_impls.push(impl_id);
    }

    pub(crate) fn items_mut(&mut self) -> &mut ItemStore {
        &mut self.items
    }
}
