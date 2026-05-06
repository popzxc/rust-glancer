//! Request-scoped analysis transactions.

use rg_body_ir::BodyIrReadTxn;
use rg_def_map::DefMapReadTxn;
use rg_semantic_ir::SemanticIrReadTxn;

/// Read transaction shared by all analysis queries in one request.
///
/// Phase transactions own request-local handles to resident packages and lazily loaded offloaded
/// packages, so all query helpers see one consistent logical project view.
#[derive(Debug, Clone)]
pub struct AnalysisReadTxn<'a> {
    def_map: DefMapReadTxn<'a>,
    semantic_ir: SemanticIrReadTxn<'a>,
    body_ir: BodyIrReadTxn<'a>,
}

impl<'a> AnalysisReadTxn<'a> {
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
