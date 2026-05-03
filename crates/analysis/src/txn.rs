//! Request-scoped analysis transactions.

use rg_body_ir::{BodyIrDb, BodyIrReadTxn};
use rg_def_map::{DefMapDb, DefMapReadTxn};
use rg_semantic_ir::{SemanticIrDb, SemanticIrReadTxn};

/// Read transaction shared by all analysis queries in one request.
///
/// The resident implementation is deliberately small. Once packages can be offloaded, this object
/// becomes the request-scoped owner of any packages loaded back from disk.
#[derive(Debug, Clone)]
pub struct AnalysisReadTxn<'a> {
    def_map: DefMapReadTxn<'a>,
    semantic_ir: SemanticIrReadTxn<'a>,
    body_ir: BodyIrReadTxn<'a>,
}

impl<'a> AnalysisReadTxn<'a> {
    pub fn new(
        def_map: &'a DefMapDb,
        semantic_ir: &'a SemanticIrDb,
        body_ir: &'a BodyIrDb,
    ) -> Self {
        Self::from_phase_txns(
            def_map.read_txn(),
            semantic_ir.read_txn(),
            body_ir.read_txn(),
        )
    }

    pub fn from_phase_txns(
        def_map: DefMapReadTxn<'a>,
        semantic_ir: SemanticIrReadTxn<'a>,
        body_ir: BodyIrReadTxn<'a>,
    ) -> Self {
        Self {
            def_map,
            semantic_ir,
            body_ir,
        }
    }

    pub(crate) fn def_map(&self) -> &DefMapReadTxn<'a> {
        &self.def_map
    }

    pub(crate) fn semantic_ir(&self) -> &SemanticIrReadTxn<'a> {
        &self.semantic_ir
    }

    pub(crate) fn body_ir(&self) -> &BodyIrReadTxn<'a> {
        &self.body_ir
    }
}
