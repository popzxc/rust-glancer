use std::time::{Duration, Instant};

use rg_memsize::MemorySize;

/// Build-time memory and timing report for the project pipeline.
///
/// This is intentionally a facts-only API: callers can inspect coarse checkpoints without
/// receiving references to transient phase databases such as ItemTree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildProfile {
    checkpoints: Vec<BuildCheckpoint>,
}

impl BuildProfile {
    pub(crate) fn new(checkpoints: Vec<BuildCheckpoint>) -> Self {
        Self { checkpoints }
    }

    pub fn checkpoints(&self) -> &[BuildCheckpoint] {
        &self.checkpoints
    }
}

/// One profiling sample collected while the project pipeline is building.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildCheckpoint {
    pub label: &'static str,
    pub elapsed: Duration,
    /// Retained size of the object sampled at this checkpoint.
    pub retained_bytes: Option<usize>,
    /// Retained size of all live phase state known at this checkpoint.
    pub active_retained_bytes: Option<usize>,
    /// Runtime resident memory reported by the executable, if available.
    pub resident_bytes: Option<usize>,
}

pub type ResidentMemorySampler = Box<dyn FnMut() -> Option<usize>>;

/// Optional profiling knobs for `Project::build_profiled`.
#[derive(Default)]
pub struct BuildProfileOptions {
    pub retained_memory: bool,
    pub resident_memory_sampler: Option<ResidentMemorySampler>,
}

pub(crate) struct BuildProfiler {
    started_at: Instant,
    retained_memory: bool,
    resident_memory_sampler: Option<ResidentMemorySampler>,
    checkpoints: Vec<BuildCheckpoint>,
}

impl BuildProfiler {
    pub(crate) fn disabled() -> Self {
        Self {
            started_at: Instant::now(),
            retained_memory: false,
            resident_memory_sampler: None,
            checkpoints: Vec::new(),
        }
    }

    pub(crate) fn new(options: BuildProfileOptions) -> Self {
        Self {
            started_at: Instant::now(),
            retained_memory: options.retained_memory,
            resident_memory_sampler: options.resident_memory_sampler,
            checkpoints: Vec::new(),
        }
    }

    pub(crate) fn measure<T>(&self, value: &T) -> Option<usize>
    where
        T: MemorySize,
    {
        self.retained_memory.then(|| value.memory_size())
    }

    pub(crate) fn sum_retained(&self, values: &[Option<usize>]) -> Option<usize> {
        self.retained_memory
            .then(|| values.iter().flatten().copied().sum())
    }

    pub(crate) fn sample_resident_memory(&mut self) -> Option<usize> {
        self.resident_memory_sampler
            .as_mut()
            .and_then(|sampler| sampler())
    }

    pub(crate) fn record(
        &mut self,
        label: &'static str,
        retained_bytes: Option<usize>,
        active_retained_bytes: Option<usize>,
        resident_bytes: Option<usize>,
    ) {
        if !self.is_enabled() {
            return;
        }

        self.checkpoints.push(BuildCheckpoint {
            label,
            elapsed: self.started_at.elapsed(),
            retained_bytes,
            active_retained_bytes,
            resident_bytes,
        });
    }

    pub(crate) fn finish(self) -> BuildProfile {
        BuildProfile::new(self.checkpoints)
    }

    fn is_enabled(&self) -> bool {
        self.retained_memory || self.resident_memory_sampler.is_some()
    }
}
