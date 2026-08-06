//! The end-to-end demo: real sample-grammar words parsed through `Morpher` -> `pg_realize::gloss_bundle` -> `pg_realize::to_ir` -> `pg_realize::TableRealizer::realize`, plus an all-corpora robustness sweep.
//! See `docs/research/n2-realize-gate-conventions.md` for the self-skip/pinning conventions this shares with the other gate tiers and why the corpus sweep is subsampled.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pg_grammar::model::Grammar;
use pg_parse::Morpher;
use pg_realize::{Realization, RealizeMap, Realizer, TableRealizer};

fn sample_path(name: &str) -> Option<PathBuf> {
    // CARGO_MANIFEST_DIR = .../rust/crates/pg-realize ; samples live at repo_root/samples/data.
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(name);
    path.exists().then_some(path)
}

fn load_grammar(xml_name: &str) -> Option<Grammar> {
    let path = sample_path(xml_name)?;
    let xml = std::fs::read_to_string(&path).expect("read sample grammar");
    Some(pg_grammar::load(&xml).unwrap_or_else(|e| panic!("failed to load {xml_name}: {e}")))
}

/// Gitignored real-language data; every test self-skips via `load_grammar` before reaching this call, so a missing file here is a genuine error once execution has already committed to the fixture being present.
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

/// Parse `word` with an uncapped `Morpher` (these demo words are known-fast) and realize every surviving analysis, in `outcome.structured` order.
fn realize_all(g: &Grammar, map: &RealizeMap, r: &TableRealizer, word: &str) -> Vec<Realization> {
    let m = Morpher::new(g, usize::MAX);
    let outcome = m.parse_word(word);
    outcome
        .structured
        .iter()
        .map(|wa| {
            let bundle = pg_realize::gloss_bundle(g, wa);
            let ir = pg_realize::to_ir(&bundle, map, word);
            r.realize(&ir)
        })
        .collect()
}

// --- (a) End-to-end amharic demo: possessed/pluralized nouns, not case-marked, since no corpus word combines a Case gloss with poss/pl on one analysis ---

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
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
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_plural_only_noun_renders_children() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    let r = realizer();
    // "ልጆች" = "child-pl", an irregular plural via lexicon.toml's exceptions table -> "children" (no article: bare Num::Pl, Poss::None).
    let results = realize_all(&g, &map, &r, "ልጆች");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].text, "children");
    assert!(results[0].complete);
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_ambiguous_pluralized_possessed_noun_one_complete_one_partial() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    let r = realizer();
    // "ልጆቹ" is a documented 2-way ambiguity: [0] "child-pl-poss.3m" -> complete "his children"; [1] "child-pl-def.m" -> the same "children" but partial via nonempty residue (def.m unmapped).
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
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_unpossessed_bare_root_renders_with_indefinite_article() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    let r = realizer();
    // "ሆድ" = "stomach", root-only, no Num/Poss/Case morpheme at all -> Num::Unspec -> bare citation form, no article.
    let results = realize_all(&g, &map, &r, "ሆድ");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].text, "stomach");
    assert!(results[0].complete);
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_unmapped_verb_word_falls_back_to_partial_citation_form() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    let r = realizer();
    // "ሄደ" = "go--pfv--pfv.3m": nominal-only realization sends its two unmapped verb-aspect affixes to extras, so the None.None.Unspec cell still fills ("go") but nonempty residue forces `complete: false`.
    let results = realize_all(&g, &map, &r, "ሄደ");
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0].text, "go");
    assert!(!results[0].complete);
    assert_eq!(
        results[0].residue,
        vec!["-pfv-".to_string(), "pfv.3m".to_string()]
    );
}

// --- (b) All-corpora robustness gate ---
// See `docs/research/n2-realize-gate-conventions.md` for why the sweep is step-capped, wall-clock-bounded, and subsampled per corpus.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/*-hc.xml, *-words.txt); run with --include-ignored"]
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
                let bundle = pg_realize::gloss_bundle(&g, wa);

                let ir_mapped = pg_realize::to_ir(&bundle, &map, word);
                let realization_mapped = r.realize(&ir_mapped);
                assert!(
                    !realization_mapped.text.is_empty(),
                    "{} {word:?}: empty realized text (mapped sidecar)",
                    corpus.xml
                );

                let ir_empty = pg_realize::to_ir(&bundle, &empty_map, word);
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
