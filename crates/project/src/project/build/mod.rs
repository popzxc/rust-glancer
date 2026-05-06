//! Fresh project construction.

mod phases;

use anyhow::Context as _;

use rg_body_ir::BodyIrBuildPolicy;
use rg_workspace::WorkspaceMetadata;

use crate::{
    BuildProcessMemory, BuildProfile, PackageResidencyPlan, PackageResidencyPolicy,
    cache::{CachedWorkspace, PackageCacheStore, integration},
    profile::{BuildProfiler, ProcessMemorySampler},
};

use super::{Project, state::ProjectState};

/// Result of building a project, optionally including build-time profiling data.
pub struct ProjectBuild {
    project: Project,
    profile: Option<BuildProfile>,
}

impl ProjectBuild {
    pub fn into_project(self) -> Project {
        self.project
    }

    pub fn profile(&self) -> Option<&BuildProfile> {
        self.profile.as_ref()
    }

    pub fn into_parts(self) -> (Project, Option<BuildProfile>) {
        (self.project, self.profile)
    }
}

/// Fluent construction API for a fresh analysis project.
pub struct ProjectBuilder {
    workspace: WorkspaceMetadata,
    body_ir_policy: BodyIrBuildPolicy,
    package_residency_policy: PackageResidencyPolicy,
    measure_retained_memory: bool,
    process_memory_sampler: Option<ProcessMemorySampler>,
}

impl ProjectBuilder {
    pub(crate) fn new(workspace: WorkspaceMetadata) -> Self {
        Self {
            workspace,
            body_ir_policy: BodyIrBuildPolicy::default(),
            package_residency_policy: PackageResidencyPolicy::default(),
            measure_retained_memory: false,
            process_memory_sampler: None,
        }
    }

    pub fn body_ir_policy(mut self, policy: BodyIrBuildPolicy) -> Self {
        self.body_ir_policy = policy;
        self
    }

    pub fn package_residency_policy(mut self, policy: PackageResidencyPolicy) -> Self {
        self.package_residency_policy = policy;
        self
    }

    pub fn measure_retained_memory(mut self, enabled: bool) -> Self {
        self.measure_retained_memory = enabled;
        self
    }

    pub fn process_memory_sampler(
        mut self,
        sampler: impl FnMut() -> Option<BuildProcessMemory> + 'static,
    ) -> Self {
        self.process_memory_sampler = Some(Box::new(sampler));
        self
    }

    pub fn build(self) -> anyhow::Result<ProjectBuild> {
        let profile_requested =
            self.measure_retained_memory || self.process_memory_sampler.is_some();
        let mut profiler =
            BuildProfiler::new(self.measure_retained_memory, self.process_memory_sampler);
        let mut state = build_resident_state(
            self.workspace,
            self.body_ir_policy,
            self.package_residency_policy,
            &mut profiler,
        )
        .context("while attempting to build resident analysis project")?;
        integration::apply_residency(&mut state)
            .context("while attempting to apply package cache residency")?;

        let process_memory = profiler.sample_process_memory();
        let project_bytes = profiler.measure(&state);
        profiler.record(
            "after project",
            project_bytes,
            project_bytes,
            process_memory,
        );
        let profile = profile_requested.then(|| profiler.finish());

        Ok(ProjectBuild {
            project: Project { state },
            profile,
        })
    }
}

pub(crate) fn build_resident_state(
    workspace: WorkspaceMetadata,
    body_ir_policy: BodyIrBuildPolicy,
    package_residency_policy: PackageResidencyPolicy,
    profiler: &mut BuildProfiler,
) -> anyhow::Result<ProjectState> {
    let phases = phases::build(&workspace, body_ir_policy, profiler)?;
    let package_residency = PackageResidencyPlan::build(&workspace, package_residency_policy);
    let cached_workspace = CachedWorkspace::build(&workspace, &phases.parse);
    let cache_store = PackageCacheStore::for_workspace(&workspace, &cached_workspace);

    Ok(ProjectState {
        workspace,
        cached_workspace,
        cache_store,
        body_ir_policy,
        package_residency_policy,
        package_residency,
        names: phases.names,
        parse: phases.parse,
        def_map: phases.def_map,
        semantic_ir: phases.semantic_ir,
        body_ir: phases.body_ir,
    })
}
