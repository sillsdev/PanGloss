//! End-to-end pin (propose+peel+confirm) for the NFD combining-mark recall bug, against a tiny hand-built grammar.
//! See `docs/research/pg-foma-emit-design-notes.md`'s "Diacritics" section for the root cause and fix.

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

// (a) emit + compile must succeed, and the Multichar_Symbols header must actually declare the combining runs.

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

// (b) Bare propose (`FomaProposer::propose`, no confirm/peel) must find a candidate for every word, including bare unaffixed roots.

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

// (c) Full parity: `FomaAnalyzer`'s confirmed analysis count must equal the default engine's (`Morpher`) count for every word, not merely be nonzero.

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

        // Confirmed surface strings must exactly equal the queried word -- not just nonzero recall, but correct recall.
        for (_, surface) in &foma_outcome.analyses {
            assert_eq!(
                surface, word,
                "confirmed analysis surface must equal the queried word"
            );
        }
    }
}

// (d) Boundary case: a standalone combining-mark char-def concatenated after a different char-def was a dormant instance of the same bug, closed by `emit::boundary_combining_run_symbols`.

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
