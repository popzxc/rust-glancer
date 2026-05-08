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
    /// Time spent since the previous checkpoint, or since build start for the first checkpoint.
    pub phase_elapsed: Duration,
    /// Time spent since build start.
    pub elapsed: Duration,
    /// Retained size of the object sampled at this checkpoint.
    pub retained_bytes: Option<usize>,
    /// Retained size of all live phase state known at this checkpoint.
    pub active_retained_bytes: Option<usize>,
    /// Runtime heap bytes allocated through the process allocator, if available.
    pub allocated_bytes: Option<usize>,
    /// Runtime heap bytes held in active allocator pages, if available.
    pub active_bytes: Option<usize>,
    /// Runtime resident memory reported by the executable, if available.
    pub resident_bytes: Option<usize>,
}

/// Process allocator counters sampled by the executable during a profiled build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildProcessMemory {
    pub allocated_bytes: usize,
    pub active_bytes: usize,
    pub resident_bytes: usize,
}

pub type ProcessMemorySampler = Box<dyn FnMut() -> Option<BuildProcessMemory>>;

pub(crate) struct BuildProfiler {
    started_at: Instant,
    timing: bool,
    retained_memory: bool,
    process_memory_sampler: Option<ProcessMemorySampler>,
    checkpoints: Vec<BuildCheckpoint>,
}

impl BuildProfiler {
    pub(crate) fn disabled() -> Self {
        Self {
            started_at: Instant::now(),
            timing: false,
            retained_memory: false,
            process_memory_sampler: None,
            checkpoints: Vec::new(),
        }
    }

    pub(crate) fn new(
        timing: bool,
        retained_memory: bool,
        process_memory_sampler: Option<ProcessMemorySampler>,
    ) -> Self {
        Self {
            started_at: Instant::now(),
            timing,
            retained_memory,
            process_memory_sampler,
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

    pub(crate) fn sample_process_memory(&mut self) -> Option<BuildProcessMemory> {
        self.process_memory_sampler
            .as_mut()
            .and_then(|sampler| sampler())
    }

    pub(crate) fn record(
        &mut self,
        label: &'static str,
        retained_bytes: Option<usize>,
        active_retained_bytes: Option<usize>,
        process_memory: Option<BuildProcessMemory>,
    ) {
        if !self.is_enabled() {
            return;
        }

        let elapsed = self.started_at.elapsed();
        let previous_elapsed = self
            .checkpoints
            .last()
            .map(|checkpoint| checkpoint.elapsed)
            .unwrap_or_default();

        self.checkpoints.push(BuildCheckpoint {
            label,
            phase_elapsed: elapsed.saturating_sub(previous_elapsed),
            elapsed,
            retained_bytes,
            active_retained_bytes,
            allocated_bytes: process_memory.map(|memory| memory.allocated_bytes),
            active_bytes: process_memory.map(|memory| memory.active_bytes),
            resident_bytes: process_memory.map(|memory| memory.resident_bytes),
        });
    }

    pub(crate) fn finish(self) -> BuildProfile {
        BuildProfile::new(self.checkpoints)
    }

    fn is_enabled(&self) -> bool {
        self.timing || self.retained_memory || self.process_memory_sampler.is_some()
    }
}
