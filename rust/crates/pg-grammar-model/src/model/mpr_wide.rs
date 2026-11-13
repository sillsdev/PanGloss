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

/// Runs `f` on a handle's list in place; `f` must not intern, or the write lock deadlocks.
pub(super) fn with_words<R>(handle: u64, f: impl FnOnce(&[u64]) -> R) -> R {
    let read = TABLE.read().unwrap_or_else(PoisonError::into_inner);
    f(list(&read, handle))
}

/// [`with_words`] for two handles under one read lock.
pub(super) fn with_two<R>(a: u64, b: u64, f: impl FnOnce(&[u64], &[u64]) -> R) -> R {
    let read = TABLE.read().unwrap_or_else(PoisonError::into_inner);
    f(list(&read, a), list(&read, b))
}

fn list(table: &Option<Table>, handle: u64) -> &[u64] {
    table
        .as_ref()
        .and_then(|t| t.lists.get(handle as usize))
        .expect("a wide MprSet handle always names an interned list")
}

/// How many distinct wide sets this process has interned.
pub(super) fn len() -> usize {
    let read = TABLE.read().unwrap_or_else(PoisonError::into_inner);
    read.as_ref().map_or(0, |t| t.lists.len())
}
