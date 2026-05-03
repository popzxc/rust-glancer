//! Project-owned package cache integration.
//!
//! Cache artifacts bundle several phase payloads together, so this module sits above the phase
//! databases. Lower crates expose package-level hooks, but they do not know where artifacts live or
//! which residency policy selected a package for offloading.

use std::sync::Arc;

use anyhow::Context as _;
use rg_analysis::AnalysisReadTxn;
use rg_body_ir::{BodyIrDb, BodyIrPackageBundle, PackageBodies};
use rg_def_map::{DefMapDb, DefMapPackageBundle, Package as DefMapPackage, PackageSlot};
use rg_semantic_ir::{PackageIr, SemanticIrDb, SemanticIrPackageBundle};

use crate::{
    PackageCacheArtifact, PackageCacheBodyIrState, PackageCachePayload, PackageResidency, Project,
};

/// Writes and offloads every package selected by the current residency policy.
pub(crate) fn apply_residency(project: &mut Project) -> anyhow::Result<()> {
    let packages = (0..project.workspace.packages().len())
        .map(PackageSlot)
        .collect::<Vec<_>>();
    apply_residency_for_packages(project, &packages)
}

/// Re-applies cache residency to packages that were rebuilt in memory.
pub(crate) fn apply_residency_for_packages(
    project: &mut Project,
    packages: &[PackageSlot],
) -> anyhow::Result<()> {
    let mut offloaded_packages = Vec::new();

    for package in packages {
        if project.package_residency.package(*package) != Some(PackageResidency::Offloadable) {
            continue;
        }

        let artifact = artifact_from_project(project, *package)?;
        project
            .cache_store
            .write_artifact(&artifact)
            .with_context(|| {
                format!(
                    "while attempting to write package cache artifact for package {}",
                    package.0,
                )
            })?;

        // Only drop resident data after the full cross-phase package artifact is durable. If a
        // future implementation downgrades write errors to warnings, this invariant should remain.
        project.def_map.offload_package(*package).with_context(|| {
            format!("while attempting to offload def-map package {}", package.0)
        })?;
        project
            .semantic_ir
            .offload_package(*package)
            .with_context(|| {
                format!(
                    "while attempting to offload semantic IR package {}",
                    package.0
                )
            })?;
        project.body_ir.offload_package(*package).with_context(|| {
            format!("while attempting to offload body IR package {}", package.0)
        })?;
        offloaded_packages.push(package.0);
    }

    if !offloaded_packages.is_empty() {
        // Offloading drops many strong `Name` handles from phase payloads. Prune the interner
        // immediately so dead weak entries and their Arc control blocks do not pin allocator pages
        // until a later rebuild happens to compact the project.
        project.names.shrink_to_fit();

        if packages.len() == project.parse.package_count() {
            // Parse metadata survives package offloading because it is the source map for editor
            // locations. On a full residency pass, pack stable offloaded source maps into shared
            // buffers. Incremental rebuilds skip this because partially repacking an existing
            // shared buffer would keep the old shared group alive through untouched files.
            if offloaded_packages.len() == project.parse.package_count() {
                project.parse.pack_line_indexes();
            } else {
                project
                    .parse
                    .pack_line_indexes_for_packages(&offloaded_packages);
            }
        }
    }

    Ok(())
}

/// Restores offloaded packages into the project before an in-place rebuild.
///
/// Rebuild phases can consult packages outside the rebuild set as part of name/import resolution,
/// so the mutable project must be fully resident before those phases start replacing packages.
pub(crate) fn materialize_project(project: &mut Project) -> anyhow::Result<()> {
    for package_idx in 0..project.workspace.packages().len() {
        let package = PackageSlot(package_idx);
        if resident_package_arcs(project, package).is_some() {
            continue;
        }

        let artifact = read_artifact(project, package)?;
        let payload = artifact.payload;
        project
            .def_map
            .replace_package(package, payload.def_map.into_package())
            .with_context(|| {
                format!(
                    "while attempting to restore def-map package {} from cache",
                    package.0,
                )
            })?;
        project
            .semantic_ir
            .replace_package(package, payload.semantic_ir.into_package())
            .with_context(|| {
                format!(
                    "while attempting to restore semantic IR package {} from cache",
                    package.0,
                )
            })?;
        project
            .body_ir
            .replace_package(
                package,
                body_ir_package_from_payload(package, payload.body_ir)?,
            )
            .with_context(|| {
                format!(
                    "while attempting to restore body IR package {} from cache",
                    package.0,
                )
            })?;
    }

    Ok(())
}

/// Builds a query transaction with every offloaded package materialized back into owned memory.
pub(crate) fn materialized_analysis_txn(project: &Project) -> anyhow::Result<AnalysisReadTxn<'_>> {
    let mut def_map_packages = Vec::with_capacity(project.workspace.packages().len());
    let mut semantic_ir_packages = Vec::with_capacity(project.workspace.packages().len());
    let mut body_ir_packages = Vec::with_capacity(project.workspace.packages().len());

    for package_idx in 0..project.workspace.packages().len() {
        let package = PackageSlot(package_idx);

        match resident_package_arcs(project, package) {
            Some((def_map, semantic_ir, body_ir)) => {
                def_map_packages.push(def_map);
                semantic_ir_packages.push(semantic_ir);
                body_ir_packages.push(body_ir);
            }
            None => {
                let artifact = read_artifact(project, package)?;
                let payload = artifact.payload;
                def_map_packages.push(Arc::new(payload.def_map.into_package()));
                semantic_ir_packages.push(Arc::new(payload.semantic_ir.into_package()));
                body_ir_packages.push(Arc::new(body_ir_package_from_payload(
                    package,
                    payload.body_ir,
                )?));
            }
        }
    }

    Ok(AnalysisReadTxn::from_phase_txns(
        DefMapDb::read_txn_from_package_arcs(def_map_packages),
        SemanticIrDb::read_txn_from_package_arcs(semantic_ir_packages),
        BodyIrDb::read_txn_from_package_arcs(body_ir_packages),
    ))
}

fn artifact_from_project(
    project: &Project,
    package: PackageSlot,
) -> anyhow::Result<PackageCacheArtifact> {
    let header = project
        .cached_workspace
        .artifact_header(package)
        .with_context(|| {
            format!(
                "while attempting to build package cache header for package {}",
                package.0,
            )
        })?;
    let def_map = project.def_map.package(package).with_context(|| {
        format!(
            "while attempting to fetch resident def-map package {}",
            package.0,
        )
    })?;
    let semantic_ir = project.semantic_ir.package(package).with_context(|| {
        format!(
            "while attempting to fetch resident semantic IR package {}",
            package.0,
        )
    })?;
    let body_ir = project.body_ir.package(package).with_context(|| {
        format!(
            "while attempting to fetch resident body IR package {}",
            package.0,
        )
    })?;

    Ok(PackageCacheArtifact::new(
        header,
        PackageCachePayload::new(
            DefMapPackageBundle::new(def_map.clone()),
            SemanticIrPackageBundle::new(semantic_ir.clone()),
            PackageCacheBodyIrState::Built(Box::new(BodyIrPackageBundle::new(body_ir.clone()))),
        ),
    ))
}

fn resident_package_arcs(
    project: &Project,
    package: PackageSlot,
) -> Option<(Arc<DefMapPackage>, Arc<PackageIr>, Arc<PackageBodies>)> {
    Some((
        project.def_map.package_arc(package)?,
        project.semantic_ir.package_arc(package)?,
        project.body_ir.package_arc(package)?,
    ))
}

fn read_artifact(project: &Project, package: PackageSlot) -> anyhow::Result<PackageCacheArtifact> {
    let header = project
        .cached_workspace
        .artifact_header(package)
        .with_context(|| {
            format!(
                "while attempting to build package cache header for package {}",
                package.0,
            )
        })?;

    match project.cache_store.read_artifact(&header) {
        Ok(Some(artifact)) => Ok(artifact),
        Ok(None) => anyhow::bail!("missing package cache artifact for package {}", package.0),
        Err(error) => {
            let _ = project.cache_store.invalidate_workspace_cache();
            Err(error).with_context(|| {
                format!(
                    "while attempting to materialize package cache artifact for package {}",
                    package.0,
                )
            })
        }
    }
}

fn body_ir_package_from_payload(
    package: PackageSlot,
    body_ir: PackageCacheBodyIrState,
) -> anyhow::Result<PackageBodies> {
    match body_ir {
        PackageCacheBodyIrState::Built(bundle) => Ok(bundle.into_package()),
        PackageCacheBodyIrState::SkippedByPolicy => {
            anyhow::bail!(
                "package cache artifact for package {} skipped body IR payload",
                package.0,
            )
        }
    }
}
