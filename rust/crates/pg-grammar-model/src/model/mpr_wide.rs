//! One process-wide table of canonical word lists, so a wide `MprSet` is a `Copy` handle and handle equality is set equality.

use std::collections::HashMap;
use std::sync::{PoisonError, RwLock};

#[derive(Default)]
struct Table {
    lists: Vec<Vec<u64>>,
    handles: HashMap<Vec<u64>, u64>,
}

static TABLE: RwLock<Option<Table>> = RwLock::new(None);

/// The handle for `words` (no trailing zero word), adding it on first sight.
pub(super) fn intern(words: Vec<u64>) -> u64 {
    {
        let read = TABLE.read().unwrap_or_else(PoisonError::into_inner);
        if let Some(handle) = read.as_ref().and_then(|t| t.handles.get(&words)) {
            return *handle;
        }
    }
    let mut write = TABLE.write().unwrap_or_else(PoisonError::into_inner);
    let table = write.get_or_insert_with(Table::default);
    if let Some(handle) = table.handles.get(&words) {
        return *handle;
    }
    let handle = table.lists.len() as u64;
    table.lists.push(words.clone());
    table.handles.insert(words, handle);
    handle
}

/// The list a handle from [`intern`] names.
pub(super) fn words(handle: u64) -> Vec<u64> {
    let read = TABLE.read().unwrap_or_else(PoisonError::into_inner);
    read.as_ref()
        .and_then(|t| t.lists.get(handle as usize))
        .cloned()
        .expect("a wide MprSet handle always names an interned list")
}
