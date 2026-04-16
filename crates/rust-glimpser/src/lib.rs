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

    let target = package.targets.get(0).cloned().expect("No targets");
    println!(
        "Target name {} | type {:?} | path {}",
        target.name, target.crate_types, target.src_path
    );

    let item_tree = item_tree::build_crate_item_tree(target.src_path.into_std_path_buf())
        .context("while attempting to build crate item tree")?;
    item_tree::print_tree(&item_tree);

    Ok(())
}
