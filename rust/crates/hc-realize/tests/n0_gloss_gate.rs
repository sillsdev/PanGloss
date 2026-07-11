//! N0 integration gate (`docs/natural-phrases-plan.md` N0): parse real sample-grammar words
//! end-to-end through `Morpher` -> `hc_realize::gloss_bundle` -> `hc_realize::leipzig`, and pin
//! the exact resulting strings.
//!
//! The sample grammars are untracked local corpus files (per `rust-conversion.md` §8 / the
//! precedent in `hc-parse/tests/root_trie_gate.rs` etc.) -- present on this development machine
//! but not guaranteed present elsewhere (CI, a fresh clone), so every real-grammar test self-skips
//! when the file is absent rather than failing.
//!
//! Every pinned string below was obtained by first running the parse, inspecting the actual
//! `WordAnalysis`/gloss output, and cross-checking each token against the grammar XML's own
//! `<Gloss>` elements (task instruction: run first, read the XML, THEN pin) -- notably the
//! Amharic perfective-aspect rule's gloss is literally the string `-pfv-` (a templatic-morphology
//! marker authored with hyphens already inside it), which is why `beg--pfv--pfv.3m` has doubled
//! dashes: that is `leipzig`'s plain `-`-join faithfully reproducing real grammar data, not a
//! rendering bug.

use std::path::{Path, PathBuf};

use hc_grammar::model::Grammar;
use hc_parse::{Morpher, ParseOptions};

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

/// Parse `word`, assert it produced exactly one analysis (the common case for these pinned
/// words), and return its Leipzig rendering.
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
    let bundle = hc_realize::gloss_bundle(g, &outcome.structured[0]);
    hc_realize::leipzig(&bundle, word)
}

// --- Indonesian ------------------------------------------------------------------------------

#[test]
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
fn amharic_pinned_leipzig_strings() {
    let Some(g) = load_grammar("amharic-hc.xml") else {
        eprintln!("skipping: amharic-hc.xml not present on disk");
        return;
    };
    // root only ("stomach").
    assert_eq!(single_analysis_leipzig(&g, "ሆድ"), "stomach");
    // root ("go") + perfective-aspect template rule (gloss authored as the literal string
    // "-pfv-") + 3rd-person-masculine perfective agreement rule ("pfv.3m").
    assert_eq!(single_analysis_leipzig(&g, "ሄደ"), "go--pfv--pfv.3m");
    assert_eq!(single_analysis_leipzig(&g, "ለመነ"), "beg--pfv--pfv.3m");
}

// --- Sena ------------------------------------------------------------------------------------

#[test]
fn sena_pinned_leipzig_strings() {
    let Some(g) = load_grammar("sena-hc.xml") else {
        eprintln!("skipping: sena-hc.xml not present on disk");
        return;
    };
    // Sena glosses are Portuguese + bare noun-class digits (design doc's "pure fallback"
    // corpus) -- these three happen to have exactly one surviving analysis each.
    assert_eq!(single_analysis_leipzig(&g, "uyu"), "este");
    assert_eq!(single_analysis_leipzig(&g, "miseru"), "4-caso");
    assert_eq!(single_analysis_leipzig(&g, "pya"), "8-ASSOC");
}

/// Sena's `mbali` is the free-fluctuation multi-analysis case documented in
/// `hc-parse/src/lib.rs::result_signature` and `sena_free_fluctuation_gate.rs` -- confirms
/// `gloss_bundle`/`leipzig` produce one Leipzig string per `outcome.structured[i]`, same index
/// order as `outcome.analyses`, without collapsing or reordering the (deliberately undeduped)
/// candidate set.
#[test]
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
        .map(|wa| hc_realize::leipzig(&hc_realize::gloss_bundle(&g, wa), "mbali"))
        .collect();
    assert_eq!(glosses.len(), outcome.analyses.len());
    // None of the glosses is empty (every analysis has a root, and every morpheme here has a
    // gloss in the Sena XML) -- a real robustness signal, not a tautology, since gloss_bundle
    // could otherwise silently drop tokens.
    assert!(glosses.iter().all(|s| !s.is_empty()), "{glosses:?}");
}

// --- Parity: the --gloss path must never perturb the frozen signature ------------------------

/// The gloss path reads `ParseOutcome` after it is fully built; it must never be able to change
/// `result_signature`'s output. Computing the signature both before and after walking every
/// analysis through `gloss_bundle`/`leipzig` (which only reads `&ParseOutcome`/`&Grammar`, never
/// mutates either) pins that non-interference for real multi-morpheme, multi-analysis words
/// across all three sample grammars.
#[test]
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
                let bundle = hc_realize::gloss_bundle(&g, wa);
                let _ = hc_realize::leipzig(&bundle, word);
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

/// The three sample grammars have no `IsPattern` lexical entry (verified: no `<PhoneticShape>`
/// starting with `[` anywhere in `samples/data/*.xml`), so the guess branch can never fire
/// against them -- ported here from the same hand-transcribed-grammar approach
/// `hc-parse/tests/guesser_gate.rs` uses (that file's own module doc explains why: there is no
/// upstream C# CLI `--guess` flag to diff a golden TSV against either). One V-requiring `+d`
/// suffix rule (`<MorphemeId>PAST</MorphemeId>`, no `<Gloss>`) and one `[Any]*` lexical-pattern
/// entry with an empty syntactic FS, matching `guesser_gate.rs`'s fixture.
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
    hc_grammar::load(GUESS_XML)
        .unwrap_or_else(|e| panic!("guess-gate fixture grammar failed to load: {e}"))
}

/// A bare guessed root ("gag", no lexicon entry, no affix): `gloss_bundle` must produce a single
/// `is_root: true`, gloss-less token, and `leipzig` must render it as `*gag*` (the surface word
/// between asterisks), matching this crate's own doc'd guessed-root rendering rule.
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

    let bundle = hc_realize::gloss_bundle(&g, wa);
    assert_eq!(bundle.tokens.len(), 1);
    assert!(bundle.tokens[0].is_root);
    assert_eq!(bundle.tokens[0].gloss, None);
    assert_eq!(bundle.root_index, Some(0));
    assert!(bundle.guessed);

    assert_eq!(hc_realize::leipzig(&bundle, "gag"), "*gag*");
}

/// A guessed root plus a real, glossed... well, gloss-less-by-construction (`ed_suffix` has no
/// `<Gloss>`, only a `<MorphemeId>PAST</MorphemeId>`) affix rule ("gagd" = guessed root "gag" +
/// `ed_suffix`): two tokens, root starred, the real affix rendered `[?]` since it has no
/// `<Gloss>` element (a fresh, real defensive case distinct from the guessed sentinel).
#[test]
fn guessed_root_plus_real_affix_word_renders_starred_plus_bracket() {
    let g = guess_grammar();
    let m = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default().with_guess_root(true);
    let outcome = m.parse_word_opts("gagd", &opts);

    assert!(outcome.guessed);
    // Two co-existing guesses for "gagd" (guesser_gate.rs's own headline pin); the 2-morph one
    // (guessed root + PAST suffix) sorts first.
    assert_eq!(outcome.structured.len(), 2);
    let two_morph = &outcome.structured[0];
    assert_eq!(two_morph.morpheme_ids.len(), 2);

    let bundle = hc_realize::gloss_bundle(&g, two_morph);
    assert_eq!(bundle.tokens.len(), 2);
    assert!(bundle.tokens[0].is_root);
    assert!(!bundle.tokens[1].is_root);
    assert_eq!(
        bundle.tokens[1].gloss, None,
        "ed_suffix has a <MorphemeId> but no <Gloss>"
    );

    assert_eq!(hc_realize::leipzig(&bundle, "gagd"), "*gagd*-[?]");

    // parity: guess_root=false must be a complete no-op for this same word/grammar (§4.1).
    let opts_off = ParseOptions::default().with_guess_root(false);
    assert_eq!(m.parse_word_opts("gagd", &opts_off).signature(), "-");
}
