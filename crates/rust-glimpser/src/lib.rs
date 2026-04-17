//! Core library crate for the `rust-glimpser` workspace.
use anyhow::Context as _;
use std::path::PathBuf;

use cargo_metadata::{Metadata, MetadataCommand};

mod item_tree;

pub fn analyze(path: PathBuf) -> anyhow::Result<()> {
    if !path.exists() {
        anyhow::bail!("folder {} does not exist", path.display());
    }

    let cargo_manifest = path.join("Cargo.toml");
    if !cargo_manifest.exists() {
        anyhow::bail!("folder {} does not have Cargo.toml in it", path.display());
    }

    let metadata: Metadata = MetadataCommand::new()
        .manifest_path(cargo_manifest)
        .exec()
        .context("cargo metadata failed")?;

    let package = metadata
        .workspace_packages()
        .get(0)
        .cloned()
        .expect("No packages");
    println!("Analyzing {}", package.name);

    if package.targets.is_empty() {
        anyhow::bail!("package {} has no targets", package.name);
    }
    println!("Found {} targets", package.targets.len());

    let target_inputs = package
        .targets
        .iter()
        .map(|target| item_tree::target::TargetInput {
            name: target.name.clone(),
            kinds: target.kind.iter().map(ToString::to_string).collect(),
            root_file: target.src_path.clone().into_std_path_buf(),
        })
        .collect();
    let package_index =
        item_tree::package::PackageIndex::build(package.name.to_string(), target_inputs)
            .context("while attempting to build package index")?;
    println!("{package_index}");

    Ok(())
}
