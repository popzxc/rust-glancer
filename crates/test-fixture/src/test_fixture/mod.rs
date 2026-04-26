mod fixture;
mod paths;

pub use self::{
    fixture::{
        CrateFixture, FixtureMarkers, FixtureSpec, fixture_crate, fixture_crate_with_markers,
    },
    paths::test_file,
};
