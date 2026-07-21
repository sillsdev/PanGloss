//! PK1 (design `docs/superpowers/specs/2026-07-15-fst-precision-knob-design.md` §8 sequencing
//! item (1), §6 "Testing"): the recall-invariance harness for the FST precision knob's step-1
//! `AllFlags` preset (`pg_foma::precision`) — THE key property (§0/§6): the knob is
//! performance-only, so the composite propose→confirm path must reach IDENTICAL confirmed
//! analyses at [`PrecisionConfig::Strip`] (default) and [`PrecisionConfig::AllFlags`], and the
//! raw candidate set must only ever SHRINK (never gain a candidate) between them.
//!
//! ## Why this file drives `apply_up`/peel/confirm directly instead of `FomaAnalyzer`
//! `pg_foma::composite::FomaAnalyzer::new`/`pg_foma::analyzer::FomaProposer::new` both hardcode
//! `emit::emit(g)` (`PrecisionConfig::Strip`) internally, and this task's ownership is scoped to
//! `emit.rs`/`precision.rs`/`lib.rs`/new tests only — `analyzer.rs`/`composite.rs` are left
//! untouched. So this file reimplements the SAME propose→confirm composite
//! (`crate::composite::FomaAnalyzer::analyze_word`'s own doc names the pipeline: `propose(word)`
//! UNION `peel_candidates(word, propose)`, deduped by `(morphemes, root_index)`, then
//! `confirm_batch`) using only PUBLIC building blocks: `pg_foma::emit::emit_with_precision` to get
//! a lexc source for either preset, the `foma` crate directly to compile+`apply_up` it (mirroring
//! `FomaProposer::propose`'s own logic via `pg_foma::tags::{decode_path, to_candidates}`), and
//! `pg_foma::peel`/`pg_foma::confirm`'s already-`pub` pieces (`ReduplicationPeeler`,
//! `build_morpheme_owners`, `confirm_batch`) verbatim — the SAME peel/confirm machinery every
//! other gate in this crate exercises, just re-plumbed to accept either compiled network.
//!
//! ## Non-vacuity (guards against a knob that "passes" by doing nothing — advisor guidance)
//! Indonesian declares zero environment constraints at all (`pg_grammar` XML has no
//! `RequiredEnvironments`/`ExcludedEnvironments` element), so `AllFlags` there is CORRECTLY
//! byte-identical to `Strip` — proves nothing about the mechanism biting. Sena DOES have real
//! coverable instances (`pg_foma::precision`'s own catalog test:
//! `precision::tests::sena_catalog_finds_the_expected_left_literal_instances`). This file asserts,
//! for Sena specifically: (a) the `AllFlags` lexc source actually contains flag-diacritic symbols;
//! (b) at least one Sena corpus word's raw candidate set is a STRICT subset under `AllFlags`
//! (fewer candidates than `Strip`, not just an equal-size coincidence) — i.e. the flags visibly
//! prune something, not merely compile to a no-op.
//!
//! ## Test-timing policy (revised 2026-07-17)
//! The default local `cargo test --workspace --release` run must stay under ~60s and must not
//! depend on the gitignored real-language corpus fixtures (`samples/data/*`) at all — every test
//! in this file loads one, so ALL FOUR (including the otherwise-fast
//! `emit_and_emit_with_precision_strip_are_the_same_call`) are unconditionally
//! `#[ignore = "..."]`d, replacing the old `cfg_attr(debug_assertions, ...)` debug-only ignore on
//! the three engine-oracle tests. `load_grammar`'s existing `Option`-returning self-skip already
//! keeps `--include-ignored` runs green where the fixture is absent (CI); the `#[ignore]` is what
//! keeps them out of the default run at all, run speed aside. Run the full set locally with
//! `cargo test -p pg-foma --release --test pk1_precision_recall_invariance -- --include-ignored`.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use foma::apply::apply_init;
use foma::lexcread::fsm_lexc_parse_string;
use foma::options::FomaOptions;
use foma::types::Fsm;

use pg_foma::confirm::{self, MorphemeOwner};
use pg_foma::emit::{self, EmitResult};
use pg_foma::peel::ReduplicationPeeler;
use pg_foma::precision::PrecisionConfig;
use pg_foma::tags::{self, Candidate};
use pg_grammar::model::Grammar;
use pg_parse::Morpher;

fn sample_path(name: &str) -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../../../samples/data").join(name)
}

fn load_grammar(name: &str) -> Option<Grammar> {
    let path = sample_path(name);
    let xml = std::fs::read_to_string(&path).ok()?;
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {name}: {e}")))
}

fn read_words(name: &str) -> Vec<String> {
    let path = sample_path(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Emit `g` under `precision` and foma-compile it — panics loudly (not gracefully) on a compile
/// failure, since that itself is exactly the kind of "AllFlags broke the network" finding this
/// harness exists to catch.
fn compile(g: &Grammar, precision: PrecisionConfig) -> (Fsm, EmitResult) {
    let result = emit::emit_with_precision(g, precision);
    let opts = FomaOptions::default();
    let net = fsm_lexc_parse_string(&opts, None, &result.lexc_source).unwrap_or_else(|| {
        panic!(
            "{precision:?}: foma failed to compile the emitted lexc source (report: {} uncovered, \
             tier {:?}) -- first 2000 chars of source:\n{}",
            result.report.uncovered.len(),
            result.report.tier,
            &result.lexc_source.chars().take(2000).collect::<String>()
        )
    });
    (net, result)
}

/// Mirrors `pg_foma::analyzer::FomaProposer::propose` exactly (same NFD normalization, same
/// `decode_path`/`to_candidates`/dedup-by-`(morphemes, root_index)`-preserving-first-seen-order
/// convention) but against a caller-supplied network instead of an owned `FomaProposer`, so the
/// SAME adapter works for both the `Strip` and `AllFlags` compiled nets.
fn propose(net: &Fsm, word: &str) -> Vec<Candidate> {
    let normalized = pg_grammar::nfd::nfd(word);
    let mut handle = apply_init(net);
    let mut seen: HashSet<(Vec<u32>, i32)> = HashSet::new();
    let mut out = Vec::new();
    for s in handle.up(&normalized) {
        let Some(path) = tags::decode_path(&s) else {
            continue;
        };
        for c in tags::to_candidates(&path) {
            let key: (Vec<u32>, i32) = (c.morphemes.iter().map(|m| m.0).collect(), c.root_index);
            if seen.insert(key) {
                out.push(c);
            }
        }
    }
    out
}

/// `(morphemes, root_index)` key set for a candidate slice -- the identity `crate::confirm`'s
/// `analyses_match`/this harness's subset and equality checks are keyed on throughout (plan §2:
/// "Allomorph IDs are NOT part of candidate identity").
fn candidate_keys(cands: &[Candidate]) -> BTreeSet<(Vec<u32>, i32)> {
    cands
        .iter()
        .map(|c| (c.morphemes.iter().map(|m| m.0).collect(), c.root_index))
        .collect()
}

/// `propose(word)` UNION `peeler.peel_candidates(word, propose)`, deduped by `(morphemes,
/// root_index)` -- exactly `FomaAnalyzer::analyze_word`'s own candidate-assembly step (that
/// module's doc), reimplemented here against a caller-supplied net (module doc's "why this file...").
fn propose_and_peel(
    net: &Fsm,
    g: &Grammar,
    peeler: &ReduplicationPeeler,
    word: &str,
) -> Vec<Candidate> {
    let mut candidates = propose(net, word);
    let peeled = peeler.peel_candidates(g, word, &mut |r: &str| propose(net, r));
    for c in peeled {
        let already = candidates.iter().any(|existing| {
            existing.root_index == c.root_index && existing.morphemes == c.morphemes
        });
        if !already {
            candidates.push(c);
        }
    }
    candidates
}

/// `confirm_batch` + flatten to the `(morpheme_ids, root_morpheme_index)` multiset key
/// `f4_composite_gate.rs`'s own `structured_multiset` uses, for comparing against the OTHER
/// preset's confirmed set (not against the full engine here -- that parity is every OTHER gate's
/// job; this harness's only question is Strip-vs-AllFlags agreement).
fn confirmed_multiset(
    g: &Grammar,
    owners: &[Option<MorphemeOwner>],
    morpher: &Morpher,
    candidates: &[Candidate],
    word: &str,
) -> Vec<(Vec<u32>, i32)> {
    let mut m: Vec<(Vec<u32>, i32)> = Vec::new();
    for bucket in confirm::confirm_batch(g, owners, morpher, candidates, word) {
        for (wa, _join, _surface) in bucket {
            m.push((wa.morpheme_ids.clone(), wa.root_morpheme_index));
        }
    }
    m.sort();
    m
}

/// One grammar's precision-recall-invariance run: builds both compiled nets ONCE, then for every
/// word in `words` asserts (1) upward-only candidates (`AllFlags` keys ⊆ `Strip` keys) and (2)
/// identical confirmed multisets. Returns whether AT LEAST ONE word saw a STRICTLY smaller
/// `AllFlags` candidate set (module doc, "Non-vacuity") -- callers decide whether that's required.
fn run_invariance(g: &Grammar, words: &[String]) -> bool {
    let (net_strip, result_strip) = compile(g, PrecisionConfig::Strip);
    let (net_allflags, result_allflags) = compile(g, PrecisionConfig::AllFlags);

    // Sanity: uncovered-construct counts must be identical between presets (the knob never adds or
    // removes what's REPRESENTABLE, only how it's compiled) -- a divergence here would mean
    // `PrecisionEmit`'s wiring accidentally perturbed something outside its own gate logic.
    assert_eq!(
        result_strip.report.uncovered.len(),
        result_allflags.report.uncovered.len(),
        "Strip vs AllFlags must report the same uncovered-construct COUNT (the knob is emission-\
         strategy-only, never a coverage change)"
    );

    let peeler = ReduplicationPeeler::new(g);
    let owners = confirm::build_morpheme_owners(g);
    let morpher = Morpher::new(g, usize::MAX);

    let mut saw_strict_shrink = false;
    let mut checked = 0usize;
    for word in words {
        let cands_strip = propose_and_peel(&net_strip, g, &peeler, word);
        let cands_allflags = propose_and_peel(&net_allflags, g, &peeler, word);

        let keys_strip = candidate_keys(&cands_strip);
        let keys_allflags = candidate_keys(&cands_allflags);
        assert!(
            keys_allflags.is_subset(&keys_strip),
            "{word:?}: AllFlags candidate set must be a SUBSET of Strip's (flags only REMOVE \
             false candidates) -- AllFlags-only keys: {:?}",
            keys_allflags.difference(&keys_strip).collect::<Vec<_>>()
        );
        if keys_allflags.len() < keys_strip.len() {
            saw_strict_shrink = true;
        }

        let confirmed_strip = confirmed_multiset(g, &owners, &morpher, &cands_strip, word);
        let confirmed_allflags = confirmed_multiset(g, &owners, &morpher, &cands_allflags, word);
        assert_eq!(
            confirmed_strip,
            confirmed_allflags,
            "{word:?}: CONFIRMED analyses must be IDENTICAL between Strip and AllFlags -- the \
             knob must never change which analyses come out, only network/candidate-count \
             performance. Strip candidates={}, AllFlags candidates={}",
            cands_strip.len(),
            cands_allflags.len()
        );
        checked += 1;
    }
    println!("checked {checked} words; saw_strict_candidate_shrink={saw_strict_shrink}");
    saw_strict_shrink
}

// -------------------------------------------------------------------------------------------
// Sena: real coverable ENVIRONMENT instances exist (root-side /ma_//na_, rule-side /mb_) --
// the harness's main, non-vacuous target.
// -------------------------------------------------------------------------------------------

/// How many Sena corpus words this gate scans. Each word here runs propose+peel+confirm FOUR
/// times over (Strip candidates, AllFlags candidates, Strip confirm, AllFlags confirm), so this
/// stays well under `f4_composite_gate.rs`'s own "40 words" mini-parity budget.
const SENA_SCAN_WORDS: usize = 30;

/// Fast focused regression for the specific word ("miseru") whose recall the FIRST version of the
/// `AllFlags` set-side broke (module doc / `precision.rs` findings: a missing `@P@` value zeroed
/// every `@R@` gate, and a whole-literal-only `ends_with` under-set boundary-spanning contexts).
/// Compiles the two Sena nets once (~1 min in release) and confirms a HANDFUL of words at both
/// presets — orders of magnitude faster than [`sena_precision_recall_invariance`]'s full-corpus
/// scan, so the fix can be iterated without paying the whole-corpus engine-oracle cost each time.
/// Asserts only the confirmed-set identity (recall) here; non-vacuity (strict shrink) is the
/// full-corpus test's job.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_miseru_focused_recall_invariance() {
    let Some(g) = load_grammar("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let words: Vec<String> = ["miseru", "mbali", "kucita", "kufamba", "musandilesera"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    // `run_invariance`'s per-word assertions (AllFlags ⊆ Strip candidates; Strip == AllFlags
    // confirmed multiset) are exactly the recall property; a panic here pins the regression.
    let saw_strict_shrink = run_invariance(&g, &words);
    println!("focused miseru run: saw_strict_candidate_shrink={saw_strict_shrink}");
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_precision_recall_invariance() {
    let Some(g) = load_grammar("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };

    // Non-vacuity guard (a): the AllFlags source must actually contain flag-diacritic symbols for
    // this grammar -- if this ever fails, the catalog stopped finding Sena's coverable instances
    // (a regression in `precision::classify`, not a property of Sena itself).
    let (_, result_allflags) = compile(&g, PrecisionConfig::AllFlags);
    // NB: flag NAMES are dot-free (`precision::flag_id` -- `@R.ENV7.y@`, never `@R.ENV.0007@`;
    // see that module's flag_check/zero-digit findings), so match `@R.ENV` without a trailing dot.
    assert!(
        result_allflags.lexc_source.contains("@R.ENV"),
        "Sena's AllFlags lexc source must contain at least one owner-side ENV require flag \
         (exclude is declined this step, so only @R@ is ever emitted)"
    );
    assert!(
        result_allflags.lexc_source.contains("@P.ENV"),
        "Sena's AllFlags lexc source must contain at least one ENV flag set"
    );

    let words: Vec<String> = read_words("sena-words.txt")
        .into_iter()
        .take(SENA_SCAN_WORDS)
        .collect();
    assert!(
        words.len() >= SENA_SCAN_WORDS.min(10),
        "expected Sena corpus words to scan"
    );

    let saw_strict_shrink = run_invariance(&g, &words);
    // Non-vacuity guard (b): among the scanned words, AllFlags must have actually pruned at least
    // one raw candidate somewhere -- otherwise every assertion above passed vacuously (the flags
    // compiled but never bit). If this fails, widen `SENA_SCAN_WORDS` or re-check that the
    // coverable instances' literals actually occur adjacent to their gated allomorphs in real
    // corpus words before concluding the mechanism itself is inert.
    assert!(
        saw_strict_shrink,
        "expected AllFlags to strictly shrink the raw candidate set for at least one of the first \
         {SENA_SCAN_WORDS} Sena corpus words -- otherwise this harness cannot distinguish a \
         working knob from a no-op one"
    );
}

// -------------------------------------------------------------------------------------------
// Indonesian: zero environment constraints declared at all -- AllFlags is CORRECTLY
// byte-identical to Strip here (`precision::tests::indonesian_catalog_is_empty`), so this leg
// only asserts the identity/subset properties hold (trivially), not non-vacuity.
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn indonesian_precision_recall_invariance() {
    let Some(g) = load_grammar("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };

    let (net_strip, result_strip) = compile(&g, PrecisionConfig::Strip);
    let (_net_allflags, result_allflags) = compile(&g, PrecisionConfig::AllFlags);
    assert_eq!(
        result_strip.lexc_source, result_allflags.lexc_source,
        "Indonesian declares zero environment constraints -- AllFlags must be BYTE-IDENTICAL to \
         Strip here (nothing for PrecisionEmit to gate)"
    );
    drop(net_strip);

    let words = read_words("indonesian-words.txt");
    assert!(
        words.len() >= 100,
        "expected most of the Indonesian corpus, got {}",
        words.len()
    );
    let _saw_strict_shrink = run_invariance(&g, &words);
}

// -------------------------------------------------------------------------------------------
// The Strip default's byte-identity property, in one direct assertion (no engine oracle needed --
// fast, runs in BOTH debug and release).
// -------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/{sena,indonesian}-hc.xml); run with --include-ignored"]
fn emit_and_emit_with_precision_strip_are_the_same_call() {
    // `emit::emit` is defined as `emit_with_precision(g, PrecisionConfig::Strip)` -- this is a
    // structural/documentation-level assertion (loaded once, cheaply, on whichever sample grammar
    // is present) that the wrapper hasn't drifted from that contract.
    for name in ["sena-hc.xml", "indonesian-hc.xml"] {
        let Some(g) = load_grammar(name) else {
            eprintln!("skipping {name}: not present on disk");
            continue;
        };
        let a = emit::emit(&g).lexc_source;
        let b = emit::emit_with_precision(&g, PrecisionConfig::Strip).lexc_source;
        assert_eq!(
            a, b,
            "{name}: emit() must equal emit_with_precision(_, Strip) byte-for-byte"
        );
    }
}
