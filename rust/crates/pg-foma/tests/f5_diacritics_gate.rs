//! Diacritics gate: a real 100%-recall violation, not a reference-grammar gap. Words containing
//! Latin diacritics (ñ, é, ö, î) got ZERO parses under `--engine=foma` -- not just affixed forms,
//! even bare unaffixed roots -- while the default engine (`pg_parse::Morpher`) analyzed every one
//! correctly, and Cyrillic/ASCII text was unaffected.
//!
//! ## Root cause (confirmed empirically, not just by re-reading code)
//! `pg_grammar::nfd::nfd` NFD-normalizes every surface string `pg_foma::emit` writes into lexc
//! source AND the query word `pg_foma::analyzer` feeds to `apply_up` (mirroring C#'s
//! `Normalize(FormD)`). Under NFD a precomposed accented letter like é (U+00E9, one codepoint)
//! decomposes into TWO codepoints: "e" (U+0065) + COMBINING ACUTE ACCENT (U+0301).
//!
//! - `vendor/foma/src/lexcread.rs::lexc_string_to_tokens` (the lexc SOURCE tokenizer, used when
//!   compiling the emitted lexc into a network) has no combining-mark handling at all: absent a
//!   declared multichar symbol, it assigns one symbol per codepoint, so "café" compiled into TWO
//!   separate arcs for "e" and the combining mark (confirmed by dumping the emitted lexc source
//!   for a hand-built diacritics grammar and hex-inspecting the root entry's bytes: `63 61 66 65
//!   cc 81` = "cafe" + U+0301, five distinct codepoints, no multichar declaration before the fix).
//! - `vendor/foma/src/apply.rs` (the QUERY-string tokenizer used by `apply_up`/`apply_down`) does
//!   the opposite: it unconditionally merges any codepoint with its immediately following run of
//!   `foma::utf8::is_combining` codepoints into ONE token and forces that token's symbol number to
//!   `IDENTITY` -- which only ever matches a network's `?` (`UNKNOWN`) wildcard arc (`apply.rs`
//!   around the `sigmatch_array` construction, doc comment "Merge trailing Unicode combining
//!   characters into one ? (IDENTITY)").
//!
//! A network with two literal, ordinary arcs for "e" and the combining mark (no multichar
//! symbol, no `?` wildcard arc) has NOTHING an `IDENTITY`-tagged query token can match at that
//! position: total non-match, for ANY word containing a base+combining-mark run anywhere in it --
//! independent of affixation (hits bare roots too), and never triggered by Cyrillic (whose letters
//! don't NFD-decompose into base+combining pairs).
//!
//! ## The fix
//! `pg_foma::emit`'s `combining_run_symbols` (see that function's doc in `src/emit.rs`) scans
//! every char-def's `representations_nfd()` for base+combining-mark runs and declares each one as
//! a lexc `Multichar_Symbols` entry. This makes BOTH tokenizers agree: `lexcread.rs`'s
//! `first_mc_prefix` now matches the whole run as ONE compiled arc, and `apply.rs`'s initial
//! sigma-trie walk (which runs BEFORE the combining-merge check) matches that same run as one
//! KNOWN symbol via `lastmatch`, so the merge-check finds nothing left to merge and never
//! downgrades the token to `IDENTITY`.
//!
//! This file is the end-to-end (propose+peel+confirm, i.e. exactly what `pangloss --engine=foma`
//! runs) pin, against the tiny hand-built `tests/fixtures/dia-hc.xml` grammar (4 roots --
//! año/café/göl/kelî -- 2 suffix rules, one optional final slot) captured from the original bug
//! report. White-box unit tests on the mechanism itself (`combining_run_symbols`/
//! `char_is_combining`) live inline in `src/emit.rs`'s test module.

use std::path::{Path, PathBuf};

use pg_foma::analyzer::FomaProposer;
use pg_foma::composite::FomaAnalyzer;
use pg_foma::emit;
use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn load_dia() -> Grammar {
    let path = fixture_path("dia-hc.xml");
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load dia-hc.xml: {e}"))
}

/// Every word from the original bug report: 4 bare roots + their 2 suffixed forms each.
const WORDS: &[&str] = &[
    "año", "años", "café", "cafés", "göl", "göller", "kelî", "kelîs",
];

// -------------------------------------------------------------------------------------------
// (a) emit + compile: must succeed, and the Multichar_Symbols header must actually declare the
//     combining runs (not just compute them and drop them -- see the module doc).
// -------------------------------------------------------------------------------------------

#[test]
fn a_emits_and_compiles_with_combining_runs_declared() {
    let g = load_dia();
    let result = emit::emit(&g);
    assert_eq!(
        result.report.counts.entries, 4,
        "4 lexical entries in the fixture"
    );
    assert_eq!(
        result.report.counts.rules, 2,
        "2 morphological rules (PL, PL2) in the fixture"
    );

    let header_end = result
        .lexc_source
        .find("\nLEXICON")
        .unwrap_or(result.lexc_source.len());
    let header = &result.lexc_source[..header_end];
    for run in ["e\u{301}", "i\u{302}", "n\u{303}", "o\u{308}"] {
        assert!(
            header.contains(run),
            "Multichar_Symbols header must declare the NFD combining run {run:?}; header:\n{header}"
        );
    }
}

// -------------------------------------------------------------------------------------------
// (b) BARE PROPOSE: `FomaProposer::propose` (the thin emit+compile+apply_up wrapper, with no
//     confirm/peel in the loop) must find a candidate for EVERY word, including bare unaffixed
//     roots -- this is the exact symptom from the bug report ("not just the affixed forms, but
//     even bare unaffixed roots get zero parses").
// -------------------------------------------------------------------------------------------

#[test]
fn b_bare_propose_finds_every_diacritic_word() {
    let g = load_dia();
    let mut proposer = FomaProposer::new(&g).expect("dia-hc.xml compiles");
    for &word in WORDS {
        let candidates = proposer.propose(word);
        assert!(
            !candidates.is_empty(),
            "propose({word:?}) returned zero candidates -- the diacritics bug (README/bug report) \
             is back"
        );
    }
}

// -------------------------------------------------------------------------------------------
// (c) FULL PARITY: the composite propose->confirm pipeline (`FomaAnalyzer`, exactly what
//     `pangloss --engine=foma` runs) must reach the SAME confirmed analysis count as the default
//     engine (`Morpher`, the recall oracle every other gate in this crate uses) for every word --
//     this tiny closed-vocabulary grammar admits full parity, not just nonzero recall.
// -------------------------------------------------------------------------------------------

#[test]
fn c_foma_analyzer_matches_engine_exactly() {
    let g = load_dia();
    let mut analyzer = FomaAnalyzer::new(&g).expect("dia-hc.xml compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    for &word in WORDS {
        let engine_outcome = morpher.parse_word_opts(word, &opts);
        let foma_outcome = analyzer.analyze_word(word);

        assert!(
            !engine_outcome.structured.is_empty(),
            "sanity: the default engine itself must analyze {word:?} (fixture bug, not the foma \
             bug under test, if this fails)"
        );
        assert_eq!(
            foma_outcome.confirmed,
            engine_outcome.structured.len(),
            "word {word:?}: foma engine confirmed {} analyses, default engine found {} -- \
             expected full parity on this closed-vocabulary fixture",
            foma_outcome.confirmed,
            engine_outcome.structured.len()
        );

        // The confirmed surface strings must be exactly the queried word (module doc: this is
        // %not just nonzero, but CORRECT%).
        for (_, surface) in &foma_outcome.analyses {
            assert_eq!(
                surface, word,
                "confirmed analysis surface must equal the queried word"
            );
        }
    }
}

// -------------------------------------------------------------------------------------------
// (d) BOUNDARY CASE: a standalone combining-mark char-def (e.g. an autosegmental tone mark
//     modeled as its own grapheme) concatenated right after a DIFFERENT char-def in a root's own
//     surface text -- `emit::combining_run_symbols` only scans WITHIN one char-def's own
//     representation, so this run (spanning the boundary between two DIFFERENT char-defs) was a
//     dormant instance of the exact same bug until `emit::boundary_combining_run_symbols` (see
//     that function's doc in `src/emit.rs`) closed it. `tests/fixtures/boundary-mark-affix-hc.xml`:
//     3 char-defs ("b"/"s" plain, and a standalone mark whose sole representation is COMBINING
//     ACUTE ACCENT alone), one root spelled "b" + the standalone mark's own representation (i.e.
//     authored text "b\u{301}", two codepoints, base then a DIFFERENT char-def's mark -- not one
//     char-def's own decomposition), one optional suffix rule inserting "s" (mirroring
//     `dia-hc.xml`'s PL rule). Separate fixture from `boundary-mark-hc.xml` (used by
//     `src/emit.rs`'s white-box unit tests): that one's char-def table is deliberately minimal (3
//     char-defs, exact-set-asserted); adding this rule's own "s" char-def to it would grow that
//     exact-set assertion for no reason.
// -------------------------------------------------------------------------------------------

fn load_boundary_mark() -> Grammar {
    let path = fixture_path("boundary-mark-affix-hc.xml");
    let xml =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    pg_grammar::load(&xml)
        .unwrap_or_else(|e| panic!("failed to load boundary-mark-affix-hc.xml: {e}"))
}

const BOUNDARY_WORDS: &[&str] = &["b\u{301}", "b\u{301}s"];

#[test]
fn d_boundary_mark_declares_the_cross_char_def_run() {
    let g = load_boundary_mark();
    let result = emit::emit(&g);
    let header_end = result
        .lexc_source
        .find("\nLEXICON")
        .unwrap_or(result.lexc_source.len());
    let header = &result.lexc_source[..header_end];
    assert!(
        header.contains("b\u{301}"),
        "Multichar_Symbols header must declare the cross-char-def boundary run \"b\\u{{301}}\"; \
         header:\n{header}"
    );
}

#[test]
fn e_boundary_mark_foma_analyzer_matches_engine_exactly() {
    let g = load_boundary_mark();
    let mut analyzer = FomaAnalyzer::new(&g).expect("boundary-mark-hc.xml compiles");
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default();

    for &word in BOUNDARY_WORDS {
        let engine_outcome = morpher.parse_word_opts(word, &opts);
        let foma_outcome = analyzer.analyze_word(word);

        assert!(
            !engine_outcome.structured.is_empty(),
            "sanity: the default engine itself must analyze {word:?} (fixture bug, not the foma \
             boundary-mark bug under test, if this fails)"
        );
        assert_eq!(
            foma_outcome.confirmed,
            engine_outcome.structured.len(),
            "word {word:?}: foma engine confirmed {} analyses, default engine found {} -- \
             expected full parity on this closed-vocabulary fixture (the boundary-mark bug \
             reproduces as a silent zero here)",
            foma_outcome.confirmed,
            engine_outcome.structured.len()
        );
        for (_, surface) in &foma_outcome.analyses {
            assert_eq!(
                surface, word,
                "confirmed analysis surface must equal the queried word"
            );
        }
    }
}
