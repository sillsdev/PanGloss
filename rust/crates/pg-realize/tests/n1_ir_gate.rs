//! N1 integration gate (`docs/natural-phrases-plan.md` N1): parse real sample-grammar words
//! end-to-end through `Morpher` -> `pg_realize::gloss_bundle` -> `pg_realize::to_ir`, loading the
//! real sidecar files from `samples/data/*-realize.toml`, and pin the exact resulting `GlossIr`
//! values.
//!
//! Same self-skip discipline as `n0_gloss_gate.rs`: every real-grammar test no-ops when the
//! grammar XML is absent on disk. The two sidecar TOML files this milestone adds
//! (`samples/data/amharic-realize.toml`, `samples/data/indonesian-realize.toml`) are tracked
//! alongside this commit, so they're expected to be present whenever their matching grammar is;
//! a missing sidecar when the grammar *is* present is a real authoring bug, not an environment
//! difference, and is allowed to panic via `expect`.
//!
//! Every pinned `GlossIr` below was obtained by first running the parse with `--gloss` (or via
//! `pg_realize::gloss_bundle`/`leipzig` directly) to see the actual bundle, THEN writing the
//! expected `GlossIr` -- same discipline `n0_gloss_gate.rs` documents for its Leipzig strings.
//!
//! Search note (documented per the milestone's task instructions): the full 673-word
//! `amharic-words.txt` corpus was scanned (every word parsed with `--gloss`, output grepped for
//! `poss.`/`pl`/`at`/`from`/`to`/`abl` tokens) for a word combining a case-role gloss with a
//! `poss.*` and/or `pl` gloss on the same analysis, and no such word exists in this corpus --
//! `at`/`from`/`to`/`abl` and the plural/possessive affixes never co-occur in the sample data
//! (the grammar's case-like adpositions mostly attach directly as their own root-like
//! allomorphs, e.g. `ላንተ` "to-2m", where "to" is parsed as the *root* token rather than an
//! affix on a noun, so `to_ir`'s root-exclusion rule keeps it out of the Case slot entirely for
//! that word). The richest bundle that *does occur* in the corpus combining two mapped feature
//! slots is `ልጆቹ` "child-pl-poss.3m" / "child-pl-def.m" (two genuinely ambiguous analyses,
//! `outcome.structured[0]` = the poss.3m reading, `[1]` = the def.m reading) -- pinned below by
//! explicit analysis index, same technique `n0_gloss_gate.rs`'s
//! `sena_multi_analysis_gloss_lines_match_analyses_count_and_order` uses for Sena's `mbali`.
//! The sidecar's `Case` mappings themselves are confirmed directly against the loaded
//! `RealizeMap` (`amharic_sidecar_case_entries_map_as_intended` below), since no real corpus
//! word exercises Case through `to_ir` end-to-end.

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

/// Load a sidecar map that is expected to exist whenever its grammar does (tracked alongside
/// this milestone's commit) -- a missing file here is an authoring bug, so this panics rather
/// than self-skipping.
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
fn both_real_sidecar_files_parse_without_error() {
    // amharic-realize.toml exercises comments, the [features] header, and quoted keys with dots
    // ("poss.1s" etc.); indonesian-realize.toml exercises a header-only (no entries) file.
    // load_map already panics on any parse error, so reaching the asserts below is most of the
    // test; the lookups confirm the real content, not just that *some* map was produced.
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
fn amharic_pluralized_possessed_noun_maps_both_num_and_poss() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    // "ልጆቹ" is genuinely ambiguous (two analyses: n0_gloss_gate.rs's sibling test documents the
    // same pattern for Sena's "mbali"): [0] "child-pl-poss.3m" (root "child" + pl + poss.3m),
    // [1] "child-pl-def.m" (root "child" + pl + a "def.m" definite-marker affix that has no
    // sidecar entry -> extras). This is the richest bundle in the corpus combining two mapped
    // feature slots (Num + Poss) on one analysis.
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
fn amharic_unmapped_verb_agreement_glosses_land_in_extras() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    let map = load_map("amharic-realize.toml");
    // "go--pfv--pfv.3m" (n0_gloss_gate.rs pins the same Leipzig string): root "go" + the two
    // verb-aspect/agreement affixes, neither of which has a sidecar entry (verb-related glosses
    // are intentionally unmapped in amharic-realize.toml) -> both land in extras, in order.
    let ir = single_analysis_ir(&g, &map, "ሄደ");
    assert_eq!(ir.concept, Concept::Lex("go".to_string()));
    assert_eq!(ir.num, Num::Unspec);
    assert_eq!(ir.poss, Poss::None);
    assert_eq!(ir.case, CaseRole::None);
    assert_eq!(ir.extras, vec!["-pfv-".to_string(), "pfv.3m".to_string()]);
}

/// The sidecar's `Case` mappings (`at`->Loc, `from`/`abl`->Abl, `to`->All) themselves, confirmed
/// end-to-end through the real loaded sidecar file rather than a hand-built one -- the corpus
/// search documented at module level found no real word co-occurring a case gloss with poss/pl,
/// but `at`/`from`/`to`/`abl` alone as a word's own root-affix pair do occur (e.g. `ላንተ`
/// "to-2m" parses with "to" as the *root* token, not an affix, per `pg_realize::to_ir`'s root-
/// exclusion rule -- so that real word cannot exercise Case mapping either; this test instead
/// confirms the loaded sidecar produces the intended Case assignments directly via `RealizeMap`,
/// which is what `to_ir`'s priority-chain step 2 actually consults).
#[test]
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
fn indonesian_voice_prefix_stays_in_extras_via_minimal_sidecar() {
    let Some(g) = load_grammar("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    let map = load_map("indonesian-realize.toml");
    // "AV-read": root "read" + the AV voice prefix, which has no entry in the minimal
    // indonesian sidecar by design -> extras.
    let ir = single_analysis_ir(&g, &map, "membaca");
    assert_eq!(ir.concept, Concept::Lex("read".to_string()));
    assert_eq!(ir.extras, vec!["AV".to_string()]);
}

// --- Sena: no sidecar at all -- RealizeMap::empty() must still degrade gracefully ------------

#[test]
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
fn sena_two_morpheme_word_everything_unmapped_goes_to_extras() {
    let Some(g) = load_grammar("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    // "4-caso": root is "caso" (index 1, per pg_grammar's own root_morpheme_index resolution --
    // confirmed by inspection, not assumed), non-root "4" (a noun-class digit gloss) has no
    // mapping anywhere (RealizeMap::empty()) -> extras.
    let ir = single_analysis_ir(&g, &RealizeMap::empty(), "miseru");
    assert_eq!(ir.concept, Concept::Lex("caso".to_string()));
    assert_eq!(ir.num, Num::Unspec);
    assert_eq!(ir.poss, Poss::None);
    assert_eq!(ir.case, CaseRole::None);
    assert_eq!(ir.extras, vec!["4".to_string()]);
}

// --- Guessed root: no real sample grammar can trigger it (n0_gloss_gate.rs's own finding: none
// of the three sample grammars has an `IsPattern`/`[Any]*` lexical entry), so this reuses the
// same small synthetic fixture approach n0_gloss_gate.rs and pg-parse/tests/guesser_gate.rs use.

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

// --- Robustness (bounded smoke check) ---------------------------------------------------------
//
// N1's task list calls for the amharic/indonesian/sena cases pinned above, not the exhaustive
// per-word robustness sweep -- `docs/natural-phrases-plan.md` explicitly scopes "for every word
// in all three samples/data/*-words.txt ... never panics" as an **N2** gate. A first attempt at
// an N1-local version of that full sweep (all 673+121+7121 corpus words, uncapped
// `Morpher::new(g, usize::MAX)`) was cut after it ran for 8+ minutes of CPU time without
// finishing against the 7121-word Sena corpus -- some Sena word(s) trigger heavy ambiguity/
// backtracking at that scale, which is exactly the kind of perf question N2's real robustness
// gate (with a bounded analysis cap, per the plan) needs to characterize properly, not something
// to paper over here. This smoke test instead takes a small bounded slice of each corpus with a
// capped `Morpher` (`cap = 20`, `with_word_timeout`) purely to confirm `to_ir` itself never
// panics on ordinary corpus words -- it is not a substitute for N2's gate.

#[test]
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
                // Total function: reaching here at all is the assertion. Also sanity-check the
                // concept is never a way to smuggle a panic-prone empty string through silently.
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
