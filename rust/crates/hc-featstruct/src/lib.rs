//! Feature structures for the HermitCrab engine (plan §5.3).
//!
//! Two representations share this crate:
//! - **Bit-vector symbolic FS** — the primary form for closed-class segment-domain features
//!   (phonetic features on shape nodes, natural-class constraints on FST arcs). One `u64` lane
//!   per symbolic feature; unify = lane-wise AND. This is what C# #446 proved is the hot-path
//!   common case (its `ulong[]` flat-unify fast path), made primary here.
//! - **DAG unifier** — the full port for syntactic/head/foot/realizational feature structures
//!   that can nest, carry string features, and bind variables.
//!
//! Frozen instances are interned to `FsId(u32)` so the memo key and all gating comparisons are
//! integer compares (plan §5.3, §6.3).
#![forbid(unsafe_code)]

pub mod bitvec;
pub mod interner;
pub mod ops;
pub mod tree;

pub use bitvec::{flat_unifiable, full_mask, SymbolBits};
pub use interner::Interner;
pub use ops::{add, is_unifiable, priority_union, subsumes, subtract, unify};
pub use tree::{FeatId, FeatureStruct, FeatureStructBuilder, FeatureValue};

/// Stable per-grammar identity of a frozen feature structure (plan §5.3).
///
/// Assigned by an [`Interner`]; equality is a `u32` compare, which is the whole point — it turns
/// the memo key and every gating comparison into integer compares (plan §6.3).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct FsId(pub u32);

// Hot-struct size discipline (plan §9): these are compared/copied in the traversal and memo hot
// paths, so a refactor must not silently fatten them. `SymbolBits` is one feature's symbol set;
// `FsId` is an interned handle.
const _: () = assert!(std::mem::size_of::<SymbolBits>() == 8);
const _: () = assert!(std::mem::size_of::<FsId>() == 4);
