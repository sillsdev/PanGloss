//! `WorkCap`: the `--work-cap` value shared by CLI parsing and the stats cache.
//!
//! Resource containment for per-word work: the total of charged work steps.
//! Exhaustion produces an incomplete outcome; it cannot establish a correctness verdict.

use std::fmt;
use std::num::NonZeroU64;
use std::str::FromStr;

use crate::error::StatsError;

/// A `--work-cap` value: no bound, or a positive count of work steps. `Finite` excludes zero at the type
/// level, since a zero cap would fire before the first work step -- indistinguishable from a bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkCap {
    Unbounded,
    Finite(NonZeroU64),
}

impl WorkCap {
    /// SQLite storage sentinel for `Unbounded`; never collides with a `Finite` cap, which is always `>= 1`.
    const UNBOUNDED_SENTINEL: i64 = -1;

    /// The `usize` work cap `pg_parse::Morpher::new` takes; `Unbounded` saturates to `usize::MAX`.
    pub fn as_morpher_cap(self) -> usize {
        match self {
            WorkCap::Unbounded => usize::MAX,
            WorkCap::Finite(n) => usize::try_from(n.get()).unwrap_or(usize::MAX),
        }
    }

    /// This cap's SQLite storage form: finite caps round-trip via `i64`, `Unbounded` becomes the sentinel.
    pub fn to_storage(self) -> Result<i64, StatsError> {
        match self {
            WorkCap::Unbounded => Ok(Self::UNBOUNDED_SENTINEL),
            WorkCap::Finite(n) => i64::try_from(n.get()).map_err(|_| StatsError::CounterOverflow {
                counter: "work_cap",
                value: n.get(),
            }),
        }
    }

    /// Inverse of `to_storage`. Defensive against a hand-edited row (`schema.sql` is a documented
    /// public escape hatch): a non-sentinel value `<= 0` clamps to `1` instead of panicking.
    pub fn from_storage(stored: i64) -> Self {
        if stored == Self::UNBOUNDED_SENTINEL {
            return WorkCap::Unbounded;
        }
        WorkCap::Finite(NonZeroU64::new(stored.max(1) as u64).unwrap_or(NonZeroU64::MIN))
    }
}

impl fmt::Display for WorkCap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkCap::Unbounded => write!(f, "unbounded"),
            WorkCap::Finite(n) => write!(f, "{n}"),
        }
    }
}

impl FromStr for WorkCap {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "unbounded" {
            return Ok(WorkCap::Unbounded);
        }
        let n: u64 = s.parse().map_err(|_| {
            format!("invalid work cap {s:?}: expected \"unbounded\" or a positive integer")
        })?;
        NonZeroU64::new(n).map(WorkCap::Finite).ok_or_else(|| {
            "--work-cap 0 is rejected: a zero cap fires before the first work step".to_string()
        })
    }
}

impl serde::Serialize for WorkCap {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(test)]
mod tests;
