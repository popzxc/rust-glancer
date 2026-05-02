use rg_memsize::{MemoryRecorder, MemorySize};

use crate::{
    AnalysisChangeSummary, AnalysisHost, ChangedFile, FileContext, PackageCacheArtifact,
    PackageCacheBodyIrState, PackageCacheDependency, PackageCacheHeader, PackageCacheIdentity,
    PackageCachePayload, PackageCachePlan, PackageCacheSchemaVersion, PackageCacheTarget,
    PackageResidency, PackageResidencyPlan, PackageResidencyPolicy, Project, ProjectBuildOptions,
    SavedFileChange,
};

macro_rules! record_fields {
    ($recorder:expr, $owner:expr, $($field:ident),+ $(,)?) => {
        $(
            $recorder.scope(stringify!($field), |recorder| {
                $owner.$field.record_memory_children(recorder);
            });
        )+
    };
}

impl MemorySize for Project {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            workspace,
            build_options,
            package_residency,
            names,
            parse,
            def_map,
            semantic_ir,
            body_ir,
        );
    }
}

impl MemorySize for ProjectBuildOptions {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, body_ir_policy, package_residency_policy);
    }
}

impl MemorySize for PackageCachePlan {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("packages", |recorder| {
            self.packages.record_memory_children(recorder);
        });
    }
}

impl MemorySize for PackageCacheIdentity {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            package,
            package_id,
            name,
            source,
            edition,
            manifest_path,
            targets,
            dependencies,
        );
    }
}

impl MemorySize for PackageCacheTarget {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, name, kind, src_path);
    }
}

impl MemorySize for PackageCacheDependency {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder, self, package_id, name, is_normal, is_build, is_dev,
        );
    }
}

impl MemorySize for PackageCacheHeader {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, schema_version, package);
    }
}

impl MemorySize for PackageCacheSchemaVersion {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        self.0.record_memory_children(recorder);
    }
}

impl MemorySize for PackageCacheBodyIrState {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Built(bundle) => recorder.scope("built", |recorder| {
                bundle.record_memory_children(recorder);
            }),
            Self::SkippedByPolicy => {}
        }
    }
}

impl MemorySize for PackageCacheArtifact {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, header, payload);
    }
}

impl MemorySize for PackageCachePayload {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, def_map, semantic_ir, body_ir);
    }
}

impl MemorySize for PackageResidencyPolicy {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for PackageResidencyPlan {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, policy, packages);
    }
}

impl MemorySize for PackageResidency {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for AnalysisHost {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("project", |recorder| {
            self.project.record_memory_children(recorder);
        });
    }
}

impl MemorySize for SavedFileChange {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("path", |recorder| {
            self.path.record_memory_children(recorder);
        });
    }
}

impl MemorySize for AnalysisChangeSummary {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            changed_files,
            affected_packages,
            changed_targets,
        );
    }
}

impl MemorySize for ChangedFile {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, package, file);
    }
}

impl MemorySize for FileContext {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, package, file, targets);
    }
}
