//! Core library crate for the `rust-glimpser` workspace.
use anyhow::Context as _;
use std::path::PathBuf;

pub(crate) mod analysis;
pub(crate) mod body_ir;
pub(crate) mod def_map;
pub(crate) mod item_tree;
pub(crate) mod parse;
mod project;
pub(crate) mod semantic_ir;
mod workspace_metadata;

#[cfg(test)]
mod test_utils;

pub use self::project::Project;
pub use self::workspace_metadata::WorkspaceMetadata;

/// Runs project analysis for the Cargo manifest at `path` and prints the current analysis report.
pub fn analyze(path: PathBuf) -> anyhow::Result<()> {
    if !path.exists() {
        anyhow::bail!("folder {} does not exist", path.display());
    }

    let cargo_manifest = path.join("Cargo.toml");
    if !cargo_manifest.exists() {
        anyhow::bail!("folder {} does not have Cargo.toml in it", path.display());
    }

    let metadata: cargo_metadata::Metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(cargo_manifest)
        .exec()
        .context("cargo metadata failed")?;

    let workspace = WorkspaceMetadata::from_cargo(metadata);
    let project = Project::build(workspace).context("while attempting to build project")?;
    println!("{project}");

    Ok(())
}
