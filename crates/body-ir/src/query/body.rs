//! Body IR read surface shared by resident DBs and read transactions.

use crate::{BodyData, BodyIrDb, BodyIrReadTxn, BodyRef};

/// Minimal Body IR query surface shared by resident DBs and read transactions.
pub(crate) trait BodyIrQuery {
    fn body_data(&self, body_ref: BodyRef) -> Option<&BodyData>;
}

impl BodyIrQuery for BodyIrDb {
    fn body_data(&self, body_ref: BodyRef) -> Option<&BodyData> {
        BodyIrDb::body_data(self, body_ref)
    }
}

impl BodyIrQuery for BodyIrReadTxn<'_> {
    fn body_data(&self, body_ref: BodyRef) -> Option<&BodyData> {
        BodyIrReadTxn::body_data(self, body_ref)
    }
}
