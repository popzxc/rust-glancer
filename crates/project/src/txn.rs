//! Project-level read transactions.

use rg_analysis::AnalysisReadTxn;

use crate::{Project, cache::integration};

/// Read transaction for project-level query APIs.
///
/// The transaction is request-scoped: query callers create it once, build an `Analysis` view from
/// it, and reuse that view for the duration of the request.
#[derive(Debug, Clone)]
pub struct ProjectReadTxn<'a> {
    analysis: AnalysisReadTxn<'a>,
}

impl<'a> ProjectReadTxn<'a> {
    pub(crate) fn new(project: &'a Project) -> anyhow::Result<Self> {
        Ok(Self {
            analysis: integration::materialized_analysis_txn(project)?,
        })
    }

    pub(crate) fn analysis(&self) -> &AnalysisReadTxn<'a> {
        &self.analysis
    }
}
