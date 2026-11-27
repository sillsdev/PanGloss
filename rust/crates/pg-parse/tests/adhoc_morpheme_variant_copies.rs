//! Ad-hoc morpheme prohibitions naming a main entry's MSA must also hold on every variant entry that borrows it, as HCLoader registers variants in `m_morphemes[mainMsa]` (data/adhoc-morpheme-variant-copies/README.md).

use pg_parse::{Morpher, ParseOptions};

const PROJECT: &str = include_str!("data/adhoc-morpheme-variant-copies/project.fwdata");
const VERB_MSA: &str = "00000000-0000-0000-0000-000000000102";
const NOUN_MSA: &str = "00000000-0000-0000-0000-000000000103";
const PREFIX_MSA: &str = "00000000-0000-0000-0000-000000000202";
const ENCLITIC_MSA: &str = "00000000-0000-0000-0000-000000000402";

fn prohibition(n: usize, first: &str, rest: &str) -> String {
    format!(
        "<rt class=\"MoMorphAdhocProhib\" guid=\"00000000-0000-0000-0000-0000000009a{n}\"><Adjacency val=\"0\" /><FirstMorpheme><objsur guid=\"{first}\" t=\"r\" /></FirstMorpheme><RestOfMorphs><objsur guid=\"{rest}\" t=\"r\" /></RestOfMorphs></rt>"
    )
}

/// Analysis counts for the words in `WORDS`, in order, with `rules` appended to the project.
const WORDS: [&str; 6] = ["tada", "tadi", "da", "di", "diz", "daz"];

fn observe(rules: &[String]) -> Vec<usize> {
    let source = PROJECT.replace(
        "</languageproject>",
        &format!("{}</languageproject>", rules.concat()),
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.fwdata");
    std::fs::write(&path, source).unwrap();
    let (snapshot, _report) = pg_fwdata::import_file(&path).unwrap();
    let (grammar, _warnings) = pg_grammar::compile_project(&snapshot).unwrap();
    let morpher = Morpher::new(&grammar, usize::MAX);
    WORDS
        .iter()
        .map(|word| {
            let out = morpher.parse_word_opts(word, &ParseOptions::default());
            assert!(!out.invalid_shape && !out.capped && !out.timed_out && !out.guessed);
            out.structured.len()
        })
        .collect()
}

fn count(counts: &[usize], word: &str) -> usize {
    counts[WORDS.iter().position(|w| *w == word).unwrap()]
}

#[test]
fn adhoc_morpheme_prohibition_on_a_main_msa_also_blocks_its_variant_stem_with_a_prefix() {
    let base = observe(&[]);
    for word in WORDS {
        assert!(count(&base, word) > 0, "control: {word} parses");
    }
    for (name, first, rest) in [
        ("main first", VERB_MSA, PREFIX_MSA),
        ("prefix first", PREFIX_MSA, VERB_MSA),
    ] {
        let after = observe(&[prohibition(1, first, rest)]);
        assert_eq!(count(&after, "tada"), 0, "{name}: main stem blocked");
        assert_eq!(count(&after, "tadi"), 0, "{name}: variant stem blocked");
        for word in ["da", "di", "diz", "daz"] {
            assert_eq!(
                count(&after, word),
                count(&base, word),
                "{name}: {word} unaffected"
            );
        }
    }
}

#[test]
fn adhoc_morpheme_prohibition_on_a_main_msa_also_blocks_its_variant_stem_with_an_enclitic() {
    let base = observe(&[]);
    // Both main MSAs, so no reading of the host survives.
    for (name, rules) in [
        (
            "main first",
            [
                prohibition(1, VERB_MSA, ENCLITIC_MSA),
                prohibition(2, NOUN_MSA, ENCLITIC_MSA),
            ],
        ),
        (
            "enclitic first",
            [
                prohibition(1, ENCLITIC_MSA, VERB_MSA),
                prohibition(2, ENCLITIC_MSA, NOUN_MSA),
            ],
        ),
    ] {
        let after = observe(&rules);
        assert_eq!(count(&after, "daz"), 0, "{name}: main stem blocked");
        assert_eq!(count(&after, "diz"), 0, "{name}: variant stem blocked");
        for word in ["da", "di", "tada", "tadi"] {
            assert_eq!(
                count(&after, word),
                count(&base, word),
                "{name}: {word} unaffected"
            );
        }
    }
}
