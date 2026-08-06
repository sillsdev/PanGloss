//! Parses real sample-grammar words end-to-end through `Morpher` -> `pg_realize::gloss_bundle` -> `pg_realize::to_ir` and pins the exact resulting `GlossIr`, self-skipping (never panicking) when the grammar XML or gitignored `*-realize.toml` sidecar is absent on disk. Every test touching a `samples/data/*` fixture is `#[ignore]`d so the default run stays fast; only the fully-synthetic `guessed_root_becomes_guessed_concept_and_never_panics` runs by default.

use std::path::{Path, PathBuf};

use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};
use pg_realize::{CaseRole, Concept, GlossIr, Num, Poss, RealizeMap};

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

/// Loads a sidecar map; callers are expected to have already self-skipped via `sample_path`/`load_grammar` when the fixture is absent, so a missing file here still panics.
fn load_map(toml_name: &str) -> RealizeMap {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir.join("../../../samples/data").join(toml_name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read sidecar {toml_name}: {e}"));
    RealizeMap::parse(&text).unwrap_or_else(|e| panic!("parse sidecar {toml_name}: {e}"))
}

/// Parse `word`, assert exactly one analysis, and return its `GlossIr` built through `map`.
fn single_analysis_ir(g: &Grammar, map: &RealizeMap, word: &str) -> GlossIr {
    let m = Morpher::new(g, usize::MAX);
    let outcome = m.parse_word(word);
    assert_eq!(
        outcome.structured.len(),
        1,
        "{word:?}: expected exactly one analysis, got {} ({:?})",
        outcome.structured.len(),
        outcome.analyses
    );
    let bundle = pg_realize::gloss_bundle(g, &outcome.structured[0]);
    pg_realize::to_ir(&bundle, map, word)
}

// --- Sidecar loading: both real files parse cleanly through the real restricted-TOML reader ---

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/{amharic,indonesian}-realize.toml); run with --include-ignored"]
fn both_real_sidecar_files_parse_without_error() {
    // load_map already panics on any parse error, so reaching the asserts below is most of the test; the lookups confirm the real content, not just that some map was produced.
    let Some(_) = sample_path("amharic-realize.toml") else {
        eprintln!("skipping: amharic-realize.toml not present on disk");
        return;
    };
    let Some(_) = sample_path("indonesian-realize.toml") else {
        eprintln!("skipping: indonesian-realize.toml not present on disk");
        return;
    };
    let amharic = load_map("amharic-realize.toml");
    assert_eq!(
        amharic.lookup("pl"),
        Some(pg_realize::FeatureAssignment::Num(Num::Pl))
    );
    assert_eq!(
        amharic.lookup("poss.1s"),
        Some(pg_realize::FeatureAssignment::Poss(Poss::P1Sg))
    );
    assert_eq!(amharic.lookup("nonexistent-gloss"), None);

    let indonesian = load_map("indonesian-realize.toml");
    assert_eq!(indonesian.lookup("Caus"), None, "minimal by design");
}

// --- Amharic: the real sidecar, exercising every feature slot -------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_bare_root_has_no_features_and_no_extras() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    // "stomach", root-only word (n0_gloss_gate.rs pins the same word's Leipzig string).
    let ir = single_analysis_ir(&g, &map, "ሆድ");
    assert_eq!(ir.concept, Concept::Lex("stomach".to_string()));
    assert_eq!(ir.num, Num::Unspec);
    assert_eq!(ir.poss, Poss::None);
    assert_eq!(ir.case, CaseRole::None);
    assert!(ir.extras.is_empty());
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_possessed_noun_maps_poss_via_sidecar() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    // "house-poss.2m": root "house" + poss.2m affix, single analysis.
    let ir = single_analysis_ir(&g, &map, "ቤትህ");
    assert_eq!(ir.concept, Concept::Lex("house".to_string()));
    assert_eq!(ir.num, Num::Unspec);
    assert_eq!(ir.poss, Poss::P2SgM);
    assert_eq!(ir.case, CaseRole::None);
    assert!(ir.extras.is_empty());
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_plural_only_noun_maps_num() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    // "child-pl", single unambiguous analysis: root "child" + pl, no possessor.
    let ir = single_analysis_ir(&g, &map, "ልጆች");
    assert_eq!(ir.concept, Concept::Lex("child".to_string()));
    assert_eq!(ir.num, Num::Pl);
    assert_eq!(ir.poss, Poss::None);
    assert_eq!(ir.case, CaseRole::None);
    assert!(ir.extras.is_empty());
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_pluralized_possessed_noun_maps_both_num_and_poss() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    // "ልጆቹ" is genuinely ambiguous (two analyses): [0] "child-pl-poss.3m", [1] "child-pl-def.m" (a definite-marker affix with no sidecar entry, so it lands in extras).
    let g_ref = &g;
    let m = Morpher::new(g_ref, usize::MAX);
    let outcome = m.parse_word("ልጆቹ");
    assert_eq!(
        outcome.structured.len(),
        2,
        "expected the documented 2-way ambiguity, got {:?}",
        outcome.analyses
    );

    let bundle0 = pg_realize::gloss_bundle(g_ref, &outcome.structured[0]);
    let ir0 = pg_realize::to_ir(&bundle0, &map, "ልጆቹ");
    assert_eq!(ir0.concept, Concept::Lex("child".to_string()));
    assert_eq!(ir0.num, Num::Pl);
    assert_eq!(ir0.poss, Poss::P3SgM);
    assert_eq!(ir0.case, CaseRole::None);
    assert!(ir0.extras.is_empty());

    let bundle1 = pg_realize::gloss_bundle(g_ref, &outcome.structured[1]);
    let ir1 = pg_realize::to_ir(&bundle1, &map, "ልጆቹ");
    assert_eq!(ir1.concept, Concept::Lex("child".to_string()));
    assert_eq!(ir1.num, Num::Pl);
    assert_eq!(ir1.poss, Poss::None, "def.m has no sidecar entry, not Poss");
    assert_eq!(ir1.case, CaseRole::None);
    assert_eq!(ir1.extras, vec!["def.m".to_string()]);
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_unmapped_verb_agreement_glosses_land_in_extras() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    // "go--pfv--pfv.3m": root "go" plus two verb-aspect/agreement affixes, both intentionally unmapped in amharic-realize.toml, so both land in extras in order.
    let ir = single_analysis_ir(&g, &map, "ሄደ");
    assert_eq!(ir.concept, Concept::Lex("go".to_string()));
    assert_eq!(ir.num, Num::Unspec);
    assert_eq!(ir.poss, Poss::None);
    assert_eq!(ir.case, CaseRole::None);
    assert_eq!(ir.extras, vec!["-pfv-".to_string(), "pfv.3m".to_string()]);
}

/// The sidecar's `Case` mappings, confirmed directly via `RealizeMap` rather than through a real corpus word, since no real word in this corpus exercises Case through `to_ir` end-to-end.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_sidecar_case_entries_map_as_intended() {
    let Some(_g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    assert_eq!(
        map.lookup("at"),
        Some(pg_realize::FeatureAssignment::Case(CaseRole::Loc))
    );
    assert_eq!(
        map.lookup("from"),
        Some(pg_realize::FeatureAssignment::Case(CaseRole::Abl))
    );
    assert_eq!(
        map.lookup("abl"),
        Some(pg_realize::FeatureAssignment::Case(CaseRole::Abl))
    );
    assert_eq!(
        map.lookup("to"),
        Some(pg_realize::FeatureAssignment::Case(CaseRole::All))
    );
}

// --- Indonesian: minimal sidecar, roots pass through, derivational affixes stay extras -------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn indonesian_root_only_word_has_no_extras_through_minimal_sidecar() {
    let Some(g) = load_grammar("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let map = load_map("indonesian-realize.toml");
    let ir = single_analysis_ir(&g, &map, "baca");
    assert_eq!(ir.concept, Concept::Lex("read".to_string()));
    assert_eq!(ir.num, Num::Unspec);
    assert_eq!(ir.poss, Poss::None);
    assert_eq!(ir.case, CaseRole::None);
    assert!(ir.extras.is_empty());
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn indonesian_voice_prefix_stays_in_extras_via_minimal_sidecar() {
    let Some(g) = load_grammar("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let map = load_map("indonesian-realize.toml");
    // "AV-read": root "read" plus the AV voice prefix, which has no entry in the minimal indonesian sidecar by design, so it lands in extras.
    let ir = single_analysis_ir(&g, &map, "membaca");
    assert_eq!(ir.concept, Concept::Lex("read".to_string()));
    assert_eq!(ir.extras, vec!["AV".to_string()]);
}

// --- Sena: no sidecar at all -- RealizeMap::empty() must still degrade gracefully ------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_root_only_word_through_empty_map() {
    let Some(g) = load_grammar("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    // No samples/data/sena-realize.toml by design (§ task spec: "NO sena file").
    let ir = single_analysis_ir(&g, &RealizeMap::empty(), "uyu");
    assert_eq!(ir.concept, Concept::Lex("este".to_string()));
    assert_eq!(ir.num, Num::Unspec);
    assert_eq!(ir.poss, Poss::None);
    assert_eq!(ir.case, CaseRole::None);
    assert!(ir.extras.is_empty());
}

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_two_morpheme_word_everything_unmapped_goes_to_extras() {
    let Some(g) = load_grammar("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    // "4-caso": root is "caso"; the non-root "4" (a noun-class digit gloss) has no mapping anywhere, so it lands in extras.
    let ir = single_analysis_ir(&g, &RealizeMap::empty(), "miseru");
    assert_eq!(ir.concept, Concept::Lex("caso".to_string()));
    assert_eq!(ir.num, Num::Unspec);
    assert_eq!(ir.poss, Poss::None);
    assert_eq!(ir.case, CaseRole::None);
    assert_eq!(ir.extras, vec!["4".to_string()]);
}

// Guessed root: no real sample grammar has an `IsPattern`/`[Any]*` lexical entry, so this reuses a small synthetic fixture instead.

const GUESS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>N1GuessGate</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1">
        <Name>Morphophonemic</Name>
        <LexicalEntries>
          <LexicalEntry id="ePattern">
            <Allomorphs><Allomorph id="aPattern"><PhoneticShape>[Any]*</PhoneticShape></Allomorph></Allomorphs>
            <Gloss>pattern</Gloss>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>
"#;

#[test]
fn guessed_root_becomes_guessed_concept_and_never_panics() {
    let g = pg_grammar::load(GUESS_XML)
        .unwrap_or_else(|e| panic!("guess-gate fixture grammar failed to load: {e}"));
    let m = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default().with_guess_root(true);
    let outcome = m.parse_word_opts("gag", &opts);

    assert!(outcome.guessed);
    assert_eq!(outcome.structured.len(), 1);
    let bundle = pg_realize::gloss_bundle(&g, &outcome.structured[0]);
    let ir = pg_realize::to_ir(&bundle, &RealizeMap::empty(), "gag");

    assert_eq!(ir.concept, Concept::Guessed("gag".to_string()));
    assert_eq!(ir.num, Num::Unspec);
    assert_eq!(ir.poss, Poss::None);
    assert_eq!(ir.case, CaseRole::None);
    assert!(ir.extras.is_empty());
}

// Robustness (bounded smoke check): an uncapped `Morpher::new(g, usize::MAX)` sweep over the full corpora ran 8+ minutes without finishing against the 7121-word Sena corpus, so this instead takes a small bounded slice with a capped `Morpher` (`cap = 20`, `with_word_timeout`) purely to confirm `to_ir` never panics on ordinary corpus words.

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/*-hc.xml, *-words.txt); run with --include-ignored"]
fn to_ir_never_panics_on_a_bounded_corpus_sample() {
    let cases: &[(&str, Option<&str>, &str)] = &[
        (
            "amharic-hc.xml",
            Some("amharic-realize.toml"),
            "amharic-words.txt",
        ),
        (
            "indonesian-hc.xml",
            Some("indonesian-realize.toml"),
            "indonesian-words.txt",
        ),
        ("sena-hc.xml", None, "sena-words.txt"),
    ];
    for (xml_name, toml_name, words_name) in cases {
        let Some(g) = load_grammar(xml_name) else {
            eprintln!("skipping {xml_name}: not present on disk");
            continue;
        };
        let Some(words_path) = sample_path(words_name) else {
            eprintln!("skipping {words_name}: not present on disk");
            continue;
        };
        let words = std::fs::read_to_string(&words_path).expect("read word list");
        let map = toml_name.map(load_map).unwrap_or_else(RealizeMap::empty);
        let m = Morpher::new(&g, 20).with_word_timeout(Some(std::time::Duration::from_millis(500)));
        for word in words.lines().filter(|l| !l.trim().is_empty()).take(100) {
            let outcome = m.parse_word(word);
            for wa in &outcome.structured {
                let bundle = pg_realize::gloss_bundle(&g, wa);
                let ir_mapped = pg_realize::to_ir(&bundle, &map, word);
                let ir_empty = pg_realize::to_ir(&bundle, &RealizeMap::empty(), word);
                // Total function: reaching here at all is the assertion; also check the concept string is never silently empty.
                match &ir_mapped.concept {
                    Concept::Lex(s) | Concept::Guessed(s) => {
                        assert!(!s.is_empty(), "{xml_name} {word:?}: empty concept string")
                    }
                }
                match &ir_empty.concept {
                    Concept::Lex(s) | Concept::Guessed(s) => {
                        assert!(!s.is_empty(), "{xml_name} {word:?}: empty concept string")
                    }
                }
            }
        }
    }
}
