//! Lightweight, approximate memory attribution for rust-glancer data structures.
//!
//! The goal is not allocator-perfect accounting. We want stable, tagged measurements that explain
//! which retained phase data deserves optimization attention.

use std::{any, collections::BTreeMap, mem};

mod default_impls;
#[cfg(feature = "ls_types")]
mod ls_types_impls;
#[cfg(feature = "ra_syntax")]
mod ra_syntax_impls;

/// Records approximate retained memory for a value.
///
/// Implementations follow a two-part convention:
/// - `record_memory_size` records the value's inline/shallow size;
/// - `record_memory_children` records memory owned behind pointers or container buffers.
///
/// Manual struct impls can usually call the default `record_memory_size` and then call
/// `record_memory_children` on inline fields to avoid double-counting those fields' shallow bytes.
/// If field-level attribution matters, override `record_memory_size` and record inline fields
/// explicitly, with any padding/unknown remainder tagged back to the parent type.
pub trait MemorySize {
    fn record_memory_size(&self, recorder: &mut MemoryRecorder)
    where
        Self: Sized,
    {
        recorder.record_shallow::<Self>(mem::size_of::<Self>());
        self.record_memory_children(recorder);
    }

    fn record_memory_children(&self, recorder: &mut MemoryRecorder);

    fn memory_size(&self) -> usize
    where
        Self: Sized,
    {
        let mut recorder = MemoryRecorder::new("root");
        self.record_memory_size(&mut recorder);
        recorder.total_bytes()
    }
}

/// One memory contribution attached to the current recorder path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRecord {
    pub path: String,
    pub type_name: String,
    pub kind: MemoryRecordKind,
    pub bytes: usize,
}

/// Coarse explanation of where retained bytes live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MemoryRecordKind {
    /// Inline bytes of the measured value itself.
    Shallow,
    /// Initialized bytes in a heap allocation owned by a pointer/container.
    Heap,
    /// Allocated but currently unused container capacity.
    SpareCapacity,
    /// Best-effort accounting for layouts hidden by upstream crates or std.
    Approximate,
}

/// Controls whether the recorder also keeps every individual contribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRecorderMode {
    /// Keep only totals for each `(path, type_name, kind)` bucket.
    Aggregate,
    /// Keep aggregated totals plus the raw contribution stream.
    Detailed,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MemoryRecordKey {
    path: String,
    type_name: String,
    kind: MemoryRecordKind,
}

/// Accumulates tagged memory records while preserving a logical path.
///
/// Recording is aggregated by default because project-wide profiles can emit hundreds of thousands
/// of contributions. Detailed mode is available for debugging recorder implementations, but normal
/// reports should depend on the grouped totals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRecorder {
    path: Vec<String>,
    records: BTreeMap<MemoryRecordKey, usize>,
    raw_records: Option<Vec<MemoryRecord>>,
}

impl MemoryRecorder {
    pub fn new(root: impl Into<String>) -> Self {
        Self::with_mode(root, MemoryRecorderMode::Aggregate)
    }

    pub fn detailed(root: impl Into<String>) -> Self {
        Self::with_mode(root, MemoryRecorderMode::Detailed)
    }

    pub fn with_mode(root: impl Into<String>, mode: MemoryRecorderMode) -> Self {
        Self {
            path: vec![root.into()],
            records: BTreeMap::new(),
            raw_records: match mode {
                MemoryRecorderMode::Aggregate => None,
                MemoryRecorderMode::Detailed => Some(Vec::new()),
            },
        }
    }

    pub fn scope<R>(&mut self, label: impl Into<String>, f: impl FnOnce(&mut Self) -> R) -> R {
        self.path.push(label.into());
        let result = f(self);
        self.path.pop();
        result
    }

    pub fn record_shallow<T>(&mut self, bytes: usize)
    where
        T: ?Sized,
    {
        self.record::<T>(MemoryRecordKind::Shallow, bytes);
    }

    pub fn record_heap<T>(&mut self, bytes: usize)
    where
        T: ?Sized,
    {
        self.record::<T>(MemoryRecordKind::Heap, bytes);
    }

    pub fn record_spare_capacity<T>(&mut self, bytes: usize)
    where
        T: ?Sized,
    {
        self.record::<T>(MemoryRecordKind::SpareCapacity, bytes);
    }

    pub fn record_approximate<T>(&mut self, bytes: usize)
    where
        T: ?Sized,
    {
        self.record::<T>(MemoryRecordKind::Approximate, bytes);
    }

    pub fn record<T>(&mut self, kind: MemoryRecordKind, bytes: usize)
    where
        T: ?Sized,
    {
        self.record_type_name(kind, any::type_name::<T>(), bytes);
    }

    pub fn record_type_name(
        &mut self,
        kind: MemoryRecordKind,
        type_name: impl Into<String>,
        bytes: usize,
    ) {
        if bytes == 0 {
            return;
        }

        let path = self.path.join(".");
        let type_name = type_name.into();
        let key = MemoryRecordKey {
            path: path.clone(),
            type_name: type_name.clone(),
            kind,
        };
        *self.records.entry(key).or_default() += bytes;

        if let Some(raw_records) = &mut self.raw_records {
            raw_records.push(MemoryRecord {
                path,
                type_name,
                kind,
                bytes,
            });
        }
    }

    pub fn mode(&self) -> MemoryRecorderMode {
        if self.raw_records.is_some() {
            MemoryRecorderMode::Detailed
        } else {
            MemoryRecorderMode::Aggregate
        }
    }

    pub fn records(&self) -> Vec<MemoryRecord> {
        self.records
            .iter()
            .map(|(key, bytes)| MemoryRecord {
                path: key.path.clone(),
                type_name: key.type_name.clone(),
                kind: key.kind,
                bytes: *bytes,
            })
            .collect()
    }

    pub fn raw_records(&self) -> Option<&[MemoryRecord]> {
        self.raw_records.as_deref()
    }

    pub fn total_bytes(&self) -> usize {
        self.records.values().sum()
    }

    pub fn totals_by_path(&self) -> BTreeMap<&str, usize> {
        let mut totals = BTreeMap::new();
        for (key, bytes) in &self.records {
            *totals.entry(key.path.as_str()).or_default() += bytes;
        }
        totals
    }

    pub fn totals_by_kind(&self) -> BTreeMap<MemoryRecordKind, usize> {
        let mut totals = BTreeMap::new();
        for (key, bytes) in &self.records {
            *totals.entry(key.kind).or_default() += bytes;
        }
        totals
    }

    pub fn totals_by_type(&self) -> BTreeMap<&str, usize> {
        let mut totals = BTreeMap::new();
        for (key, bytes) in &self.records {
            *totals.entry(key.type_name.as_str()).or_default() += bytes;
        }
        totals
    }
}

#[cfg(test)]
mod tests {
    use std::{any, collections::BTreeMap};

    use super::{MemoryRecordKind, MemoryRecorder, MemoryRecorderMode};

    #[test]
    fn recorder_keeps_scoped_paths_and_totals() {
        let mut recorder = MemoryRecorder::new("project");
        recorder.scope("parse", |recorder| {
            recorder.record_heap::<String>(40);
            recorder.scope("files", |recorder| recorder.record_shallow::<usize>(2));
        });
        recorder.scope("body_ir", |recorder| {
            recorder.record_spare_capacity::<Vec<u8>>(8)
        });

        let totals = recorder.totals_by_path();
        assert_eq!(totals.get("project.parse"), Some(&40));
        assert_eq!(totals.get("project.parse.files"), Some(&2));
        assert_eq!(totals.get("project.body_ir"), Some(&8));
        assert_eq!(recorder.total_bytes(), 50);
    }

    #[test]
    fn recorder_summarizes_by_kind() {
        let mut recorder = MemoryRecorder::new("root");
        recorder.record_shallow::<usize>(3);
        recorder.record_heap::<String>(5);
        recorder.record_heap::<Vec<u8>>(7);

        let mut expected = BTreeMap::new();
        expected.insert(MemoryRecordKind::Shallow, 3);
        expected.insert(MemoryRecordKind::Heap, 12);

        assert_eq!(recorder.totals_by_kind(), expected);
    }

    #[test]
    fn recorder_attaches_type_names_to_records() {
        let mut recorder = MemoryRecorder::new("root");
        recorder.record_shallow::<usize>(8);
        recorder.record_heap::<String>(13);

        let records = recorder.records();
        assert!(
            records
                .iter()
                .any(|record| record.type_name == any::type_name::<usize>())
        );
        assert!(
            records
                .iter()
                .any(|record| record.type_name == any::type_name::<String>())
        );

        let totals = recorder.totals_by_type();
        assert_eq!(totals.get(any::type_name::<usize>()), Some(&8));
        assert_eq!(totals.get(any::type_name::<String>()), Some(&13));
    }

    #[test]
    fn recorder_can_attach_custom_type_names() {
        let mut recorder = MemoryRecorder::new("root");
        recorder.record_type_name(MemoryRecordKind::Approximate, "rowan::GreenToken", 21);

        let records = recorder.records();
        let record = &records[0];
        assert_eq!(record.type_name, "rowan::GreenToken");
        assert_eq!(record.bytes, 21);
    }

    #[test]
    fn recorder_aggregates_duplicate_contributions_by_default() {
        let mut recorder = MemoryRecorder::new("root");
        recorder.record_heap::<String>(5);
        recorder.record_heap::<String>(7);

        let records = recorder.records();
        assert_eq!(recorder.mode(), MemoryRecorderMode::Aggregate);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].bytes, 12);
        assert_eq!(recorder.raw_records(), None);
    }

    #[test]
    fn detailed_recorder_keeps_raw_contributions() {
        let mut recorder = MemoryRecorder::detailed("root");
        recorder.record_heap::<String>(5);
        recorder.record_heap::<String>(7);

        let records = recorder.records();
        let raw_records = recorder
            .raw_records()
            .expect("detailed recorder should keep raw records");

        assert_eq!(recorder.mode(), MemoryRecorderMode::Detailed);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].bytes, 12);
        assert_eq!(raw_records.len(), 2);
        assert_eq!(raw_records[0].bytes, 5);
        assert_eq!(raw_records[1].bytes, 7);
    }
}
