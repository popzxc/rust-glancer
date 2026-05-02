//! Narrow query surfaces consumed by Body IR algorithms.
//!
//! Body resolution runs in two contexts: build-time mutation over resident databases, and
//! query-time reads through request transactions. These traits describe the small read interface
//! shared by both contexts so the algorithms do not depend on storage details.

mod body;
mod def_map;
mod semantic;

pub(crate) use self::{body::BodyIrQuery, def_map::DefMapQuery, semantic::SemanticIrQuery};
