//! N0 integration gate: parses real sample-grammar words end-to-end through `Morpher` -> `pg_realize::gloss_bundle` -> `pg_realize::leipzig`, and pins the exact resulting strings.
//! See docs/natural-phrases-plan.md N0 for the plan; the doubled dashes in `beg--pfv--pfv.3m` are real grammar data (a `-pfv-` gloss), not a rendering bug. Sample grammars are gitignored, so real-grammar tests self-skip when absent; the two guessed-root tests below use a synthetic inline grammar and always run.

use std::path::{Path, PathBuf};

use pg_grammar::model::Grammar;
use pg_parse::{Morpher, ParseOptions};

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

/// Parse `word`, assert it produced exactly one analysis, and return its Leipzig rendering.
fn single_analysis_leipzig(g: &Grammar, word: &str) -> String {
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
    pg_realize::leipzig(&bundle, word)
}

// --- Indonesian ------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/indonesian-hc.xml); run with --include-ignored"]
fn indonesian_pinned_leipzig_strings() {
    let Some(g) = load_grammar("indonesian-hc.xml") else {
        eprintln!("skipping: indonesian-hc.xml not present on disk");
        return;
    };
    // root only, glossed via <LexicalEntry><Gloss>read</Gloss></LexicalEntry>.
    assert_eq!(single_analysis_leipzig(&g, "baca"), "read");
    // AV-prefix (mem+) + root, both glossed ("AV", "read").
    assert_eq!(single_analysis_leipzig(&g, "membaca"), "AV-read");
    assert_eq!(single_analysis_leipzig(&g, "mengambil"), "AV-take");
    // AV-prefix + root + reduplicative "Cont" (continuative) suffix rule.
    assert_eq!(
        single_analysis_leipzig(&g, "membagi-bagi"),
        "AV-divide-Cont"
    );
}

// --- Amharic ---------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/amharic-hc.xml); run with --include-ignored"]
fn amharic_pinned_leipzig_strings() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    // root only ("stomach").
    assert_eq!(single_analysis_leipzig(&g, "ሆድ"), "stomach");
    // root ("go") + perfective-aspect template rule (gloss literally "-pfv-") + 3sm perfective agreement rule ("pfv.3m").
    assert_eq!(single_analysis_leipzig(&g, "ሄደ"), "go--pfv--pfv.3m");
    assert_eq!(single_analysis_leipzig(&g, "ለመነ"), "beg--pfv--pfv.3m");
}

// --- Sena ------------------------------------------------------------------------------------

#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_pinned_leipzig_strings() {
    let Some(g) = load_grammar("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    // Sena glosses are Portuguese plus bare noun-class digits; these three happen to have exactly one surviving analysis each.
    assert_eq!(single_analysis_leipzig(&g, "uyu"), "este");
    assert_eq!(single_analysis_leipzig(&g, "miseru"), "4-caso");
    assert_eq!(single_analysis_leipzig(&g, "pya"), "8-ASSOC");
}

/// Sena's `mbali` is the free-fluctuation multi-analysis case; confirms `gloss_bundle`/`leipzig` produce one string per `outcome.structured[i]`, same order as `outcome.analyses`, undeduped.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/sena-hc.xml); run with --include-ignored"]
fn sena_multi_analysis_gloss_lines_match_analyses_count_and_order() {
    let Some(g) = load_grammar("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    let m = Morpher::new(&g, usize::MAX);
    let outcome = m.parse_word("mbali");
    assert!(
        outcome.analyses.len() > 1,
        "mbali should be genuinely ambiguous: {:?}",
        outcome.analyses
    );
    assert_eq!(outcome.analyses.len(), outcome.structured.len());

    let glosses: Vec<String> = outcome
        .structured
        .iter()
        .map(|wa| pg_realize::leipzig(&pg_realize::gloss_bundle(&g, wa), "mbali"))
        .collect();
    assert_eq!(glosses.len(), outcome.analyses.len());
    // None of the glosses is empty; gloss_bundle could otherwise silently drop tokens.
    assert!(glosses.iter().all(|s| !s.is_empty()), "{glosses:?}");
}

// --- Parity: the --gloss path must never perturb the frozen signature ------------------------

/// The gloss path only reads `&ParseOutcome`/`&Grammar`, never mutates either, so it must never change `result_signature`'s output; checked before and after across all three sample grammars.
#[test]
#[ignore = "needs local gitignored corpus data (samples/data/{indonesian,amharic,sena}-hc.xml); run with --include-ignored"]
fn gloss_path_never_perturbs_parity_signature() {
    for (xml_name, words) in [
        (
            "indonesian-hc.xml",
            &["baca", "membaca", "mengambil", "membagi-bagi"][..],
        ),
        ("amharic-hc.xml", &["ሆድ", "ሄደ", "ለመነ"][..]),
        ("sena-hc.xml", &["uyu", "miseru", "pya", "mbali"][..]),
    ] {
        let Some(g) = load_grammar(xml_name) else {
            eprintln!("skipping {xml_name}: not present on disk");
            continue;
        };
        let m = Morpher::new(&g, usize::MAX);
        for &word in words {
            let outcome = m.parse_word(word);
            let sig_before = outcome.signature();
            for wa in &outcome.structured {
                let bundle = pg_realize::gloss_bundle(&g, wa);
                let _ = pg_realize::leipzig(&bundle, word);
            }
            let sig_after = outcome.signature();
            assert_eq!(
                sig_before, sig_after,
                "{xml_name} {word:?}: signature changed across the gloss path"
            );
        }
    }
}

// --- Guessed-root path -------------------------------------------------------------------------

/// The three sample grammars have no `IsPattern` lexical entry, so the guess branch can never fire against them; this hand-transcribed fixture mirrors `pg-parse/tests/guesser_gate.rs`'s own instead.
const GUESS_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<HermitCrabInput>
  <Language>
    <Name>N0GuessGate</Name>
    <PartsOfSpeech><PartOfSpeech id="posV"><Name>V</Name></PartOfSpeech></PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cG"><Representations><Representation>g</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cD"><Representations><Representation>d</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
      <BoundaryDefinitions>
        <BoundaryDefinition id="cPlus"><Representations><Representation>+</Representation></Representations></BoundaryDefinition>
      </BoundaryDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRules="mrEd">
        <Name>Morphophonemic</Name>
        <MorphologicalRuleDefinitions>
          <MorphologicalRule id="mrEd" requiredPartsOfSpeech="posV">
            <Name>ed_suffix</Name>
            <MorphemeId>PAST</MorphemeId>
            <MorphologicalSubrules>
              <MorphologicalSubrule id="subEd">
                <MorphologicalInput>
                  <PhoneticSequence id="stem">
                    <OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence>
                  </PhoneticSequence>
                </MorphologicalInput>
                <MorphologicalOutput>
                  <CopyFromInput index="stem" />
                  <InsertSegments><PhoneticShape>+d</PhoneticShape></InsertSegments>
                </MorphologicalOutput>
              </MorphologicalSubrule>
            </MorphologicalSubrules>
          </MorphologicalRule>
        </MorphologicalRuleDefinitions>
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

fn guess_grammar() -> Grammar {
    pg_grammar::load(GUESS_XML)
        .unwrap_or_else(|e| panic!("guess-gate fixture grammar failed to load: {e}"))
}

/// A bare guessed root ("gag"): `gloss_bundle` must produce a single `is_root: true`, gloss-less token, rendered by `leipzig` as `*gag*`.
#[test]
fn guessed_root_only_word_renders_starred_surface() {
    let g = guess_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default().with_guess_root(true);
    let outcome = m.parse_word_opts("gag", &opts);

    assert!(outcome.guessed);
    assert_eq!(outcome.structured.len(), 1);
    let wa = &outcome.structured[0];
    assert_eq!(wa.morpheme_ids, vec![u32::MAX]);

    let bundle = pg_realize::gloss_bundle(&g, wa);
    assert_eq!(bundle.tokens.len(), 1);
    assert!(bundle.tokens[0].is_root);
    assert_eq!(bundle.tokens[0].gloss, None);
    assert_eq!(bundle.root_index, Some(0));
    assert!(bundle.guessed);

    assert_eq!(pg_realize::leipzig(&bundle, "gag"), "*gag*");
}

/// A guessed root plus a real affix that is itself gloss-less (`ed_suffix` has a `MorphemeId` but no `Gloss`): two tokens, root starred, affix rendered `[?]`.
#[test]
fn guessed_root_plus_real_affix_word_renders_starred_plus_bracket() {
    let g = guess_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default().with_guess_root(true);
    let outcome = m.parse_word_opts("gagd", &opts);

    assert!(outcome.guessed);
    // Two co-existing guesses for "gagd"; the 2-morph one (guessed root + PAST suffix) sorts first.
    assert_eq!(outcome.structured.len(), 2);
    let two_morph = &outcome.structured[0];
    assert_eq!(two_morph.morpheme_ids.len(), 2);

    let bundle = pg_realize::gloss_bundle(&g, two_morph);
    assert_eq!(bundle.tokens.len(), 2);
    assert!(bundle.tokens[0].is_root);
    assert!(!bundle.tokens[1].is_root);
    assert_eq!(
        bundle.tokens[1].gloss, None,
        "ed_suffix has a <MorphemeId> but no <Gloss>"
    );

    assert_eq!(pg_realize::leipzig(&bundle, "gagd"), "*gagd*-[?]");

    // parity: guess_root=false must be a complete no-op for this same word/grammar (§4.1).
    let opts_off = ParseOptions::default().with_guess_root(false);
    assert_eq!(m.parse_word_opts("gagd", &opts_off).signature(), "-");
}
