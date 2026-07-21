//! The shared 3-assertion Phase C gate template (`docs/fst-plan/phase-c-generator-design.md` §4):
//! (a) recall via compose, (b) resource envelope, (c) honest failure. Every `phase_c_*.rs` gate
//! builds its own net (the right pipeline differs per construct -- design doc §6) and then calls
//! into this shared module for the actual assertions, so the assertions themselves stay identical
//! across gates.
//!
//! ## (a) Recall via compose -- adapted from the P6/Aweti investigation
//! `linear_identity_fsm`/`tag_string_fsm`/`recall_reachable` adapt the technique
//! `p6_aweti_q4_compose_recall.rs` (a throwaway diagnostic in a sibling worktree, copied here by
//! re-implementing it from reading that file, not by referencing it -- it belongs to a DIFFERENT
//! in-flight change) proved terminating where `apply_up`'s own search over the FULL net is not:
//! fix the composed net's lower tape to exactly one word via `fsm_compose` with a small linear
//! identity transducer (a polynomial-bounded product, `O(|net states| * |word length + 1|)`,
//! independent of the net's own total path count), then project the result's upper tape.
//!
//! **Deviation from q4's own final step, found empirically while building GATE 2 (structural
//! composite / circumfix entries specifically):** q4 finishes with `fsm_intersect` against a
//! `tag_string_fsm` acceptor, then `fsm_isempty`. On a structural-composite entry (whose lexc
//! encoding pairs the WHOLE literal surface span identity-wise -- upper and lower carry the same
//! phoneme characters after the leading tag arcs, unlike the token-space emitters where non-tag
//! positions are epsilon-upper) the PROJECTED upper net still contains one arc per phoneme
//! position (epsilon-labelled in effect, but a real forward-advancing transition, not removed by
//! `fsm_minimize` alone) between/after the real tag arcs. `fsm_intersect`'s synchronized product
//! does not appear to epsilon-close across these before pairing states with `tag_string_fsm`'s
//! epsilon-free path, so the intersection comes back EMPTY even though the tag sequence is
//! genuinely reachable -- verified directly: `apply_init(&upper_net).up(&concatenated_tag_string)`
//! (a proper epsilon-closing search) DOES find it, on this exact same tiny projected net, for every
//! case `fsm_intersect` missed. This is consistent with this crate's own documented experience of
//! this vendored foma's epsilon handling being a real hazard, not folklore (`pg-foma/src/gate.rs`'s
//! own module doc catalogs three unrelated epsilon/flag surprises in this same crate version).
//!
//! [`recall_reachable`] therefore finishes with an `apply_up` search -- but critically, ONLY on the
//! already-word-restricted `upper` net (a handful of states, by construction: the whole point of
//! the compose-restriction step), never on the full, potentially enormous composed net apply_up's
//! own search is unsafe against. `tag_string_fsm` is kept (still a correct, reusable acceptor
//! builder) for any future gate that wants a pure-intersect check on a net shaped like q4's own
//! (token-space, no structural composites) where the epsilon issue above does not arise.
//!
//! Trade-off (design doc §4a, stated plainly): this proves REACHABILITY of one expected tag
//! sequence for one surface string, not `FomaProposer` candidate-set fidelity (the actual
//! `apply_up`-based proposer could enumerate that same reachable path plus spurious ones, or fail
//! to terminate trying). Gates about `FomaProposer` behavior itself should call
//! `FomaProposer::propose` directly instead; gates about "does the net I built even relate this
//! surface string to this analysis" (both gates here) want this.
//!
//! ## (b) Resource envelope
//! `Fsm.statecount`/`arccount` after building the gate's own net, `Instant`-timed compose+load,
//! and per-word p99 timing over an oracle word list -- the same measures
//! `docs/fst-plan/phase-b-compose-budget-design.md`'s own `ComposeBudget` checks, but read here
//! directly off the `Fsm` rather than through that budget's checked wrappers (a gate wants to
//! ASSERT specific numbers stay small, not merely that they didn't exceed a cap).
//!
//! ## (c) Honest failure
//! [`assert_compose_error`]: given a `Result<T, pg_foma::compose_budget::ComposeError>` from a
//! call made under a deliberately tiny `ComposeBudget::with_caps` (never an env var -- design doc
//! §6: "explicit-caps constructors, never env vars", mirroring every existing `ComposeBudget` test
//! in this crate), assert it is the SPECIFIC expected variant.

#![allow(dead_code)] // not every gate uses every helper here

use std::time::{Duration, Instant};

use foma::apply::apply_init;
use foma::constructions::fsm_compose;
use foma::dynarray::{
    fsm_construct_add_arc, fsm_construct_done, fsm_construct_init, fsm_construct_set_final,
    fsm_construct_set_initial,
};
use foma::extract::fsm_upper;
use foma::minimize::fsm_minimize;
use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_foma::compose_budget::ComposeError;
use pg_grammar::model::{Grammar, LexEntryId, MRuleId, MorphRuleDef};

// =================================================================================================
// (a) Recall via compose.
// =================================================================================================

/// One arc per character of `token_string` (already single-codepoint tokens -- `pg_foma::replace::
/// SegAlphabet`'s PUA scheme, or any other already-tokenized string), used identically on both
/// tapes: a linear identity transducer for one query.
pub fn linear_identity_fsm(name: &str, token_string: &str) -> Fsm {
    let mut h = fsm_construct_init(name);
    let chars: Vec<char> = token_string.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let sym = c.to_string();
        fsm_construct_add_arc(&mut h, i as i32, (i + 1) as i32, &sym, &sym);
    }
    fsm_construct_set_initial(&mut h, 0);
    fsm_construct_set_final(&mut h, chars.len() as i32);
    fsm_construct_done(h)
}

/// One arc per (already atomic, possibly multi-character) tag-text symbol -- a linear acceptor
/// (identity transducer) for one candidate analysis's tag sequence, matching how the composed
/// net's own `Multichar_Symbols` declares each tag as one atomic arc symbol.
pub fn tag_string_fsm(name: &str, tags: &[String]) -> Fsm {
    let mut h = fsm_construct_init(name);
    for (i, t) in tags.iter().enumerate() {
        fsm_construct_add_arc(&mut h, i as i32, (i + 1) as i32, t, t);
    }
    fsm_construct_set_initial(&mut h, 0);
    fsm_construct_set_final(&mut h, tags.len() as i32);
    fsm_construct_done(h)
}

/// The compose-recall technique itself (module doc (a)): fix `net`'s lower (surface) tape to
/// exactly `surface_tokens`, project the result's upper tape, and check whether `expected_tags`
/// (in order, concatenated into one string) is a reachable path through it -- via a bounded
/// `apply_up` search on that (by construction, tiny) restricted-and-projected net, NOT
/// `fsm_intersect` (module doc's own "deviation from q4" section explains why intersect silently
/// misses a reachable path on a structural-composite entry's projected net, verified empirically).
/// `net` is cloned (composition consumes its operand) so a gate can call this repeatedly against
/// the same built net.
pub fn recall_reachable(net: &Fsm, surface_tokens: &str, expected_tags: &[String]) -> bool {
    let opts = FomaOptions::default();
    let word_fsm = linear_identity_fsm("word", surface_tokens);
    let restricted = fsm_compose(&opts, net.clone(), word_fsm);
    let restricted = fsm_minimize(&opts, restricted);
    let upper = fsm_upper(restricted);
    let upper = fsm_minimize(&opts, upper);
    let expected: String = expected_tags.concat();
    let mut handle = apply_init(&upper);
    handle.up(&expected).any(|r| r == expected)
}

// =================================================================================================
// (b) Resource envelope.
// =================================================================================================

/// Asserts `net`'s size stays under both caps -- a gate's own "this stayed small" claim, not a
/// `ComposeBudget` typed-error check (that's (c), for the deliberately-over-budget variant).
pub fn assert_net_size_within(net: &Fsm, state_cap: i32, arc_cap: i32) {
    assert!(
        net.statecount <= state_cap,
        "net has {} states, expected <= {state_cap}",
        net.statecount
    );
    assert!(
        net.arccount <= arc_cap,
        "net has {} arcs, expected <= {arc_cap}",
        net.arccount
    );
}

/// p99 (99th percentile) of `samples`, sorted ascending -- deterministic given the same input
/// (no interpolation, just the ceil-indexed sample, matching the "sub-10ms trip-wire" framing
/// design doc §4b uses; not a statistically rigorous percentile estimator, just a stable,
/// reproducible one for a tiny gate word list).
pub fn p99(mut samples: Vec<Duration>) -> Duration {
    assert!(
        !samples.is_empty(),
        "p99 of an empty sample set is undefined"
    );
    samples.sort();
    let idx = ((samples.len() as f64) * 0.99).ceil() as usize;
    samples[idx.saturating_sub(1).min(samples.len() - 1)]
}

/// Times `f` once per element of `words`, returning the p99 across all calls -- the per-word
/// timing half of (b). `f` should do the SAME "one word -> reachable?" work each gate's own recall
/// assertion already does, so this number is directly comparable to (a)'s own per-word cost.
pub fn per_word_p99<T>(words: &[T], mut f: impl FnMut(&T) -> ()) -> Duration {
    let mut samples = Vec::with_capacity(words.len());
    for w in words {
        let t0 = Instant::now();
        f(w);
        samples.push(t0.elapsed());
    }
    p99(samples)
}

// =================================================================================================
// (c) Honest failure.
// =================================================================================================

/// Asserts `result` is `Err` and that `matches` accepts the specific [`ComposeError`] variant --
/// never a bare "it failed somehow" check (module doc (c): the whole point of a typed budget error
/// is that a gate can assert WHICH one).
pub fn assert_compose_error<T: std::fmt::Debug>(
    result: Result<T, ComposeError>,
    matches: impl FnOnce(&ComposeError) -> bool,
    what: &str,
) {
    match result {
        Ok(v) => panic!("expected {what}, but the call succeeded: {v:?}"),
        Err(e) if matches(&e) => {}
        Err(e) => panic!("expected {what}, got a different ComposeError: {e}"),
    }
}

// =================================================================================================
// Small xml-id lookup helpers (every gate needs to resolve its own generated material back out of
// the loaded `Grammar` -- `pg-grammar/src/load.rs`'s own convention: EVERY morpheme-bearing
// element's `xml_key` is its own `id=` attribute, `pg-foma/src/morphotactics.rs`'s
// `mrule_id_of`/`entry_id_of` test helpers are the precedent for this exact lookup shape).
// =================================================================================================

pub fn entry_id_of(g: &Grammar, xml_id: &str) -> LexEntryId {
    LexEntryId(
        g.entries
            .iter()
            .position(|e| g.morphemes[e.morpheme.0 as usize].xml_key == xml_id)
            .unwrap_or_else(|| panic!("no entry with xml id {xml_id:?}")) as u32,
    )
}

pub fn mrule_id_of(g: &Grammar, xml_id: &str) -> MRuleId {
    for (i, r) in g.mrules.iter().enumerate() {
        let m = match r {
            MorphRuleDef::AffixProcess(d) => d.morpheme,
            MorphRuleDef::Realizational(d) => d.morpheme,
            MorphRuleDef::Compounding(_) => continue,
        };
        if g.morphemes[m.0 as usize].xml_key == xml_id {
            return MRuleId(i as u32);
        }
    }
    panic!("no mrule with xml id {xml_id:?}");
}
