//! Project-owned package cache integration.
//!
//! Cache artifacts bundle several phase payloads together, so this module sits above the phase
//! databases. Lower crates expose package-level hooks, but they do not know where artifacts live or
//! which residency policy selected a package for offloading.

use std::{fmt, sync::Arc};

use anyhow::Context as _;
use rg_analysis::AnalysisReadTxn;
use rg_body_ir::{BodyIrPackageBundle, BodyIrReadTxn, PackageBodies};
use rg_def_map::{DefMapPackageBundle, DefMapReadTxn, Package as DefMapPackage, PackageSlot};
use rg_semantic_ir::{PackageIr, SemanticIrPackageBundle, SemanticIrReadTxn};

use crate::{
    PackageResidency,
    cache::{PackageCacheArtifact, PackageCacheBodyIrState, PackageCachePayload},
    project::{demand::PackageDemand, state::ProjectState},
};

/// Writes and offloads every package selected by the current residency policy.
pub(crate) fn apply_residency(project: &mut ProjectState) -> anyhow::Result<()> {
    let packages = (0..project.workspace.packages().len())
        .map(PackageSlot)
        .collect::<Vec<_>>();
    write_and_offload_packages(project, &packages)
}

/// Restores the current residency policy after a package rebuild.
///
/// Rebuilds materialize the package being replaced plus the dependencies it can inspect. Only
/// rebuilt packages need fresh artifacts; unchanged packages can be dropped back to their
/// already-written cache entries.
pub(crate) fn restore_residency_after_rebuild(
    project: &mut ProjectState,
    rebuilt_packages: &[PackageSlot],
) -> anyhow::Result<()> {
    let package_count = project.workspace.packages().len();
    let mut rebuilt = vec![false; package_count];
    for package in rebuilt_packages {
        if package.0 < package_count {
            rebuilt[package.0] = true;
        }
    }

    for package_idx in 0..package_count {
        let package = PackageSlot(package_idx);
        if !rebuilt[package_idx]
            || project.package_residency.package(package) != Some(PackageResidency::Offloadable)
        {
            continue;
        }

        write_package_artifact(project, package)?;
    }

    let mut offloaded_packages = Vec::new();

    for package_idx in 0..package_count {
        let package = PackageSlot(package_idx);
        if project.package_residency.package(package) != Some(PackageResidency::Offloadable) {
            continue;
        }

        offload_package(project, package)?;
        offloaded_packages.push(package_idx);
    }

    finish_offloading(project, &offloaded_packages, true);
    project
        .cache_store
        .cleanup_stale_generations()
        .context("while attempting to clean stale package cache generations")?;

    Ok(())
}

fn write_and_offload_packages(
    project: &mut ProjectState,
    packages: &[PackageSlot],
) -> anyhow::Result<()> {
    let mut offloaded_packages = Vec::new();

    for package in packages {
        if project.package_residency.package(*package) != Some(PackageResidency::Offloadable) {
            continue;
        }

        write_package_artifact(project, *package)?;
        offload_package(project, *package)?;
        offloaded_packages.push(package.0);
    }

    finish_offloading(
        project,
        &offloaded_packages,
        packages.len() == project.parse.package_count(),
    );
    project
        .cache_store
        .cleanup_stale_generations()
        .context("while attempting to clean stale package cache generations")?;

    Ok(())
}

fn finish_offloading(
    project: &mut ProjectState,
    offloaded_packages: &[usize],
    is_full_residency_pass: bool,
) {
    if !offloaded_packages.is_empty() {
        // Offloading drops many strong `Name` handles from phase payloads. Prune the interner
        // immediately so dead weak entries and their Arc control blocks do not pin allocator pages
        // until a later rebuild happens to compact the project.
        project.names.shrink_to_fit();

        if is_full_residency_pass {
            // Parse metadata survives package offloading because it is the source map for editor
            // locations. Once the global residency plan is restored, pack stable offloaded source
            // maps into shared buffers so they do not keep many small allocations around.
            if offloaded_packages.len() == project.parse.package_count() {
                project.parse.pack_line_indexes();
            } else {
                project
                    .parse
                    .pack_line_indexes_for_packages(&offloaded_packages);
            }
        }
    }
}

fn write_package_artifact(project: &ProjectState, package: PackageSlot) -> anyhow::Result<()> {
    let artifact = artifact_from_project(project, package)?;
    project
        .cache_store
        .write_artifact(&artifact)
        .with_context(|| {
            format!(
                "while attempting to write package cache artifact for package {}",
                package.0,
            )
        })
}

fn write_residency_artifacts(project: &ProjectState) -> anyhow::Result<()> {
    for package_idx in 0..project.workspace.packages().len() {
        let package = PackageSlot(package_idx);
        if project.package_residency.package(package) != Some(PackageResidency::Offloadable) {
            continue;
        }

        write_package_artifact(project, package)?;
    }

    Ok(())
}

fn offload_package(project: &mut ProjectState, package: PackageSlot) -> anyhow::Result<()> {
    // Only drop resident data after the full cross-phase package artifact is durable. If a future
    // implementation downgrades write errors to warnings, this invariant should remain.
    project
        .def_map
        .offload_package(package)
        .with_context(|| format!("while attempting to offload def-map package {}", package.0))?;
    project
        .semantic_ir
        .offload_package(package)
        .with_context(|| {
            format!(
                "while attempting to offload semantic IR package {}",
                package.0
            )
        })?;
    project
        .body_ir
        .offload_package(package)
        .with_context(|| format!("while attempting to offload body IR package {}", package.0))?;

    Ok(())
}

/// Restores the packages one in-place rebuild can inspect.
///
/// Rebuild phases may resolve through dependencies that are not themselves being replaced. The
/// caller supplies that dependency-expanded demand so unrelated offloaded packages can stay cold.
pub(crate) fn materialize_packages(
    project: &mut ProjectState,
    demand: &PackageDemand,
) -> anyhow::Result<()> {
    match try_materialize_packages(project, demand) {
        Ok(()) => Ok(()),
        Err(error) if is_cache_artifact_unavailable(&error) => {
            recover_resident_project_from_source(project).with_context(|| {
                format!(
                    "while attempting to recover analysis project after package cache became unavailable: {error}",
                )
            })
        }
        Err(error) => Err(error),
    }
}

fn try_materialize_packages(
    project: &mut ProjectState,
    demand: &PackageDemand,
) -> anyhow::Result<()> {
    for package_idx in 0..demand.package_count() {
        let package = PackageSlot(package_idx);
        if !demand.contains(package) {
            continue;
        }
        if resident_package_arcs(project, package).is_some() {
            continue;
        }

        let artifact = read_artifact(project, package)?;
        restore_package_from_payload(project, package, artifact.payload)?;
    }

    Ok(())
}

fn restore_package_from_payload(
    project: &mut ProjectState,
    package: PackageSlot,
    payload: PackageCachePayload,
) -> anyhow::Result<()> {
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

    Ok(())
}

fn recover_resident_project_from_source(project: &mut ProjectState) -> anyhow::Result<()> {
    project
        .cache_store
        .invalidate_workspace_cache()
        .context("while attempting to invalidate package cache namespace")?;
    project
        .rebuild_resident_from_source()
        .context("while attempting to rebuild resident analysis project from source")?;
    write_residency_artifacts(project)
        .context("while attempting to rewrite package cache artifacts after recovery")?;

    Ok(())
}

/// Builds a query transaction with every package materialized back into owned memory.
pub(crate) fn materialized_analysis_txn(
    project: &ProjectState,
) -> anyhow::Result<AnalysisReadTxn<'_>> {
    let demand = PackageDemand::all(&project.workspace);
    materialized_analysis_txn_for_demand(project, &demand)
}

/// Builds a query transaction with only demanded packages materialized.
pub(crate) fn materialized_analysis_txn_for_demand<'a>(
    project: &'a ProjectState,
    demand: &PackageDemand,
) -> anyhow::Result<AnalysisReadTxn<'a>> {
    match try_materialized_analysis_txn(project, demand) {
        Ok(txn) => Ok(txn),
        Err(error) if is_cache_artifact_unavailable(&error) => {
            rebuilt_analysis_txn_from_source(project, demand).with_context(|| {
                format!(
                    "while attempting to recover analysis transaction after package cache became unavailable: {error}",
                )
            })
        }
        Err(error) => Err(error),
    }
}

fn try_materialized_analysis_txn<'a>(
    project: &'a ProjectState,
    demand: &PackageDemand,
) -> anyhow::Result<AnalysisReadTxn<'a>> {
    let mut def_map_packages = vec![None; demand.package_count()];
    let mut semantic_ir_packages = vec![None; demand.package_count()];
    let mut body_ir_packages = vec![None; demand.package_count()];

    for package_idx in 0..demand.package_count() {
        let package = PackageSlot(package_idx);
        if !demand.contains(package) {
            continue;
        }

        match resident_package_arcs(project, package) {
            Some((def_map, semantic_ir, body_ir)) => {
                def_map_packages[package_idx] = Some(def_map);
                semantic_ir_packages[package_idx] = Some(semantic_ir);
                body_ir_packages[package_idx] = Some(body_ir);
            }
            None => {
                let artifact = read_artifact(project, package)?;
                let payload = artifact.payload;
                def_map_packages[package_idx] = Some(Arc::new(payload.def_map.into_package()));
                semantic_ir_packages[package_idx] =
                    Some(Arc::new(payload.semantic_ir.into_package()));
                body_ir_packages[package_idx] = Some(Arc::new(body_ir_package_from_payload(
                    package,
                    payload.body_ir,
                )?));
            }
        }
    }

    Ok(AnalysisReadTxn::from_phase_txns(
        DefMapReadTxn::from_sparse_package_arcs(def_map_packages),
        SemanticIrReadTxn::from_sparse_package_arcs(semantic_ir_packages),
        BodyIrReadTxn::from_sparse_package_arcs(body_ir_packages),
    ))
}

fn rebuilt_analysis_txn_from_source<'a>(
    project: &'a ProjectState,
    demand: &PackageDemand,
) -> anyhow::Result<AnalysisReadTxn<'a>> {
    project
        .cache_store
        .invalidate_workspace_cache()
        .context("while attempting to invalidate package cache namespace")?;

    let mut rebuilt =
        ProjectState::build_resident_with_options(project.workspace.clone(), project.build_options)
            .context("while attempting to rebuild analysis transaction from source")?;
    rebuilt.cache_store = project.cache_store.clone();
    write_residency_artifacts(&rebuilt)
        .context("while attempting to rewrite package cache artifacts after recovery")?;

    let mut def_map_packages = vec![None; demand.package_count()];
    let mut semantic_ir_packages = vec![None; demand.package_count()];
    let mut body_ir_packages = vec![None; demand.package_count()];

    for package_idx in 0..demand.package_count() {
        let package = PackageSlot(package_idx);
        if !demand.contains(package) {
            continue;
        }

        let (def_map, semantic_ir, body_ir) = resident_package_arcs(&rebuilt, package)
            .with_context(|| {
                format!("while attempting to collect rebuilt package {}", package.0)
            })?;
        def_map_packages[package_idx] = Some(def_map);
        semantic_ir_packages[package_idx] = Some(semantic_ir);
        body_ir_packages[package_idx] = Some(body_ir);
    }

    Ok(AnalysisReadTxn::from_phase_txns(
        DefMapReadTxn::from_sparse_package_arcs(def_map_packages),
        SemanticIrReadTxn::from_sparse_package_arcs(semantic_ir_packages),
        BodyIrReadTxn::from_sparse_package_arcs(body_ir_packages),
    ))
}

fn artifact_from_project(
    project: &ProjectState,
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
    project: &ProjectState,
    package: PackageSlot,
) -> Option<(Arc<DefMapPackage>, Arc<PackageIr>, Arc<PackageBodies>)> {
    Some((
        project.def_map.package_arc(package)?,
        project.semantic_ir.package_arc(package)?,
        project.body_ir.package_arc(package)?,
    ))
}

fn read_artifact(
    project: &ProjectState,
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

    match project.cache_store.read_artifact(&header) {
        Ok(Some(artifact)) => Ok(artifact),
        Ok(None) => Err(PackageCacheArtifactUnavailable::missing(package).into()),
        Err(error) => Err(PackageCacheArtifactUnavailable::invalid(package, error).into()),
    }
}

fn is_cache_artifact_unavailable(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.is::<PackageCacheArtifactUnavailable>())
}

#[derive(Debug)]
struct PackageCacheArtifactUnavailable {
    package: PackageSlot,
    reason: PackageCacheArtifactUnavailableReason,
}

impl PackageCacheArtifactUnavailable {
    fn missing(package: PackageSlot) -> Self {
        Self {
            package,
            reason: PackageCacheArtifactUnavailableReason::Missing,
        }
    }

    fn invalid(package: PackageSlot, error: anyhow::Error) -> Self {
        Self {
            package,
            reason: PackageCacheArtifactUnavailableReason::Invalid(error),
        }
    }
}

impl fmt::Display for PackageCacheArtifactUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.reason {
            PackageCacheArtifactUnavailableReason::Missing => {
                write!(
                    f,
                    "missing package cache artifact for package {}",
                    self.package.0,
                )
            }
            PackageCacheArtifactUnavailableReason::Invalid(_) => {
                write!(
                    f,
                    "invalid package cache artifact for package {}",
                    self.package.0,
                )
            }
        }
    }
}

impl std::error::Error for PackageCacheArtifactUnavailable {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.reason {
            PackageCacheArtifactUnavailableReason::Missing => None,
            PackageCacheArtifactUnavailableReason::Invalid(error) => Some(error.as_ref()),
        }
    }
}

#[derive(Debug)]
enum PackageCacheArtifactUnavailableReason {
    Missing,
    Invalid(anyhow::Error),
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
