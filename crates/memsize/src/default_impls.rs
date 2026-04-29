use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    ffi::OsString,
    hash::{BuildHasher, Hash},
    mem,
    path::PathBuf,
};

use crate::{MemoryRecorder, MemorySize};

macro_rules! impl_leaf_memory_size {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl MemorySize for $ty {
                fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
            }
        )+
    };
}

impl_leaf_memory_size!(
    (),
    bool,
    char,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
);

impl<T> MemorySize for &T {
    fn record_memory_children(&self, _recorder: &mut MemoryRecorder) {}
}

impl<T> MemorySize for Option<T>
where
    T: MemorySize,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        if let Some(value) = self {
            recorder.scope("some", |recorder| value.record_memory_children(recorder));
        }
    }
}

impl<T, E> MemorySize for Result<T, E>
where
    T: MemorySize,
    E: MemorySize,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Ok(value) => recorder.scope("ok", |recorder| value.record_memory_children(recorder)),
            Err(error) => recorder.scope("err", |recorder| error.record_memory_children(recorder)),
        }
    }
}

impl<T> MemorySize for Box<T>
where
    T: MemorySize,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("box", |recorder| {
            recorder.record_heap::<T>(mem::size_of::<T>());
            (**self).record_memory_children(recorder);
        });
    }
}

impl<T> MemorySize for Vec<T>
where
    T: MemorySize,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("items", |recorder| {
            recorder.record_heap::<T>(self.len().saturating_mul(mem::size_of::<T>()));

            for item in self {
                item.record_memory_children(recorder);
            }
        });

        let spare = self.capacity().saturating_sub(self.len());
        recorder.scope("spare_capacity", |recorder| {
            recorder.record_spare_capacity::<T>(spare.saturating_mul(mem::size_of::<T>()));
        });
    }
}

impl<T, const N: usize> MemorySize for [T; N]
where
    T: MemorySize,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("items", |recorder| {
            for item in self {
                item.record_memory_children(recorder);
            }
        });
    }
}

impl MemorySize for String {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.record_heap::<String>(self.capacity());
    }
}

impl MemorySize for OsString {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.record_approximate::<OsString>(self.as_encoded_bytes().len());
    }
}

impl MemorySize for PathBuf {
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.record_approximate::<PathBuf>(self.as_os_str().as_encoded_bytes().len());
    }
}

impl<'a, B> MemorySize for Cow<'a, B>
where
    B: ToOwned + ?Sized,
    B::Owned: MemorySize,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        match self {
            Cow::Borrowed(_) => {}
            Cow::Owned(value) => value.record_memory_children(recorder),
        }
    }
}

impl<K, V, S> MemorySize for HashMap<K, V, S>
where
    K: MemorySize + Eq + Hash,
    V: MemorySize,
    S: BuildHasher,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("entries", |recorder| {
            // HashMap hides bucket/control-byte layout. Key/value payload bytes are useful as
            // heap attribution; spare slot storage remains approximate.
            recorder.record_heap::<K>(self.len().saturating_mul(mem::size_of::<K>()));
            recorder.record_heap::<V>(self.len().saturating_mul(mem::size_of::<V>()));

            for (key, value) in self {
                recorder.scope("key", |recorder| key.record_memory_children(recorder));
                recorder.scope("value", |recorder| value.record_memory_children(recorder));
            }
        });

        let spare = self.capacity().saturating_sub(self.len());
        recorder.scope("spare_capacity", |recorder| {
            recorder.record_approximate::<HashMap<K, V, S>>(
                spare.saturating_mul(mem::size_of::<K>().saturating_add(mem::size_of::<V>())),
            );
        });
    }
}

impl<T, S> MemorySize for HashSet<T, S>
where
    T: MemorySize + Eq + Hash,
    S: BuildHasher,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("items", |recorder| {
            recorder.record_heap::<T>(self.len().saturating_mul(mem::size_of::<T>()));

            for item in self {
                item.record_memory_children(recorder);
            }
        });

        let spare = self.capacity().saturating_sub(self.len());
        recorder.scope("spare_capacity", |recorder| {
            recorder.record_approximate::<HashSet<T, S>>(spare.saturating_mul(mem::size_of::<T>()));
        });
    }
}

impl<K, V> MemorySize for BTreeMap<K, V>
where
    K: MemorySize,
    V: MemorySize,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("entries", |recorder| {
            // BTree node layout is private, so entry payload storage is intentionally approximate.
            recorder.record_approximate::<BTreeMap<K, V>>(
                self.len()
                    .saturating_mul(mem::size_of::<K>().saturating_add(mem::size_of::<V>())),
            );

            for (key, value) in self {
                recorder.scope("key", |recorder| key.record_memory_children(recorder));
                recorder.scope("value", |recorder| value.record_memory_children(recorder));
            }
        });
    }
}

impl<T> MemorySize for BTreeSet<T>
where
    T: MemorySize,
{
    fn record_memory_children(&self, recorder: &mut MemoryRecorder) {
        recorder.scope("items", |recorder| {
            recorder
                .record_approximate::<BTreeSet<T>>(self.len().saturating_mul(mem::size_of::<T>()));

            for item in self {
                item.record_memory_children(recorder);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{any, collections::BTreeMap, mem};

    use crate::{MemoryRecordKind, MemoryRecorder, MemorySize};

    #[test]
    fn records_string_shallow_and_heap_capacity() {
        let mut value = String::with_capacity(16);
        value.push_str("api");

        assert_eq!(value.memory_size(), mem::size_of::<String>() + 16);

        let mut recorder = MemoryRecorder::new("string");
        value.record_memory_size(&mut recorder);

        let totals = recorder.totals_by_kind();
        assert_eq!(
            totals.get(&MemoryRecordKind::Shallow),
            Some(&mem::size_of::<String>())
        );
        assert_eq!(totals.get(&MemoryRecordKind::Heap), Some(&16));
    }

    #[test]
    fn option_records_inline_value_once_but_keeps_owned_children() {
        let mut value = String::with_capacity(11);
        value.push_str("user");
        let value = Some(value);

        assert_eq!(value.memory_size(), mem::size_of::<Option<String>>() + 11);

        let mut recorder = MemoryRecorder::new("option");
        value.record_memory_size(&mut recorder);
        let totals = recorder.totals_by_kind();

        assert_eq!(
            totals.get(&MemoryRecordKind::Shallow),
            Some(&mem::size_of::<Option<String>>())
        );
        assert_eq!(totals.get(&MemoryRecordKind::Heap), Some(&11));
    }

    #[test]
    fn box_records_pointee_storage_as_heap() {
        let mut value = String::with_capacity(5);
        value.push_str("tool");
        let value = Box::new(value);

        assert_eq!(
            value.memory_size(),
            mem::size_of::<Box<String>>() + mem::size_of::<String>() + 5
        );

        let mut recorder = MemoryRecorder::new("box");
        value.record_memory_size(&mut recorder);
        let totals = recorder.totals_by_kind();

        assert_eq!(
            totals.get(&MemoryRecordKind::Shallow),
            Some(&mem::size_of::<Box<String>>())
        );
        assert_eq!(
            totals.get(&MemoryRecordKind::Heap),
            Some(&(mem::size_of::<String>() + 5))
        );
    }

    #[test]
    fn vec_records_initialized_items_as_heap_and_spare_capacity_separately() {
        let mut item = String::with_capacity(5);
        item.push_str("tool");
        let mut value = Vec::with_capacity(2);
        value.push(item);

        assert_eq!(
            value.memory_size(),
            mem::size_of::<Vec<String>>() + 2 * mem::size_of::<String>() + 5
        );

        let mut recorder = MemoryRecorder::new("vec");
        value.record_memory_size(&mut recorder);
        let totals = recorder.totals_by_kind();

        assert_eq!(
            totals.get(&MemoryRecordKind::Shallow),
            Some(&mem::size_of::<Vec<String>>())
        );
        assert_eq!(
            totals.get(&MemoryRecordKind::Heap),
            Some(&(mem::size_of::<String>() + 5))
        );
        assert_eq!(
            totals.get(&MemoryRecordKind::SpareCapacity),
            Some(&mem::size_of::<String>())
        );
    }

    #[test]
    fn vec_records_element_type_names() {
        let mut value = Vec::with_capacity(3);
        value.push(10_u32);
        value.push(20_u32);

        let mut recorder = MemoryRecorder::new("vec");
        value.record_memory_size(&mut recorder);

        let totals = recorder.totals_by_type();
        assert_eq!(
            totals.get(any::type_name::<Vec<u32>>()),
            Some(&mem::size_of::<Vec<u32>>())
        );
        assert_eq!(
            totals.get(any::type_name::<u32>()),
            Some(&(3 * mem::size_of::<u32>()))
        );
    }

    #[test]
    fn map_records_hidden_capacity_as_approximate() {
        let mut value = BTreeMap::new();
        value.insert("one".to_owned(), "two".to_owned());

        let mut recorder = MemoryRecorder::new("map");
        value.record_memory_size(&mut recorder);
        let totals = recorder.totals_by_kind();

        assert!(totals.contains_key(&MemoryRecordKind::Approximate));
        assert!(totals.contains_key(&MemoryRecordKind::Heap));
    }
}
