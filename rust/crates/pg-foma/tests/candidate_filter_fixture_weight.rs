//! Per-pass filter fixtures: every declared pass has a fixture, and enforcing keeps what Off keeps.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use pg_conformance_fixtures::{assert_matches_oracle, FixtureRef, Root, WordsYaml};
use pg_foma::candidate_filter::test_support::filter_of;
use pg_foma::candidate_filter::{
    CandidateFilter, CandidateFilterPass, CandidateWitness, DeferredFactReason, FeatureSet,
    FilterBudget, FilterMode, LexicalOrigin, ProposalProducer, ProposalProvenance,
    ProposedCandidate, TraceFact, TraceUnit, WitnessId,
};
use pg_foma::tags::Candidate;
use pg_grammar::model::MorphemeId;
use pg_parse::identity::AnalysisIdentity;
use pg_parse::{Morpher, WordAnalysis};

/// Every pass the filter program declares, whether or not it is built yet.
const DECLARED_PASSES: &[&str] = &[
    "structural.ownership.v1",
    "structural.transition.v1",
    "symbolic.slot_order.v1",
    "symbolic.co_occurrence.v1",
    "symbolic.static_signature.v1",
    "symbolic.partner.v1",
    "local.allomorph.v1",
    "local.exact_span.v1",
    "local.environment.v1",
];

const AWAITING_PASS: &str = "awaiting-pass";
const WIRED: &str = "wired";
const NOT_YET_PROVOKABLE: &str = "not-yet-provokable";

struct Expectation {
    pass_id: String,
    min_fire_count: u64,
    status: String,
}

struct Fixture {
    name: String,
    dir: PathBuf,
    expectation: Expectation,
}

impl Fixture {
    fn has_grammar(&self) -> bool {
        self.dir.join("grammar.xml").is_file()
    }

    /// The shared fixture vocabulary, over a category the dual-root discovery does not walk.
    fn as_ref(&self) -> FixtureRef {
        FixtureRef {
            root: Root::Staging,
            category: "filter-passes".to_string(),
            name: self.name.clone(),
            dir: self.dir.clone(),
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn fixtures_root() -> PathBuf {
    repo_root()
        .join("conformance-staging")
        .join("filter-passes")
}

fn discover_fixtures() -> Vec<Fixture> {
    let root = fixtures_root();
    let read = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("{}: read filter-pass fixture root: {e}", root.display()));
    let mut dirs: Vec<PathBuf> = read
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();

    let mut out = Vec::new();
    for dir in dirs {
        let name = dir
            .file_name()
            .expect("a directory entry has a file name")
            .to_string_lossy()
            .into_owned();
        out.push(Fixture {
            expectation: load_expectation(&dir),
            name,
            dir,
        });
    }
    assert!(
        !out.is_empty(),
        "{}: no filter-pass fixtures discovered",
        root.display()
    );
    out
}

fn load_expectation(dir: &Path) -> Expectation {
    let path = dir.join("filter-expectation.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: read filter-expectation.json: {e}", path.display()));
    let value: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{}: parse filter-expectation.json: {e}", path.display()));
    let field = |key: &str| -> &serde_json::Value {
        value
            .get(key)
            .unwrap_or_else(|| panic!("{}: missing {key:?}", path.display()))
    };
    Expectation {
        pass_id: field("pass_id")
            .as_str()
            .unwrap_or_else(|| panic!("{}: pass_id must be a string", path.display()))
            .to_string(),
        min_fire_count: field("min_fire_count")
            .as_u64()
            .unwrap_or_else(|| panic!("{}: min_fire_count must be a whole number", path.display())),
        status: field("status")
            .as_str()
            .unwrap_or_else(|| panic!("{}: status must be a string", path.display()))
            .to_string(),
    }
}

/// The passes actually built, read as `StablePassId` literals off the pass sources themselves.
fn built_pass_ids() -> BTreeSet<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/candidate_filter/passes");
    let mut found = BTreeSet::new();
    let Ok(read) = std::fs::read_dir(&dir) else {
        return found;
    };
    for entry in read.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: read pass source: {e}", path.display()));
        for literal in string_literals(&text) {
            if looks_like_a_pass_id(literal) {
                found.insert(literal.to_string());
            }
        }
    }
    found
}

fn string_literals(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('"') else {
            break;
        };
        out.push(&after[..close]);
        rest = &after[close + 1..];
    }
    out
}

/// `<module>.<concept>.v<n>`, the shape every declared pass id uses.
fn looks_like_a_pass_id(literal: &str) -> bool {
    let parts: Vec<&str> = literal.split('.').collect();
    if parts.len() < 3 {
        return false;
    }
    let Some(version) = parts.last().and_then(|p| p.strip_prefix('v')) else {
        return false;
    };
    !version.is_empty()
        && version.bytes().all(|b| b.is_ascii_digit())
        && parts[..parts.len() - 1]
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
}

/// The pass list an enforced run uses, which is empty while no pass is built.
fn production_passes() -> Vec<Box<dyn CandidateFilterPass>> {
    Vec::new()
}

fn adapt(index: usize, analysis: &WordAnalysis) -> ProposedCandidate {
    let identity = Candidate {
        morphemes: analysis
            .morpheme_ids
            .iter()
            .copied()
            .map(MorphemeId)
            .collect(),
        root_index: analysis.root_morpheme_index,
    };
    let units = analysis
        .morpheme_ids
        .iter()
        .copied()
        .map(|id| TraceUnit {
            morpheme: MorphemeId(id),
            role: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
            allomorphs: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
            slot: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
            stratum: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
            surface_span: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
            local_events: TraceFact::Deferred(DeferredFactReason::ProducerDoesNotEmit),
        })
        .collect();
    let witness = CandidateWitness {
        witness_id: WitnessId(index as u64),
        lexical_origin: LexicalOrigin::StaticGrammar,
        lexicon_revision: 0,
        units,
        deferred: FeatureSet::empty(),
        provenance: ProposalProvenance {
            producer: ProposalProducer::SyntheticFixture,
            grammar_revision: 0,
        },
    };
    ProposedCandidate::new(identity, vec![witness]).expect("one witness forms a valid proposal")
}

fn survivors(filter: &CandidateFilter, mode: FilterMode, analyses: &[WordAnalysis]) -> Vec<usize> {
    let proposals: Vec<ProposedCandidate> = analyses
        .iter()
        .enumerate()
        .map(|(index, analysis)| adapt(index, analysis))
        .collect();
    let outcome = filter.filter(mode, proposals, FilterBudget::unlimited());
    let mut indices: Vec<usize> = outcome
        .retained
        .iter()
        .flat_map(|candidate| candidate.witnesses.iter())
        .map(|witness| witness.witness_id.0 as usize)
        .collect();
    indices.sort_unstable();
    indices
}

/// Multiset equality over full `WordAnalysis` values, removing one matched occurrence at a time.
fn assert_word_analysis_multiset_eq(label: &str, off: &[&WordAnalysis], enforce: &[&WordAnalysis]) {
    assert_eq!(
        off.len(),
        enforce.len(),
        "{label}: Off retained {} analyses, Enforce retained {}",
        off.len(),
        enforce.len()
    );
    let mut remaining: Vec<&WordAnalysis> = enforce.to_vec();
    for wanted in off {
        let found = remaining.iter().position(|held| held == wanted);
        let Some(position) = found else {
            panic!("{label}: Enforce lost an analysis Off retained: {wanted:?}");
        };
        remaining.swap_remove(position);
    }
}

/// Every fixture declares one well-formed expectation and carries the files its status implies.
#[test]
fn every_fixture_declares_a_well_formed_expectation() {
    let fixtures = discover_fixtures();
    let mut claimed: BTreeMap<String, String> = BTreeMap::new();
    for fixture in &fixtures {
        let expectation = &fixture.expectation;
        assert!(
            matches!(
                expectation.status.as_str(),
                AWAITING_PASS | WIRED | NOT_YET_PROVOKABLE
            ),
            "{}: status {:?} is not one of {AWAITING_PASS:?}, {WIRED:?}, {NOT_YET_PROVOKABLE:?}",
            fixture.name,
            expectation.status
        );
        assert!(
            DECLARED_PASSES.contains(&expectation.pass_id.as_str()),
            "{}: pass_id {:?} is not a declared pass; declared: {DECLARED_PASSES:?}",
            fixture.name,
            expectation.pass_id
        );
        if let Some(other) = claimed.insert(expectation.pass_id.clone(), fixture.name.clone()) {
            panic!(
                "{} and {other} both claim pass {:?}; one pass, one fixture",
                fixture.name, expectation.pass_id
            );
        }
        if expectation.status == NOT_YET_PROVOKABLE {
            assert!(
                !fixture.has_grammar(),
                "{}: a not-yet-provokable fixture must carry no grammar.xml -- if it can be \
                 authored, author it and set status to {AWAITING_PASS:?}",
                fixture.name
            );
            continue;
        }
        assert!(
            fixture.has_grammar() && fixture.dir.join("words.yaml").is_file(),
            "{}: status {:?} requires both grammar.xml and words.yaml",
            fixture.name,
            expectation.status
        );
    }
    eprintln!(
        "candidate_filter_fixture_weight: {} fixtures, {} declared passes",
        fixtures.len(),
        DECLARED_PASSES.len()
    );
}

/// Every transcribed signature is one this repo's engine actually produces for that grammar.
#[test]
fn every_fixture_matches_the_engine_it_was_transcribed_from() {
    let mut total_checked = 0usize;
    let mut checked_fixtures = 0usize;
    for fixture in discover_fixtures() {
        if !fixture.has_grammar() {
            continue;
        }
        let reference = fixture.as_ref();
        let words: WordsYaml = reference.load_words_yaml();
        let grammar = pg_grammar::load(&reference.load_grammar_xml())
            .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", reference.label()));
        let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);
        let checked = assert_matches_oracle(&reference.label(), &words, &morpher);
        assert!(checked > 0, "{}: replayed zero words", reference.label());
        total_checked += checked;
        checked_fixtures += 1;
    }
    assert!(
        checked_fixtures > 0 && total_checked > 0,
        "replayed {total_checked} words across {checked_fixtures} fixtures; a green run here has \
         to have replayed something"
    );
    eprintln!(
        "candidate_filter_fixture_weight: {total_checked} words replayed across \
         {checked_fixtures} fixtures"
    );
}

/// Enforced filtering returns exactly what bypassing it returns, per word, in both authorities.
#[test]
fn enforced_filtering_keeps_every_analysis_off_keeps() {
    let filter = filter_of(production_passes());
    let pass_ids = filter.pass_ids();
    let mut total_analyses = 0usize;
    let mut total_words = 0usize;
    for fixture in discover_fixtures() {
        if !fixture.has_grammar() {
            continue;
        }
        let reference = fixture.as_ref();
        let words: WordsYaml = reference.load_words_yaml();
        let grammar = pg_grammar::load(&reference.load_grammar_xml())
            .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", reference.label()));
        let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);
        for entry in &words.words {
            let outcome = morpher.parse_word(&entry.word);
            let analyses = &outcome.structured;
            let label = format!("{} word {:?}", reference.label(), entry.word);

            let off = survivors(&filter, FilterMode::Off, analyses);
            let enforce = survivors(&filter, FilterMode::Enforce, analyses);

            let project = |indices: &[usize]| -> BTreeSet<AnalysisIdentity> {
                indices
                    .iter()
                    .map(|&index| {
                        AnalysisIdentity::project(&analyses[index], &grammar).unwrap_or_else(|e| {
                            panic!("{label}: analysis {index} has no stable identity: {e}")
                        })
                    })
                    .collect()
            };
            assert_eq!(
                project(&off),
                project(&enforce),
                "{label}: Off and Enforce disagree on the deduplicated identity set"
            );
            let held = |indices: &[usize]| -> Vec<&WordAnalysis> {
                indices.iter().map(|&index| &analyses[index]).collect()
            };
            assert_word_analysis_multiset_eq(&label, &held(&off), &held(&enforce));

            total_analyses += analyses.len();
            total_words += 1;
        }
    }
    assert!(
        total_words > 0 && total_analyses > 0,
        "compared {total_analyses} analyses over {total_words} words; a green run here has to \
         have filtered something"
    );
    eprintln!(
        "candidate_filter_fixture_weight: {total_analyses} analyses over {total_words} words \
         survived Off/Enforce comparison against {} enforced pass(es)",
        pass_ids.len()
    );
}

/// A wired fixture's pass must reject at least as often as that fixture claims.
#[test]
fn a_wired_fixture_reaches_its_declared_fire_count() {
    let filter = filter_of(production_passes());
    let mut wired = 0usize;
    for fixture in discover_fixtures() {
        if fixture.expectation.status != WIRED {
            continue;
        }
        wired += 1;
        let reference = fixture.as_ref();
        let words: WordsYaml = reference.load_words_yaml();
        let grammar = pg_grammar::load(&reference.load_grammar_xml())
            .unwrap_or_else(|e| panic!("{}: grammar failed to load: {e}", reference.label()));
        let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);
        let mut rejections = 0u64;
        for entry in &words.words {
            let analyses = morpher.parse_word(&entry.word).structured;
            let proposals: Vec<ProposedCandidate> = analyses
                .iter()
                .enumerate()
                .map(|(index, analysis)| adapt(index, analysis))
                .collect();
            let outcome = filter.filter(FilterMode::Enforce, proposals, FilterBudget::unlimited());
            rejections += outcome
                .report
                .per_pass
                .iter()
                .filter(|(id, _)| id.as_str() == fixture.expectation.pass_id)
                .map(|(_, counters)| counters.rejections)
                .sum::<u64>();
        }
        assert!(
            rejections >= fixture.expectation.min_fire_count,
            "{}: pass {} produced {rejections} verified rejections, below the declared floor of {}",
            fixture.name,
            fixture.expectation.pass_id,
            fixture.expectation.min_fire_count
        );
    }
    eprintln!("candidate_filter_fixture_weight: {wired} wired fixture(s) checked for fire count");
}

/// A built pass with a waiting fixture, an undeclared pass id, or a fixtureless pass each fail.
#[test]
fn no_fixture_or_pass_rots_out_of_sync() {
    let fixtures = discover_fixtures();
    let built = built_pass_ids();

    for fixture in &fixtures {
        let expectation = &fixture.expectation;
        let is_built = built.contains(&expectation.pass_id);
        if expectation.status == AWAITING_PASS && is_built {
            panic!(
                "{}: pass {} is now built, so this fixture is no longer waiting -- set status to \
                 {WIRED:?}, set the measured min_fire_count, and let this gate enforce it",
                fixture.name, expectation.pass_id
            );
        }
        if expectation.status == NOT_YET_PROVOKABLE && is_built {
            panic!(
                "{}: pass {} is now built, so re-check whether a grammar can provoke it -- a \
                 not-yet-provokable fixture asserts nothing about a pass that exists",
                fixture.name, expectation.pass_id
            );
        }
        assert!(
            is_built || DECLARED_PASSES.contains(&expectation.pass_id.as_str()),
            "{}: pass {} is neither built nor declared",
            fixture.name,
            expectation.pass_id
        );
    }

    let claimed: BTreeSet<&str> = fixtures
        .iter()
        .map(|fixture| fixture.expectation.pass_id.as_str())
        .collect();
    let orphans: Vec<&String> = built
        .iter()
        .filter(|id| !claimed.contains(id.as_str()))
        .collect();
    assert!(
        orphans.is_empty(),
        "built pass(es) with no fixture claiming them: {orphans:?} -- a pass earns its place on a \
         fixture, so add one under conformance-staging/filter-passes/"
    );

    eprintln!(
        "candidate_filter_fixture_weight: {} pass(es) built, {} fixture(s) claiming a pass",
        built.len(),
        claimed.len()
    );
}
