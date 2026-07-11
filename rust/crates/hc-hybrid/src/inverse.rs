//! `inverse.rs` (F7, HYBRID_FST_RUST_PLAN.md §8) — port of C# `InversePhonology.cs`: the
//! inverse-phonology transducer substrate (surface → underlying) that `compiler.rs`
//! (`RuleInverseCompiler`), `compiler_v1.rs` (`PhonologyRuleCompiler`), and `walk.rs`'s chain half
//! (`AnalyzeChain`/`CascadeSymbol`/`ChainClosure`) all build/walk.
//!
//! ## Representation
//! C# arcs carry a `FeatureStruct` (`SurfaceInput`/`UnderlyingOutput`, `null` = an ε-kind). This
//! port represents that FeatureStruct as a plain phonological feature-lane row (`Vec<u64>`,
//! `hc_featstruct::flat_unifiable`'s own representation — the SAME representation `trie.rs`'s
//! `ArcLabel::Segment`/`Boundary` and `walk.rs`'s `InputSegment` already use), NOT the
//! `(char_def, lanes)` pair those two also carry.
//!
//! **PARITY note (a deliberate, scoped representation decision, not an oversight):** on a grammar
//! with a NON-EMPTY declared phonological feature system (every rule-bearing grammar in this port
//! — Indonesian, Amharic, and the F7 toy grammar all declare real features), `flat_unifiable` on
//! two concrete segments' own lanes already discriminates them as finely as C#'s `FeatureStruct.
//! IsUnifiable` does (C#'s `StrRep`/char-def-identity special case, documented at length on
//! `trie.rs`'s `ArcLabel` and `walk.rs`'s `arc_matches_segment`, matters ONLY for a grammar that
//! declares ZERO phonological features — Sena is exactly that grammar, but Sena has zero
//! phonological rules, so `compiler.rs`/`compiler_v1.rs` never run on it at all and this module is
//! never exercised there). Carrying lanes only (no `char_def`) is therefore behavior-identical to
//! C# on every grammar this milestone's gates actually walk a chain over; it would only under-
//! discriminate on a hypothetical zero-phon-feature grammar that ALSO had phonological rules, which
//! does not exist among the three reference grammars or the new toy grammar.

use hc_featstruct::flat_unifiable;

pub type StateId = u32;

/// One inverse-phonology arc (C# `InversePhonology.Arc`). `surface = None` is an ε-input
/// (deletion-restoration) arc; `underlying = None` is an ε-output (epenthesis-inverse) arc; both
/// `None` is a structural epsilon (a pure state move, e.g. `EnvNfaCompiler`'s quantifier/
/// alternation plumbing).
#[derive(Clone, Debug)]
pub struct Arc {
    pub surface: Option<Vec<u64>>,
    pub underlying: Option<Vec<u64>>,
    pub target: StateId,
}

impl Arc {
    #[inline]
    pub fn is_epsilon_input(&self) -> bool {
        self.surface.is_none()
    }

    #[inline]
    pub fn is_epsilon_output(&self) -> bool {
        self.underlying.is_none()
    }

    #[inline]
    pub fn is_structural_epsilon(&self) -> bool {
        self.is_epsilon_input() && self.is_epsilon_output()
    }

    /// C# `arc.SurfaceInput.IsUnifiable(symbol)` — only meaningful when `!is_epsilon_input()`.
    #[inline]
    pub fn surface_unifiable(&self, symbol: &[u64]) -> bool {
        match &self.surface {
            Some(lanes) => flat_unifiable(lanes, symbol),
            None => false,
        }
    }
}

/// C# `InversePhonology`: states are plain integer handles (never pre-declared — a caller mints
/// them via its own counter, exactly like C#'s `RuleInverseCompiler._nextState`/
/// `PhonologyRuleCompiler._nextState`), arcs keyed by their FROM state in INSERTION order (C#'s
/// `Dictionary<int, List<Arc>>` — a plain keyed lookup, never iterated by key, so its `HashMap`
/// backing satisfies plan §4.2 the same way `trie.rs`'s memoization maps do). This is intentionally
/// NOT `trie.rs`'s `ArcCollection`-style binary-search reordering (`F1_QUIRK_AUDIT.md` quirk 9) —
/// that quirk is specific to C#'s `ArcCollection`/`FstTemplateAnalyzer`'s morphotactic trie; C#'s
/// own `InversePhonology.AddArc` (`InversePhonology.cs:72-79`) is a plain `List<Arc>.Add`, so this
/// type mirrors that directly: plain append, arcs walked in insertion order.
/// `Clone` (F7, HYBRID_FST_RUST_PLAN.md §7.1 additive-contract-change convention, same as
/// `hc-rules`'s `node_pins`/`node_full_lanes` going `pub`): callers that need to hold TWO compiled
/// rules from the same `compiler::compile` call simultaneously in a chain array (e.g. a two-rule
/// feeding-chain test) need an owned copy of each `Pinv`, not just a shared borrow of the
/// `Vec<CompiledRuleInverse>` they came from. No existing caller's behavior changes.
#[derive(Default, Clone)]
pub struct InversePhonology {
    arcs: rustc_hash::FxHashMap<StateId, Vec<Arc>>,
    accepting: rustc_hash::FxHashSet<StateId>,
    pub start_state: StateId,
}

impl InversePhonology {
    pub fn new() -> Self {
        InversePhonology::default()
    }

    pub fn add_arc(
        &mut self,
        from: StateId,
        surface: Option<Vec<u64>>,
        underlying: Option<Vec<u64>>,
        to: StateId,
    ) {
        self.arcs.entry(from).or_default().push(Arc {
            surface,
            underlying,
            target: to,
        });
    }

    /// C# `AddEpsilon`: a structural epsilon (both sides `None`).
    pub fn add_epsilon(&mut self, from: StateId, to: StateId) {
        self.add_arc(from, None, None, to);
    }

    pub fn set_accepting(&mut self, state: StateId) {
        self.accepting.insert(state);
    }

    #[inline]
    pub fn is_accepting(&self, state: StateId) -> bool {
        self.accepting.contains(&state)
    }

    /// C# `ArcsFrom`: empty slice for a state with no outgoing arcs (never panics on an unknown
    /// state id, matching C#'s `TryGetValue`-else-`Array.Empty` fallback).
    #[inline]
    pub fn arcs_from(&self, state: StateId) -> &[Arc] {
        self.arcs.get(&state).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_kinds_classify_correctly() {
        let sub = Arc {
            surface: Some(vec![1]),
            underlying: Some(vec![2]),
            target: 1,
        };
        assert!(
            !sub.is_epsilon_input() && !sub.is_epsilon_output() && !sub.is_structural_epsilon()
        );

        let restoration = Arc {
            surface: None,
            underlying: Some(vec![2]),
            target: 1,
        };
        assert!(restoration.is_epsilon_input() && !restoration.is_epsilon_output());

        let epenthesis_inverse = Arc {
            surface: Some(vec![1]),
            underlying: None,
            target: 1,
        };
        assert!(!epenthesis_inverse.is_epsilon_input() && epenthesis_inverse.is_epsilon_output());

        let structural = Arc {
            surface: None,
            underlying: None,
            target: 1,
        };
        assert!(structural.is_structural_epsilon());
    }

    #[test]
    fn arcs_from_preserves_insertion_order_not_binary_search_reorder() {
        let mut pinv = InversePhonology::new();
        for i in 0..6u32 {
            pinv.add_arc(0, Some(vec![i as u64]), Some(vec![i as u64]), i + 1);
        }
        let arcs = pinv.arcs_from(0);
        let firsts: Vec<u64> = arcs
            .iter()
            .map(|a| a.surface.as_ref().unwrap()[0])
            .collect();
        assert_eq!(
            firsts,
            vec![0, 1, 2, 3, 4, 5],
            "plain append order, no ArcCollection-style reorder"
        );
    }

    #[test]
    fn unknown_state_yields_empty_arcs() {
        let pinv = InversePhonology::new();
        assert!(pinv.arcs_from(42).is_empty());
    }
}
