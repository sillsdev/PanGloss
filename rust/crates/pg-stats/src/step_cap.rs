//! `StepCap`: the `--step-cap` value shared by `pg-cli`'s parsing and this crate's stats cache.
//!
//! A step cap is resource containment, never a correctness verdict (see `CLAUDE.md`'s "Classify
//! FST evidence before changing limits"): it bounds the analysis cascade so a batch
//! terminates deterministically, and firing it produces a typed incomplete outcome, never a wrong
//! answer.

use std::fmt;
use std::num::NonZeroU64;
use std::str::FromStr;

use crate::error::StatsError;

/// A `--step-cap` value: no bound, or a positive step count. `Finite` excludes zero at the type
/// level, since a zero cap would fire before the first step -- indistinguishable from a bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepCap {
    Unbounded,
    Finite(NonZeroU64),
}

impl StepCap {
    /// SQLite storage sentinel for `Unbounded`; never collides with a `Finite` cap, which is always `>= 1`.
    const UNBOUNDED_SENTINEL: i64 = -1;

    /// The `usize` step budget `pg_parse::Morpher::new` takes; `Unbounded` saturates to `usize::MAX`.
    pub fn as_morpher_cap(self) -> usize {
        match self {
            StepCap::Unbounded => usize::MAX,
            StepCap::Finite(n) => usize::try_from(n.get()).unwrap_or(usize::MAX),
        }
    }

    /// This cap's SQLite storage form: finite caps round-trip via `i64`, `Unbounded` becomes the sentinel.
    pub fn to_storage(self) -> Result<i64, StatsError> {
        match self {
            StepCap::Unbounded => Ok(Self::UNBOUNDED_SENTINEL),
            StepCap::Finite(n) => i64::try_from(n.get()).map_err(|_| StatsError::CounterOverflow {
                counter: "step_cap",
                value: n.get(),
            }),
        }
    }

    /// Inverse of `to_storage`. Defensive against a hand-edited row (`schema.sql` is a documented
    /// public escape hatch): a non-sentinel value `<= 0` clamps to `1` instead of panicking.
    pub fn from_storage(stored: i64) -> Self {
        if stored == Self::UNBOUNDED_SENTINEL {
            return StepCap::Unbounded;
        }
        StepCap::Finite(NonZeroU64::new(stored.max(1) as u64).unwrap_or(NonZeroU64::MIN))
    }
}

impl fmt::Display for StepCap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StepCap::Unbounded => write!(f, "unbounded"),
            StepCap::Finite(n) => write!(f, "{n}"),
        }
    }
}

impl FromStr for StepCap {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "unbounded" {
            return Ok(StepCap::Unbounded);
        }
        let n: u64 = s.parse().map_err(|_| {
            format!("invalid step cap {s:?}: expected \"unbounded\" or a positive integer")
        })?;
        NonZeroU64::new(n).map(StepCap::Finite).ok_or_else(|| {
            "--step-cap 0 is rejected: a zero cap fires before the first step".to_string()
        })
    }
}

impl serde::Serialize for StepCap {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unbounded() {
        assert_eq!("unbounded".parse::<StepCap>().unwrap(), StepCap::Unbounded);
    }

    #[test]
    fn parses_a_positive_integer() {
        assert_eq!(
            "100".parse::<StepCap>().unwrap(),
            StepCap::Finite(NonZeroU64::new(100).unwrap())
        );
    }

    #[test]
    fn rejects_zero_with_a_specific_message() {
        let err = "0".parse::<StepCap>().unwrap_err();
        assert!(err.contains("fires before the first step"), "{err}");
    }

    #[test]
    fn rejects_garbage() {
        assert!("banana".parse::<StepCap>().is_err());
    }

    #[test]
    fn displays_unbounded_and_the_number() {
        assert_eq!(StepCap::Unbounded.to_string(), "unbounded");
        assert_eq!(
            StepCap::Finite(NonZeroU64::new(42).unwrap()).to_string(),
            "42"
        );
    }

    #[test]
    fn storage_round_trips_both_variants() {
        let unbounded = StepCap::Unbounded;
        assert_eq!(
            StepCap::from_storage(unbounded.to_storage().unwrap()),
            unbounded
        );
        let finite = StepCap::Finite(NonZeroU64::new(50_000_000).unwrap());
        assert_eq!(StepCap::from_storage(finite.to_storage().unwrap()), finite);
    }

    #[test]
    fn as_morpher_cap_maps_unbounded_to_usize_max() {
        assert_eq!(StepCap::Unbounded.as_morpher_cap(), usize::MAX);
        assert_eq!(
            StepCap::Finite(NonZeroU64::new(7).unwrap()).as_morpher_cap(),
            7
        );
    }
}
