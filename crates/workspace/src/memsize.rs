use rg_memsize::{MemoryRecorder, MemorySize};

use crate::{
    Package, PackageDependency, PackageId, PackageOrigin, PackageSlot, PackageSource, RustEdition,
    SysrootCrate, SysrootSources, Target, TargetKind, WorkspaceMetadata,
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

impl MemorySize for WorkspaceMetadata {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, workspace_root, packages, package_by_id);
    }
}

impl MemorySize for PackageId {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        self.0.record_memory_children(recorder);
    }
}

impl MemorySize for PackageSlot {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for PackageOrigin {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Workspace | Self::Dependency => {}
            Self::Sysroot(krate) => krate.record_memory_children(recorder),
        }
    }
}

impl MemorySize for PackageSource {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for SysrootCrate {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for SysrootSources {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, sysroot_root, library_root);
    }
}

impl MemorySize for RustEdition {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl MemorySize for Package {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(
            recorder,
            self,
            id,
            name,
            edition,
            origin,
            source,
            is_workspace_member,
            manifest_path,
            targets,
            dependencies,
        );
    }
}

impl MemorySize for Target {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, name, kind, src_path);
    }
}

impl MemorySize for PackageDependency {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        record_fields!(recorder, self, package, name, is_normal, is_build, is_dev,);
    }
}

impl MemorySize for TargetKind {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Self::Lib
            | Self::Bin
            | Self::Example
            | Self::Test
            | Self::Bench
            | Self::CustomBuild => {}
            Self::Other(name) => name.record_memory_children(recorder),
        }
    }
}
