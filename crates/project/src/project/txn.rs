//! Project-level read transactions.

use rg_analysis::AnalysisReadTxn;

use crate::cache::integration;

use super::{demand::PackageDemand, state::ProjectState};

/// Read transaction for project-level query APIs.
///
/// The transaction is request-scoped: query callers create it once, build an `Analysis` view from
/// it, and reuse that view for the duration of the request.
#[derive(Debug, Clone)]
pub(crate) struct ProjectReadTxn<'a> {
    analysis: AnalysisReadTxn<'a>,
}

impl<'a> ProjectReadTxn<'a> {
    pub(crate) fn new(project: &'a ProjectState) -> anyhow::Result<Self> {
        Ok(Self {
            analysis: integration::materialized_analysis_txn(project)?,
        })
    }

    pub(crate) fn for_demand(
        project: &'a ProjectState,
        demand: &PackageDemand,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            analysis: integration::materialized_analysis_txn_for_demand(project, demand)?,
        })
    }

    pub(crate) fn analysis(&self) -> &AnalysisReadTxn<'a> {
        &self.analysis
    }
}
