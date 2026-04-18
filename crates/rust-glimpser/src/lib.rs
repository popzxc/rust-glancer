//! Core library crate for the `rust-glimpser` workspace.
use anyhow::Context as _;
use std::path::PathBuf;

pub(crate) mod parse;

#[cfg(test)]
mod test_utils;

/// Runs project analysis for the Cargo manifest at `path` and prints extracted item trees.
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

    let project_analysis = parse::ProjectAnalysis::build(metadata)
        .context("while attempting to build project analysis")?;
    println!("{project_analysis}");

    Ok(())
}
