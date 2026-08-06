//! Verifies duplicate-count determinism under parallel batch, rather than assuming it holds.
//! See `docs/research/duplicate-count-determinism.md` for why this holds and how the fixtures below are chosen.

use pg_assess::digest::{digest_projection, SEMANTIC_PROJECTION};
use pg_assess::identity::AnalysisIdentity;
use pg_assess::set::AnalysisSet;
use pg_foma::composite::FomaAnalyzer;
use pg_grammar::model::Grammar;

/// Bare-root synthetic grammar (never modeled on a real language): one lexical entry with three allomorphs of the identical shape "kax", expected to collapse to one `AnalysisIdentity` with `duplicate_count == 3`.
const DUP_ROOT_FIXTURE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput>
  <Language>
    <Name>SyntheticTripleAllomorphDuplicateProbe</Name>
    <PartsOfSpeech>
      <PartOfSpeech id="posV"><Name>v</Name></PartOfSpeech>
    </PartsOfSpeech>
    <CharacterDefinitionTable id="t1">
      <Name>Main</Name>
      <SegmentDefinitions>
        <SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
        <SegmentDefinition id="cX"><Representations><Representation>x</Representation></Representations></SegmentDefinition>
      </SegmentDefinitions>
    </CharacterDefinitionTable>
    <NaturalClasses>
      <FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass>
    </NaturalClasses>
    <Strata>
      <Stratum characterDefinitionTable="t1" morphologicalRuleOrder="unordered">
        <Name>Main</Name>
        <LexicalEntries>
          <LexicalEntry id="eKax" partOfSpeech="posV">
            <Allomorphs>
              <Allomorph id="aKax1"><PhoneticShape>kax</PhoneticShape></Allomorph>
              <Allomorph id="aKax2"><PhoneticShape>kax</PhoneticShape></Allomorph>
              <Allomorph id="aKax3"><PhoneticShape>kax</PhoneticShape></Allomorph>
            </Allomorphs>
            <MorphemeId>Kax</MorphemeId>
          </LexicalEntry>
        </LexicalEntries>
      </Stratum>
    </Strata>
  </Language>
</HermitCrabInput>"#;

fn load(xml: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("fixture failed to load: {e}"))
}

/// Projects every structured analysis in `outcome` to its `AnalysisSetEntry` list, one call site so every run in this file builds the set identically.
fn project_set(g: &Grammar, structured: &[pg_parse::WordAnalysis]) -> AnalysisSet {
    let identities: Vec<AnalysisIdentity> = structured
        .iter()
        .map(|wa| {
            AnalysisIdentity::project(wa, g)
                .unwrap_or_else(|e| panic!("analysis failed to project to an identity: {e}"))
        })
        .collect();
    AnalysisSet::from_observed(identities)
}

/// This fixture's C(12,6) analyses each fire a different rule subset and so exercise zero identity duplicates; self-skips if the submodule path is unavailable.
#[test]
fn sanity_deep_optional_affix_nesting_produces_no_identity_duplicates() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir
        .join("../../../machine/conformance/edge-cases/deep-optional-affix-nesting/grammar.xml");
    if !path.exists() {
        eprintln!("skipping: deep-optional-affix-nesting/grammar.xml not present on disk");
        return;
    }
    let xml = std::fs::read_to_string(&path).expect("read grammar");
    let g = load(&xml);
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture compiles");

    // k=2 leading x's: C(12,2) = 66 analyses, small enough to stay fast, large enough to make a spurious identity collision unlikely.
    let outcome = analyzer.analyze_word("xxk");
    assert_eq!(
        outcome.structured.len(),
        66,
        "C(12,2) = 66 is this fixture's own documented combinatorics; a different count means \
         the fixture or the engine changed under this test, not that duplicates appeared"
    );
    let set = project_set(&g, &outcome.structured);
    assert_eq!(
        set.len(),
        66,
        "every analysis is expected to carry a DISTINCT AnalysisIdentity (a different subset of \
         the 12 optional rules fired) -- confirming this fixture cannot exercise duplicate-count \
         determinism, exactly as task 1.12 asks to verify rather than assume"
    );
    assert!(
        set.entries().iter().all(|e| e.duplicate_count == 1),
        "if this ever fails, the fixture DOES produce genuine duplicates and this file's choice \
         of DUP_ROOT_FIXTURE as the determinism vehicle should be revisited"
    );
}

/// Sanity check that `DUP_ROOT_FIXTURE` is not vacuous: a single sequential parse of its bare root recovers 3 raw analyses collapsing to one identity with `duplicate_count == 3`.
#[test]
fn dup_root_fixture_genuinely_produces_a_triple_duplicate() {
    let g = load(DUP_ROOT_FIXTURE);
    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture compiles");
    let outcome = analyzer.analyze_word("kax");

    assert_eq!(
        outcome.structured.len(),
        3,
        "expected 3 raw confirmed analyses (one per allomorph), got {}: {:?}",
        outcome.structured.len(),
        outcome.analyses
    );
    let set = project_set(&g, &outcome.structured);
    assert_eq!(
        set.len(),
        1,
        "all 3 raw analyses must collapse to ONE AnalysisIdentity (same morphemes/root_index/\
         category) or this fixture is not exercising duplicate multiplicity at all"
    );
    assert_eq!(
        set.entries()[0].duplicate_count,
        3,
        "genuine duplicate_count > 1 is the whole premise of this test file; a count of 1 here \
         would make every other test in this file vacuous"
    );
}

/// The load-bearing check: every word's `AnalysisSet` and the semantic-projection digest built from it must be byte-identical across every run and thread count.
#[test]
fn duplicate_counts_and_semantic_digest_are_thread_count_invariant() {
    let g = load(DUP_ROOT_FIXTURE);

    // A mixed batch of duplicate-producing and zero-candidate words, large enough to give a >1-thread rayon pool genuine scheduling freedom.
    let mut words: Vec<String> = Vec::new();
    for i in 0..40 {
        words.push("kax".to_string());
        if i % 5 == 0 {
            words.push("zzzqxxxnonsense".to_string());
        }
    }

    let thread_counts = [1usize, 2, 4, 8];
    let repetitions = 15;

    // The first (threads=1, rep=0) run is the baseline every later run is compared to.
    let mut baseline: Option<(Vec<Vec<u32>>, String)> = None;

    for &threads in &thread_counts {
        for rep in 0..repetitions {
            // A fresh analyzer per run, so the property holds for ordinary construction rather than only a reused proposer instance.
            let mut analyzer = FomaAnalyzer::new(&g).expect("fixture compiles");
            let outcomes = analyzer.analyze_words_with_threads(&words, threads);
            assert_eq!(outcomes.len(), words.len());

            let mut per_word_dup_counts: Vec<Vec<u32>> = Vec::with_capacity(words.len());
            let mut semantic_cases = Vec::with_capacity(words.len());
            for (outcome, _elapsed) in &outcomes {
                let set = project_set(&g, &outcome.structured);
                per_word_dup_counts.push(set.entries().iter().map(|e| e.duplicate_count).collect());
                semantic_cases.push(set.to_semantic_value());
            }
            let semantic_digest = digest_projection(
                SEMANTIC_PROJECTION,
                &serde_json::json!({ "cases": semantic_cases }),
            )
            .expect("semantic projection is plain JSON, never fails to canonicalize");

            match &baseline {
                None => baseline = Some((per_word_dup_counts, semantic_digest)),
                Some((base_counts, base_digest)) => {
                    assert_eq!(
                        &per_word_dup_counts, base_counts,
                        "duplicate_count vectors differ at threads={threads} rep={rep} vs. the \
                         threads=1 rep=0 baseline -- duplicate multiplicity is NOT thread-count \
                         invariant"
                    );
                    assert_eq!(
                        &semantic_digest, base_digest,
                        "semanticDigest differs at threads={threads} rep={rep} vs. the \
                         threads=1 rep=0 baseline"
                    );
                }
            }
        }
    }

    // Every word must have contributed a duplicate somewhere, or the sweep above only proved that duplicate-free words stay duplicate-free.
    let (base_counts, _) = baseline.expect("at least one run happened");
    assert!(
        base_counts
            .iter()
            .any(|counts| counts.iter().any(|&c| c > 1)),
        "no word in this batch ever produced duplicate_count > 1 -- this run would be vacuous"
    );
}

/// Guards against the sweep above passing for the trivial reason that nothing ever raced: a rendezvous probe proves two confirmation tasks genuinely overlapped on separate threads.
#[test]
fn confirm_across_words_genuinely_overlaps_at_thread_count_above_one() {
    let g = load(DUP_ROOT_FIXTURE);
    let words: Vec<String> = (0..8).map(|_| "kax".to_string()).collect();

    let mut analyzer = FomaAnalyzer::new(&g).expect("fixture compiles");
    let confirmation_concurrency = analyzer.arm_confirmation_concurrency_probe();
    let outcomes = analyzer.analyze_words_with_threads(&words, 4);
    assert_eq!(outcomes.len(), words.len());
    let max_active = confirmation_concurrency.max_active();
    assert!(
        max_active > 1,
        "expected genuinely overlapping confirm tasks at thread_count=4, observed max \
         concurrently-active = {max_active} -- the determinism sweep above would not actually be \
         exercising cross-thread scheduling"
    );
}

/// The deadline is only a deadlock watchdog: success is the observed concurrency count, never elapsed time, and a broken probe fails explicitly rather than hanging.
fn observed_confirmation_concurrency_without_deadlock(
    word_count: usize,
    max_threads: usize,
) -> usize {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let g = load(DUP_ROOT_FIXTURE);
        let words: Vec<String> = (0..word_count).map(|_| "kax".to_string()).collect();
        let mut analyzer = FomaAnalyzer::new(&g).expect("fixture compiles");
        let confirmation_concurrency = analyzer.arm_confirmation_concurrency_probe();
        let outcomes = analyzer.analyze_words_with_threads(&words, max_threads);
        assert_eq!(outcomes.len(), words.len());
        sender
            .send(confirmation_concurrency.max_active())
            .expect("test receiver remains alive until the batch finishes");
    });

    let max_active = receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap_or_else(|_| {
            panic!(
                "confirmation concurrency probe deadlocked with word_count={word_count} \
                 max_threads={max_threads}"
            )
        });
    worker.join().expect("probe worker did not panic");
    max_active
}

#[test]
fn confirmation_probe_with_one_word_records_non_overlap_without_deadlock() {
    assert_eq!(observed_confirmation_concurrency_without_deadlock(1, 4), 1);
}

#[test]
fn confirmation_probe_with_one_worker_records_non_overlap_without_deadlock() {
    assert_eq!(observed_confirmation_concurrency_without_deadlock(8, 1), 1);
}
