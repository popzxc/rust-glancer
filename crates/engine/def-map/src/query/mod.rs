//! Queries over frozen def-map data.

mod cursor;
mod path_completion;
pub(crate) mod path_resolution;

pub use self::{
    cursor::DefMapCursorCandidate,
    path_completion::{DefMapPathCompletionSite, ScopeNamespace, VisibleScopeDef},
    path_resolution::ResolvePathResult,
};
