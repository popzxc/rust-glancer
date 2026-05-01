use rg_memsize::{MemoryRecorder, MemorySize};

use crate::{
    AnalysisChangeSummary, AnalysisHost, ChangedFile, FileContext, PackageResidency,
    PackageResidencyPlan, PackageResidencyPolicy, Project, ProjectBuildOptions, SavedFileChange,
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
