mod fixture;
mod paths;
mod query;
pub(crate) mod snapshot;

pub(crate) use self::{
    fixture::{CrateFixture, FixtureMarkers, fixture_crate, fixture_crate_with_markers},
    paths::test_file,
};
