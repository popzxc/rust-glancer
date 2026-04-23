mod fixture;
mod metadata_ext;
mod paths;
mod query;

pub(crate) use self::{
    fixture::{CrateFixture, fixture_crate},
    metadata_ext::TestTargetExt,
    paths::test_file,
};
