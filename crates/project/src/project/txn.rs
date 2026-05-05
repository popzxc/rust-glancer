//! Project-level read transactions.

use rg_analysis::AnalysisReadTxn;
use rg_package_store::PackageSubset;

use crate::cache::integration;

use super::state::ProjectState;

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
            analysis: integration::logical_analysis_txn(project)?,
        })
    }

    pub(crate) fn for_subset(
        project: &'a ProjectState,
        subset: &PackageSubset,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            analysis: integration::logical_analysis_txn_for_subset(project, subset)?,
        })
    }

    pub(crate) fn analysis(&self) -> &AnalysisReadTxn<'a> {
        &self.analysis
    }
}
