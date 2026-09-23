//! Shared 3-assertion gate template (recall via compose, resource envelope, honest failure).
//! `recall_reachable` uses `apply_up`, not `fsm_intersect`; see `docs/research/pg-foma-gate-template-compose-recall.md`.

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

/// One arc per already-tokenized character of `token_string`, used identically on both tapes: a linear identity transducer for one query.
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

/// One arc per (already atomic, possibly multi-character) tag-text symbol: an identity acceptor for one candidate's tag sequence.
pub fn tag_string_fsm(name: &str, tags: &[String]) -> Fsm {
    let mut h = fsm_construct_init(name);
    for (i, t) in tags.iter().enumerate() {
        fsm_construct_add_arc(&mut h, i as i32, (i + 1) as i32, t, t);
    }
    fsm_construct_set_initial(&mut h, 0);
    fsm_construct_set_final(&mut h, tags.len() as i32);
    fsm_construct_done(h)
}

/// Restricts `net`'s lower tape to `surface_tokens`, projects the upper tape, then checks whether
/// `expected_tags` is reachable via `apply_up`, not `fsm_intersect` (docs/research/pg-foma-gate-template-compose-recall.md).
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

/// Asserts `net`'s state/arc counts stay under both caps -- distinct from the typed-error check in `assert_compose_error` below.
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

/// p99 of `samples`, sorted ascending and ceil-indexed (no interpolation) -- deterministic given the same input, not a rigorous estimator.
pub fn p99(mut samples: Vec<Duration>) -> Duration {
    assert!(
        !samples.is_empty(),
        "p99 of an empty sample set is undefined"
    );
    samples.sort();
    let idx = ((samples.len() as f64) * 0.99).ceil() as usize;
    samples[idx.saturating_sub(1).min(samples.len() - 1)]
}

/// Times `f` once per element of `words`, returning the p99 across all calls.
pub fn per_word_p99<T>(words: &[T], mut f: impl FnMut(&T)) -> Duration {
    let mut samples = Vec::with_capacity(words.len());
    for w in words {
        let t0 = Instant::now();
        f(w);
        samples.push(t0.elapsed());
    }
    p99(samples)
}

/// Asserts `result` is `Err` and that `matches` accepts the specific `ComposeError` variant, never a bare "it failed somehow" check.
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

// Resolves generated material back out of the loaded `Grammar` by `xml_key` (the XML `id=` attribute).

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
