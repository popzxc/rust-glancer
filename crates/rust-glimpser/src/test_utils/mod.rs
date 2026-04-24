mod fixture;
mod paths;
mod query;
pub(crate) mod snapshot;

pub(crate) use self::{
    fixture::{CrateFixture, fixture_crate},
    paths::test_file,
};
