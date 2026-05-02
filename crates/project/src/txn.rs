//! Project-level read transactions.

use rg_analysis::AnalysisReadTxn;
use rg_body_ir::BodyIrDb;
use rg_def_map::DefMapDb;
use rg_semantic_ir::SemanticIrDb;

/// Read transaction for project-level query APIs.
///
/// The transaction is request-scoped: query callers create it once, build an `Analysis` view from
/// it, and reuse that view for the duration of the request.
#[derive(Debug, Clone)]
pub struct ProjectReadTxn<'a> {
    analysis: AnalysisReadTxn<'a>,
}

impl<'a> ProjectReadTxn<'a> {
    pub(crate) fn new(
        def_map: &'a DefMapDb,
        semantic_ir: &'a SemanticIrDb,
        body_ir: &'a BodyIrDb,
    ) -> Self {
        Self {
            analysis: AnalysisReadTxn::new(def_map, semantic_ir, body_ir),
        }
    }

    pub(crate) fn analysis(&self) -> &AnalysisReadTxn<'a> {
        &self.analysis
    }
}
