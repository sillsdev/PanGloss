//! Small shared helpers.

use crate::error::StatsError;

/// SQLite `INTEGER` is signed i64 but counters are u64; convert explicitly and error rather than wrap.
pub(crate) fn to_i64(counter: &'static str, value: u64) -> Result<i64, StatsError> {
    i64::try_from(value).map_err(|_| StatsError::CounterOverflow { counter, value })
}

/// `usize::MAX` ("unbounded") does not fit i64's positive range, so it gets its own sentinel.
const STEP_CAP_UNBOUNDED_SENTINEL: i64 = -1;

/// `--step-cap`'s storage form: finite caps round-trip via `to_i64`; `usize::MAX` becomes the sentinel.
pub(crate) fn step_cap_to_storage(step_cap: usize) -> Result<i64, StatsError> {
    if step_cap == usize::MAX {
        return Ok(STEP_CAP_UNBOUNDED_SENTINEL);
    }
    to_i64("step_cap", step_cap as u64)
}

/// Inverse of `step_cap_to_storage`.
pub(crate) fn step_cap_from_storage(stored: i64) -> usize {
    if stored == STEP_CAP_UNBOUNDED_SENTINEL {
        usize::MAX
    } else {
        stored as usize
    }
}

/// Renders a step cap for a human-facing message: "unbounded" for `usize::MAX`, the number otherwise.
pub(crate) fn format_step_cap(step_cap: usize) -> String {
    if step_cap == usize::MAX {
        "unbounded".to_string()
    } else {
        step_cap.to_string()
    }
}
