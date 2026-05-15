//! Completion assembly for source positions.
//!
//! Examples use `$0` to mark the cursor. Member completion handles shapes like
//! `user.na$0`; qualified path completion handles both body paths such as
//! `let value = crate::api::bu$0` and imports such as `use crate::api::$0`.
//! The scanners identify the cursor site, while the resolver turns that site
//! into labels, detail text, documentation, sort keys, and replacement edits.

mod context;
mod dot;
mod path;

use rg_def_map::TargetRef;
use rg_parse::FileId;

use crate::{
    Analysis,
    model::{CompletionApplicability, CompletionItem, CompletionKind, CompletionTarget},
};

use self::{context::CompletionContext, dot::DotCompletionResolver, path::PathCompletionResolver};

/// Coordinates completion-site detection with semantic candidate rendering.
///
/// For `user.na$0`, Body IR identifies the receiver expression and typed
/// prefix; the resolver looks up the receiver type and renders member
/// candidates. For `crate::api::$0`, path scanners provide the qualifier and
/// replacement span; the resolver renders visible definitions from that module.
pub(crate) struct CompletionResolver<'a, 'db>(&'a Analysis<'db>);

impl<'a, 'db> CompletionResolver<'a, 'db> {
    pub(crate) fn new(analysis: &'a Analysis<'db>) -> Self {
        Self(analysis)
    }

    /// Collects completions for one source offset, e.g. `user.$0`,
    /// `let value = crate::$0`, or `use crate::api::$0`.
    pub(crate) fn completions_at(
        &self,
        target: TargetRef,
        file_id: FileId,
        offset: u32,
    ) -> anyhow::Result<Vec<CompletionItem>> {
        let Some(context) = CompletionContext::at(self.0, target, file_id, offset)? else {
            return Ok(Vec::new());
        };

        match context {
            CompletionContext::DotCompletionSite(site) => {
                DotCompletionResolver::new(self.0).completions(site)
            }
            CompletionContext::BodyPathCompletionSite(site) => {
                PathCompletionResolver::new(self.0).body_completions(site)
            }
            CompletionContext::UsePathCompletionSite(site) => {
                PathCompletionResolver::new(self.0).use_completions(site)
            }
        }
    }
}

struct CompletionMetadata {
    label: String,
    detail: Option<String>,
    documentation: Option<String>,
}

fn completion_sort_text(
    label: &str,
    kind: CompletionKind,
    applicability: CompletionApplicability,
    target: CompletionTarget,
) -> String {
    format!(
        "{label}|{:02}|{:02}|{target:?}",
        kind.sort_text_rank(),
        applicability.sort_text_rank(),
    )
}
