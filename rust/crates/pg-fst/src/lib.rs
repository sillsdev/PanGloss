//! FST engine (plan §5.4). Only the FSA/acceptor path with registers that HermitCrab actually
//! exercises — #446 proved `Matcher.Compile()` never passes `operations`, so the transducer
//! output path is dead code for this workload and is not ported.
//!
//! Pipeline: `compile::CompileInput` → Thompson `nfa::Nfa` → `optimize`
//! (`Determinize`/`EpsilonRemoval`, subset construction with epsilon-closure and capture-tag
//! registers) → frozen CSR `Fst` → `traverse` (iterative, register scaffold, `ResultCompare`
//! ordering).
//!
//! Storage is CSR: `states` + `arcs` sorted by source state, arc constraints interned as
//! bit-vectors (§5.3). Traversal is iterative with an explicit stack, a flat reusable register
//! scaffold, and an inline visited set.
//!
//! ## Deliberate scope cuts vs `Fst.cs` (all behavior-preserving for the acceptor; see report)
//! - transducer outputs: `enqueueCount ≡ 0`, `Outputs ≡ ∅` — collapses the common-prefix/dequeue
//!   logic in `GetAllArcsForInput` and the "remaining"/output arcs in `CreateOptimizedState`;
//! - `Minimize()` (Matcher.cs:100): skipped — language/capture/priority/order preserving;
//! - lazy matching: no lazy quantifier in the model, so `IsLazy ≡ false` and `IsLazyAcceptingState`
//!   is not ported (the `ResultCompare` lazy flip, Fst.cs:427, stays inert);
//! - traversal is direction-aware (L2R and R2L), mirroring `Fst.Transduce`'s `_dir` handling:
//!   `GetNodes(_dir)` ordering + `Range.GetStart/GetEnd(_dir)` (see `traverse`); `ResultCompare`
//!   handles the right-to-left sign (Fst.cs:423).
#![forbid(unsafe_code)]

pub mod compile;
mod fst;
pub mod lanes;
pub mod nfa;
mod optimize;
mod traverse;

pub use compile::{CompileInput, CompileNode, ENTIRE_MATCH};
pub use fst::{Arc, Fst, StateMeta};
pub use traverse::{profile, FstResult, Segment, Transduce};

/// Dense FST state id (CSR index).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct StateId(pub u32);

/// Interned arc-constraint id (index into `Fst`'s constraint pool, §5.4).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ConstraintId(pub u32);

/// Traversal direction. Mirrors C# `Direction`; consulted by the walk (segment order + offset
/// sign, see `crate::traverse`) and by `ResultCompare`.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Direction {
    #[default]
    LeftToRight,
    RightToLeft,
}

/// An accepting-state annotation: the pattern-path id and its priority (C# `AcceptInfo`,
/// `CreateAcceptingState(id, acceptable, priority)`; `Acceptable` predicates are not modeled).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptInfo {
    pub id: Option<String>,
    pub priority: i32,
}

/// `TagMapCommand.CurrentPosition` sentinel (C# `-1`): copy the *current position* into `dest`.
pub const CURRENT_POSITION: i32 = -1;

/// A register-copy command (C# `TagMapCommand`): `dest ← src`, or `dest ← current position` when
/// `src == CURRENT_POSITION`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Cmd {
    pub dest: i32,
    pub src: i32,
}

/// Ports C# `TagMapCommand.CompareTo`: current-position sources sort last; otherwise by `dest`.
impl Ord for Cmd {
    fn cmp(&self, other: &Cmd) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let a_cur = self.src == CURRENT_POSITION;
        let b_cur = other.src == CURRENT_POSITION;
        match (a_cur, b_cur) {
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            _ => self.dest.cmp(&other.dest),
        }
    }
}

impl PartialOrd for Cmd {
    fn partial_cmp(&self, other: &Cmd) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// A capture register (C# `Register<TOffset>` with `TOffset == int`). The engine keeps
/// a flat `Vec<Register>` scaffold of `register_count * 2` slots (two columns per register, as
/// C#'s `Register[reg, col]`) reused across `Transduce` calls. `has` distinguishes an unset
/// register from offset 0 — `Fst::get_offsets` treats an unset/empty range as absent, so a bare
/// `i32` would mis-report zero-width captures.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct Register {
    pub offset: i32,
    pub start: bool,
    pub has: bool,
}

impl Register {
    #[inline]
    pub const fn unset() -> Register {
        Register {
            offset: 0,
            start: false,
            has: false,
        }
    }
    #[inline]
    pub const fn at(offset: i32, start: bool) -> Register {
        Register {
            offset,
            start,
            has: true,
        }
    }
    /// C# `Register.ValueEquals`.
    #[inline]
    pub fn value_eq(&self, other: &Register) -> bool {
        if self.has != other.has {
            return false;
        }
        !self.has || (self.offset == other.offset && self.start == other.start)
    }
}
