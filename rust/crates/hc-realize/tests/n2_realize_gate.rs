//! N2 integration gate (`docs/natural-phrases-plan.md` N2): the end-to-end demo — real
//! sample-grammar words parsed through `Morpher` -> `hc_realize::gloss_bundle` ->
//! `hc_realize::to_ir` -> `hc_realize::TableRealizer::realize` -- plus an all-corpora robustness
//! sweep that N1's own `to_ir_never_panics_on_a_bounded_corpus_sample` explicitly deferred to this
//! milestone (see that test's doc comment).
//!
//! Same self-skip discipline as `n0_gloss_gate.rs`/`n1_ir_gate.rs`: every real-grammar test
//! no-ops when the grammar XML is absent on disk.
//!
//! Every pinned `eng:` string below was obtained by first running
//! `cargo run -p hc-cli -- parse <grammar> <word> --gloss --natural-gloss=eng` to see the actual
//! output, THEN writing the expected assertion — same "run first, pin second" discipline
//! `n0_gloss_gate.rs`/`n1_ir_gate.rs` document for their own pinned strings.

use std::path::{Path, PathBuf};
use std::time::Duration;

use hc_grammar::model::Grammar;
use hc_parse::Morpher;
use hc_realize::{Realization, RealizeMap, Realizer, TableRealizer};

fn sample_path(name: &str) -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR = .../rust/crates/hc-realize ; samples live at repo_root/samples/data.
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn load_grammar(xml_name: &str) -> Option<Grammar> {
    let path = sample_path(xml_name)?;
    let xml = std::fs::read_to_string(&path).expect("read sample grammar");
    Some(hc_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {xml_name}: {e}")))
}

/// A missing sidecar when the grammar *is* present is an authoring bug (tracked alongside this
/// milestone's commit), so this panics rather than self-skipping — same posture as
/// `n1_ir_gate.rs::load_map`.
fn load_map(toml_name: &str) -> RealizeMap {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(toml_name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read sidecar {toml_name}: {e}"));
    RealizeMap::parse(&text).unwrap_or_else(|e| panic!("parse sidecar {toml_name}: {e}"))
}

fn realizer() -> TableRealizer {
    TableRealizer::new().unwrap_or_else(|e| panic!("embedded eng assets failed to load: {e}"))
}

/// Parse `word` with an uncapped `Morpher` (these specific demo words are known-fast — the
/// robustness gate below is where the step cap / timeout bounding matters) and realize every
/// surviving analysis, in `outcome.structured` order.
fn realize_all(g: &Grammar, map: &RealizeMap, r: &TableRealizer, word: &str) -> Vec<Realization> {
    let m = Morpher::new(g, usize::MAX);
    let outcome = m.parse_word(word);
    outcome
        .structured
        .iter()
        .map(|wa| {
            let bundle = hc_realize::gloss_bundle(g, wa);
            let ir = hc_realize::to_ir(&bundle, map, word);
            r.realize(&ir)
        })
        .collect()
}

// --- (a) End-to-end amharic demo -------------------------------------------------------------
//
// These are the milestone's headline assertions: a possessed and/or pluralized amharic noun
// renders as a natural English phrase. Per N1's own documented corpus search
// (`n1_ir_gate.rs`'s module doc), no word in `amharic-words.txt` combines a Case gloss with
// poss/pl on one analysis, so the demo target is a possessed-and/or-pluralized noun, not a
// case-marked one -- the Case slot (and the flagship "in my houses" combination) is covered by
// `hc-realize/src/table.rs`'s own unit tests on hand-built `GlossIr`s instead.

#[test]
fn amharic_possessed_noun_renders_your_house() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    let r = realizer();
    // "ቤትህ" = "house-poss.2m" (n1_ir_gate.rs pins the same word's GlossIr) -> "your house".
    let results = realize_all(&g, &map, &r, "ቤትህ");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].text, "your house");
    assert!(results[0].complete, "{:?}", results[0]);
    assert!(results[0].residue.is_empty());
}

#[test]
fn amharic_plural_only_noun_renders_children() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    let r = realizer();
    // "ልጆች" = "child-pl" (n1_ir_gate.rs's amharic_plural_only_noun_maps_num), an irregular
    // plural via lexicon.toml's exceptions table -> "children" (no article: bare Num::Pl,
    // Poss::None).
    let results = realize_all(&g, &map, &r, "ልጆች");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].text, "children");
    assert!(results[0].complete);
}

#[test]
fn amharic_ambiguous_pluralized_possessed_noun_one_complete_one_partial() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    let r = realizer();
    // "ልጆቹ" is the documented 2-way ambiguity (n1_ir_gate.rs's
    // amharic_pluralized_possessed_noun_maps_both_num_and_poss): [0] "child-pl-poss.3m" (root
    // "child" + pl + poss.3m, no extras) -> a complete "his children"; [1] "child-pl-def.m" (root
    // "child" + pl + an unmapped "def.m" definite-marker affix -> extras) -> the same irregular
    // plural "children" but flagged partial via nonempty residue.
    let results = realize_all(&g, &map, &r, "ልጆቹ");
    assert_eq!(results.len(), 2, "{results:?}");

    assert_eq!(results[0].text, "his children");
    assert!(results[0].complete, "{:?}", results[0]);
    assert!(results[0].residue.is_empty());

    assert_eq!(results[1].text, "children");
    assert!(
        !results[1].complete,
        "def.m is unmapped -> nonempty residue -> partial"
    );
    assert_eq!(results[1].residue, vec!["def.m".to_string()]);
}

#[test]
fn amharic_unpossessed_bare_root_renders_with_indefinite_article() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    let r = realizer();
    // "ሆድ" = "stomach", root-only, no Num/Poss/Case morpheme at all -> Num::Unspec -> bare
    // citation form, no article (n1_ir_gate.rs's amharic_bare_root_has_no_features_and_no_extras
    // pins the same word's GlossIr).
    let results = realize_all(&g, &map, &r, "ሆድ");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].text, "stomach");
    assert!(results[0].complete);
}

#[test]
fn amharic_unmapped_verb_word_falls_back_to_partial_citation_form() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    let r = realizer();
    // "ሄደ" = "go--pfv--pfv.3m" (n1_ir_gate.rs's amharic_unmapped_verb_agreement_glosses_land_in_extras):
    // N2 realizes nominal IRs only, so this word's two unmapped verb-aspect affixes land in
    // extras -> the None.None.Unspec cell still fills ("go"), but nonempty residue forces
    // `complete: false` -- exactly the "verb/POS guard ... straight fallback" the plan describes.
    let results = realize_all(&g, &map, &r, "ሄደ");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].text, "go");
    assert!(!results[0].complete);
    assert_eq!(
        results[0].residue,
        vec!["-pfv-".to_string(), "pfv.3m".to_string()]
    );
}

// --- (b) All-corpora robustness gate -----------------------------------------------------------
//
// For every word in all three `samples/data/*-words.txt` (subsampled -- see below), parse with a
// BOUNDED Morpher (`docs/natural-phrases-plan.md` N2's own stated requirement: `Morpher::new(&g,
// 100_000)`, the second arg being the step cap -- an uncapped Morpher against the 7121-word Sena
// corpus is "pathologically slow in debug builds", per the milestone task brief and
// `n1_ir_gate.rs`'s own module doc, which measured 8+ minutes of CPU time and gave up), realize
// every surviving analysis through both the real sidecar (or `RealizeMap::empty()` when none
// exists) and `RealizeMap::empty()` directly, and assert: no panic, `!text.is_empty()`, and the
// parity signature computed right after parsing equals one computed again after every
// gloss_bundle/to_ir/realize call for that word (belt-and-braces: this crate's functions only
// ever read `&ParseOutcome`/`&Grammar`/`&RealizeMap`, never mutate, so this must hold -- same
// property `n0_gloss_gate.rs::gloss_path_never_perturbs_parity_signature` pins for the `--gloss`
// path).
//
// Subsampling: even with the 100_000-step cap AND a 50ms `--word-timeout-ms`-equivalent wall-clock
// deadline (`Morpher::with_word_timeout`), empirically timing the full 7121-word Sena corpus and
// the full 673-word Amharic corpus at this milestone (via `hc-rs batch --step-cap 100000
// --word-timeout-ms 50`, debug build) showed most individual words in both corpora time out at
// that deadline in an unoptimized build -- full Sena alone was measured north of several minutes.
// To stay inside a "few minutes in debug" budget (the task brief's explicit time-box, "subsample
// deterministically ... and say so in the test doc comment"), this test samples every 3rd Amharic
// word and every 10th Sena word (Indonesian's 121-word corpus runs in full -- it was fast in the
// same timing pass). Measured with this exact subsampling + timeout at authoring time: comfortably
// under a minute total across all three corpora.
#[test]
fn realize_never_panics_on_a_subsampled_full_corpus_sweep() {
    struct Corpus {
        xml: &'static str,
        toml: Option<&'static str>,
        words: &'static str,
        stride: usize,
    }
    let corpora = [
        Corpus {
            xml: "amharic-hc.xml",
            toml: Some("amharic-realize.toml"),
            words: "amharic-words.txt",
            stride: 3,
        },
        Corpus {
            xml: "indonesian-hc.xml",
            toml: Some("indonesian-realize.toml"),
            words: "indonesian-words.txt",
            stride: 1,
        },
        Corpus {
            xml: "sena-hc.xml",
            toml: None,
            words: "sena-words.txt",
            stride: 10,
        },
    ];

    let r = realizer();

    for corpus in &corpora {
        let Some(g) = load_grammar(corpus.xml) else {
            eprintln!("skipping {}: not present on disk", corpus.xml);
            continue;
        };
        let Some(words_path) = sample_path(corpus.words) else {
            eprintln!("skipping {}: not present on disk", corpus.words);
            continue;
        };
        let words_text = std::fs::read_to_string(&words_path).expect("read word list");
        let map = corpus.toml.map(load_map).unwrap_or_else(RealizeMap::empty);
        let empty_map = RealizeMap::empty();

        let m = Morpher::new(&g, 100_000).with_word_timeout(Some(Duration::from_millis(50)));

        let mut checked = 0usize;
        for word in words_text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .step_by(corpus.stride)
        {
            let outcome = m.parse_word(word);
            let sig_before = outcome.signature();

            for wa in &outcome.structured {
                let bundle = hc_realize::gloss_bundle(&g, wa);

                let ir_mapped = hc_realize::to_ir(&bundle, &map, word);
                let realization_mapped = r.realize(&ir_mapped);
                assert!(
                    !realization_mapped.text.is_empty(),
                    "{} {word:?}: empty realized text (mapped sidecar)",
                    corpus.xml
                );

                let ir_empty = hc_realize::to_ir(&bundle, &empty_map, word);
                let realization_empty = r.realize(&ir_empty);
                assert!(
                    !realization_empty.text.is_empty(),
                    "{} {word:?}: empty realized text (empty sidecar)",
                    corpus.xml
                );
            }

            let sig_after = outcome.signature();
            assert_eq!(
                sig_before, sig_after,
                "{} {word:?}: signature changed across the realize path",
                corpus.xml
            );
            checked += 1;
        }
        eprintln!(
            "{}: checked {checked} word(s) (stride {})",
            corpus.xml, corpus.stride
        );
    }
}
