//! Small shared helpers.

use crate::error::StatsError;

/// SQLite `INTEGER` is signed i64 but counters are u64; convert explicitly and error rather than wrap.
pub(crate) fn to_i64(counter: &'static str, value: u64) -> Result<i64, StatsError> {
    i64::try_from(value).map_err(|_| StatsError::CounterOverflow { counter, value })
}
