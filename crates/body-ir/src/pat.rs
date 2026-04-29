use rg_item_tree::FieldKey;

use crate::{
    body::BodySource,
    ids::{BindingId, PatId},
    path::BodyPath,
};

/// One lowered pattern node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatData {
    pub source: BodySource,
    pub kind: PatKind,
}

/// Pattern forms that matter for binding and enum-payload type propagation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatKind {
    Binding {
        binding: Option<BindingId>,
        subpat: Option<PatId>,
    },
    Tuple {
        fields: Vec<PatId>,
    },
    TupleStruct {
        path: Option<BodyPath>,
        fields: Vec<PatId>,
    },
    Record {
        path: Option<BodyPath>,
        fields: Vec<RecordPatField>,
    },
    Or {
        pats: Vec<PatId>,
    },
    Slice {
        fields: Vec<PatId>,
    },
    Ref {
        pat: PatId,
    },
    Box {
        pat: PatId,
    },
    Path {
        path: Option<BodyPath>,
    },
    Wildcard,
    Unsupported,
}

/// One field inside a record pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordPatField {
    pub key: FieldKey,
    pub pat: PatId,
}
