//! Task 1.12 (`openspec/changes/add-grammar-assessment`): "Verify duplicate-count determinism
//! under parallel batch; if nondeterministic, move duplicate counts out of the semantic
//! projection and record the finding."
//!
//! `AnalysisSet::to_semantic_value` (`pg-assess/src/set.rs`) folds each entry's `duplicate_count`
//! into the semantic projection, and `ReportDraft::semantic_value` (`pg-assess/src/report.rs`)
//! folds every case's `AnalysisSet` into `semanticDigest`. That rests on an assumption this test
//! actually checks rather than takes on faith: that a candidate's raw multiplicity (how many
//! matching analyses `pg_foma::confirm::confirm_batch` returns for one word) cannot depend on
//! which rayon worker thread happened to run that word's confirm step, nor on how many worker
//! threads existed at all.
//!
//! ## Why this was expected to hold (read, not just measured)
//! `FomaAnalyzer::analyze_words_with_threads` (`pg-foma/src/composite.rs`) parallelizes **only
//! across words** — `propose_words` runs the mutable foma proposal sequentially first (one `&mut
//! self.proposer` handle, single-threaded by construction), and `confirm_proposed_words_in_pool`
//! then runs each word's *already-built* candidate list through `confirm::confirm_batch` on a
//! dedicated rayon pool, one word per task. Per that module's own doc, this is sound because
//! `Morpher` is `Sync` with no field-level interior mutability (its one `RefCell`, the memo scope,
//! is created fresh inside each `parse_word_core_selected` call) and `RuleCache`/`pg_fst::Fst` are
//! plain immutable data — so two words' confirm calls touch no shared mutable state, and a given
//! word's own multiplicity is decided entirely by a single-threaded, sequential call into
//! `confirm_batch` for THAT word alone, run identically regardless of which thread executes it or
//! how many siblings run alongside it. `pg_parse`'s own batch layer (`pg-parse/src/batch.rs`,
//! `pg-parse/tests/batch_determinism.rs`) already carries an equivalent, gate-enforced claim one
//! level down (`Morpher::parse_word` batch-invariant regardless of `max_threads`); this test is
//! the pg-foma-level analogue of that same property, specifically for the field
//! (`duplicate_count`) that the assessment layer promotes into a digest.
//!
//! Internally `confirm_batch_impl` does use hash-keyed maps (`rustc_hash::FxHashMap`,
//! `std::collections::HashSet`/`HashMap` in `pg-fst`'s `distinct()`/nondeterministic traversal),
//! which raises the reasonable suspicion that hash-seed randomization could leak into output
//! order or content. Reading those sites (`pg-fst/src/traverse.rs`'s `distinct()`, `run_inner`'s
//! `traversed` visited-set) shows the hash is used only to bucket candidates for an O(1) lookup
//! before an exact equality check decides membership/survival — never to decide which survives on
//! its own — so a randomized hasher only changes bucket layout, never the deduped result or its
//! order. This test does not re-derive that argument formally; it measures the actual observable
//! behavior instead.
//!
//! ## The fixture
//! `machine/conformance/edge-cases/deep-optional-affix-nesting` was the suggested candidate (its
//! own `words.yaml` documents `xxxxxxk` producing C(12,6) = 924 analyses) but every one of those
//! analyses fires a *different subset* of the 12 optional prefix rules, so each carries a distinct
//! `AnalysisIdentity` (a different ordered morpheme list) — reading its full oracle table (924
//! distinct `rules:` lists, no two alike) confirms it produces zero `AnalysisSet` duplicates. It
//! is included below as `sanity_deep_optional_affix_nesting_produces_no_identity_duplicates` to
//! record that finding rather than silently discard it, exactly as task 1.12 asks.
//!
//! A genuine duplicate needs the SAME `(morphemes, root_index, category)` triple recovered more
//! than once. `pg-foma/src/confirm.rs`'s own doc names the real-language precedent this mirrors
//! synthetically: `lexical_lookup_filtered` (`pg-parse/src/morpher.rs`) builds one candidate word
//! PER ALLOMORPH of a matched lexical entry, all sharing that entry's single `MorphemeId` — so a
//! lexical entry with two allomorphs of the identical phonetic shape yields two independently
//! synthesized (and independently re-confirmed) `WordAnalysis` values with byte-identical
//! `morpheme_ids`/`root_morpheme_index`, i.e. one genuine `AnalysisIdentity` duplicate per extra
//! allomorph, with no phonological rule or affix involved at all. `DUP_ROOT_FIXTURE` below is
//! exactly that: a bare-root grammar whose one lexical entry has three allomorphs, all spelled
//! "kax" (a nonsense synthetic root — this repo bars real-language fixtures).

use pg_assess::digest::{digest_projection, SEMANTIC_PROJECTION};
use pg_assess::identity::AnalysisIdentity;
use pg_assess::set::AnalysisSet;
use pg_foma::composite::FomaAnalyzer;
use pg_grammar::model::Grammar;

/// Bare-root synthetic grammar: one lexical entry, THREE allomorphs of the identical shape
/// "kax". Every allomorph is tried independently by `lexical_lookup_filtered` and re-confirmed
/// independently by `confirm::confirm_batch`, so a bare-word parse of "kax" is expected to
/// recover three `WordAnalysis` copies sharing one `AnalysisIdentity` — three raw confirms
/// collapsing to `duplicate_count == 3` once run through `pg_assess::set::AnalysisSet`.
/// Never named after or modeled on any real language (family: none — this is a nonsense root).
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

/// Project every structured analysis in `outcome` to its `AnalysisSetEntry` list (one call site
/// so every run in this file builds the set identically).
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

/// Records the finding the task asks for explicitly: the suggested fixture does NOT exercise
/// duplicate multiplicity at all (every one of its C(12,6) analyses for "xxxxxxk" fires a
/// different subset of the 12 optional rules, hence a different `AnalysisIdentity`), so it cannot
/// be the vehicle for this determinism check. Self-skips if the (untracked-by-git-status but
/// checked-in) conformance submodule path is ever unavailable, matching this repo's own
/// sample-data self-skip convention.
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

    // k=2 leading x's: C(12,2) = 66 analyses, small enough to stay fast, large enough that a
    // spurious identity collision would be very unlikely to hide by chance alone.
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

/// Sanity check that `DUP_ROOT_FIXTURE` is not vacuous: a single sequential (single-threaded)
/// parse of its bare root really does recover 3 raw analyses collapsing to ONE identity with
/// `duplicate_count == 3`, before any batching or threading enters the picture at all.
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

/// The real load-bearing check (task 1.12): run the SAME word list through
/// `FomaAnalyzer::analyze_words_with_threads` many times, at several thread counts including 1
/// and >1, and assert that every word's `AnalysisSet` (identities AND duplicate counts) and the
/// semantic-projection digest built from them are byte-identical across every run and every
/// thread count.
#[test]
fn duplicate_counts_and_semantic_digest_are_thread_count_invariant() {
    let g = load(DUP_ROOT_FIXTURE);

    // A mixed batch: many copies of the duplicate-producing bare root, interleaved with an
    // unrelated unknown word (zero candidates, exercises the empty-outcome path too) -- enough
    // words that a rayon pool with >1 thread has genuine scheduling freedom, not just one task
    // per thread trivially.
    let mut words: Vec<String> = Vec::new();
    for i in 0..40 {
        words.push("kax".to_string());
        if i % 5 == 0 {
            words.push("zzzqxxxnonsense".to_string());
        }
    }

    let thread_counts = [1usize, 2, 4, 8];
    let repetitions = 15;

    // (per-word duplicate_count vectors, per-word identity_digest vectors, run's own semantic
    // digest) -- the first (threads=1, rep=0) run is the baseline every later run is compared to.
    let mut baseline: Option<(Vec<Vec<u32>>, String)> = None;

    for &threads in &thread_counts {
        for rep in 0..repetitions {
            // A fresh analyzer per run: proves the property holds for the ordinary construction
            // path, not just for one compiled proposer instance reused across calls.
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

    // Every word must actually have contributed a duplicate somewhere, or the sweep above only
    // proved that duplicate-free words stay duplicate-free (true but uninteresting).
    let (base_counts, _) = baseline.expect("at least one run happened");
    assert!(
        base_counts
            .iter()
            .any(|counts| counts.iter().any(|&c| c > 1)),
        "no word in this batch ever produced duplicate_count > 1 -- this run would be vacuous"
    );
}

/// Independent confirmation that the confirm-across-words stage genuinely overlaps on more than
/// one OS thread for this batch (not just "thread_count=N was requested but tasks never actually
/// ran concurrently") -- guards against the sweep above passing for the trivial reason that
/// nothing ever raced. Uses an analyzer-owned `test-concurrency-hook` probe: its first two
/// confirmation tasks rendezvous before confirming, proving overlap without a timing-based delay
/// and without allowing another concurrently-running test to consume this test's observation.
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

/// The deadline is only a deadlock watchdog: success is still the observed concurrency count,
/// never elapsed time. A broken blocking probe fails explicitly instead of hanging the test binary.
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
