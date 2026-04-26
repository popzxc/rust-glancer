pub use rg_def_map as def_map;
pub use rg_item_tree as item_tree;
pub use rg_parse as parse;
pub use rg_semantic_ir as semantic_ir;
pub use rg_workspace as workspace_metadata;

pub mod body_ir;

pub use self::body_ir::*;

#[cfg(test)]
pub use test_fixture;
