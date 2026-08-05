//! **Group B**: six of the eleven orthogonal-basis mechanisms, exercised at least twice where an
//! honest second exercise exists.
//!
//! Group A (a disjoint file) owns template order/co-occurrence, cascade/strata, lexical class,
//! allomorph priority, and zero morphology. This file owns exactly six:
//!
//! | Mechanism | Exercise 1 | Exercise 2 | Second exercise honest? |
//! |---|---|---|---|
//! | Compounding | `staging:edge-cases/compounding-non-recursive` | `staging:edge-cases/recursive-endocentric-compounding` | yes |
//! | Interdigitation | `staging:edge-cases/infix-interdigitation` | `machine:languages/templatic-root-modification` | yes |
//! | Bounded metathesis | `staging:edge-cases/right-to-left-metathesis-reversal` | `staging:edge-cases/multi-table-metathesis-shared-representation` | yes |
//! | Feature/POS/MPR gates | `machine:edge-cases/mpr-gated-exception` | `machine:edge-cases/subrule-morphosyntactic-gating` | PARTLY -- see "The gate pair is only half-independent" |
//! | Unbounded peeled copy | `machine:languages/suffixing-extension-slot-ordering` | `machine:languages/metathesis-phase-isolation` | yes |
//! | Bounded copy | `machine:languages/metathesis-phase-isolation` (word level) | the same fixture at the MODEL level | NO second fixture exists -- see "Bounded copy has exactly one fixture" |
//!
//! Every exercise is an ALREADY-COMMITTED conformance fixture and every expected number an
//! assertion compares against is READ OUT of that fixture's own committed `words.yaml`. Nothing
//! here hand-derives a signature, an analysis count, or a multiplicity, so this gate cannot certify
//! a claim its own author invented. That discipline is `morphotactics_boundary_cleanup_slice.rs`'s
//! own, which this file follows deliberately rather than inventing a second convention.
//!
//! # Which relation each assertion uses, named at every site
//!
//! This matters enough to spell out, because a relation chosen for convenience is how the v1
//! certification scope was once made invisible (`pg_foma::parity`'s own module doc, and the
//! fix that restored it). Three distinct relations appear below:
//!
//! - **Deduplicated [`pg_foma::parity::OccurrenceIdentities`] SET cardinality**
//!   ([`OccurrenceIdentities::len`]) -- the PROGRAM's own parity relation, counted. Multiplicity is
//!   deliberately not part of it. Every per-word claim about "how many distinct analyses exist" is
//!   this relation, bounded above and below by the committed record in [`assert_word_parity`] and
//!   pinned exactly in [`assert_identities_and_multiplicity`].
//! - **MULTISET cardinality** ([`OccurrenceIdentities::raw_analyses`]) against the committed
//!   `parses:` row count. `words.yaml` is documented as sorted-but-NOT-deduped
//!   (`pg_parse::result_signature`, `WordEntry::expected_signature`), so a repeated signature there
//!   is a real, measured multiplicity, not a formatting artifact. Asserted for every committed word
//!   of every exercise, by [`assert_word_parity`], and again per witness word by
//!   [`assert_identities_and_multiplicity`] -- so "one identity found twice" can never pass as "two
//!   identities".
//! - **The committed SIGNATURE relation** (`pg_conformance_fixtures::assert_matches_oracle`) --
//!   status plus sorted-joined signature string, the shared ground truth every conformance fixture
//!   already goes through. Used as the anchor each exercise opens with, so the identity work below
//!   is a REFINEMENT of existing ground truth rather than a second, independently-drifting one.
//!
//! Two relations are deliberately NOT used, and saying which is part of naming the ones that are.
//! `OccurrenceIdentities::same_identities` (set EQUALITY between two occurrence sets) has no use
//! here: every exercise compares one engine run against a committed record, never two engine runs
//! against each other, so there is no second set for it to equal. And full
//! `pg_parse::WordAnalysis` equality would make engine internals (`syn_fs`, `mpr`, dense ordinals)
//! observable as disagreement -- a too-strong relation is the mirror image of the too-weak one that
//! hid the v1 scope.
//!
//! Where an exercise needs two witnesses to be genuinely two, it compares MORPHEME SEQUENCES
//! ([`morpheme_sequences`], the `morphemes` field of each identity) and requires the two sets to be
//! disjoint. Identity counts alone would not do it: two surfaces could each carry one analysis and
//! still be the same analysis, which would make a pair of witnesses one witness with two names.
//!
//! # The bounded/unbounded copy distinction, made structural rather than asserted
//!
//! 7.8's wording lists "bounded copy" and "unbounded peeled copy" as two mechanisms without saying
//! where the line is. Left as a label it would be unfalsifiable, so this file pins it to a property
//! of the loaded grammar that [`copy_width_bound`] computes:
//!
//! - A **bounded copy** copies an LHS part whose pattern has a FINITE width bound -- e.g.
//!   `metathesis-phase-isolation`'s `mrRedupCV`, whose copied part `rcCV` is exactly two
//!   `SimpleContext` nodes (one consonant, one vowel). A finite-width copy is a finite relation over
//!   the alphabet, so a composed FST can express it outright.
//! - An **unbounded copy** copies a part with NO finite width bound -- an
//!   `<OptionalSegmentSequence min="1" max="-1">`, i.e. a [`PatternNode::Quantifier`] whose `max` is
//!   `None`. `{ww : w in Sigma*}` is not a regular language, so no finite-state
//!   relation expresses it and the surface must be PEELED at query time
//!   (`pg_foma::peel::ReduplicationPeeler`) instead. This is why the peel path exists at all, and
//!   why it -- and only it -- carries a chain-depth budget.
//!
//! [`the_bounded_unbounded_copy_line_is_a_property_of_the_grammar`] asserts that this distinction is
//! a real, computed fact about the two rules rather than a comment: `mrRedupCV`'s copied part must
//! have a finite bound and `mrRedupFull`'s must not.
//!
//! # A budget refusal must never read as a recall failure
//!
//! The peel is the one mechanism here with a chain-depth budget, and a refused peel yields a
//! TRUNCATED candidate set. Two rules follow, both observed:
//!
//! 1. **Every recall claim in this file is made under a budget whose `chain_depth_cap` is `None`**
//!    (unbounded), built by [`unbounded_budget`]. No assertion about which analyses exist can
//!    therefore be a disguised budget refusal.
//! 2. **The refusal is a typed `Err`, never `Ok(vec![])`.** `peel_candidates` returns
//!    `Result<Vec<Candidate>, ComposeError>`, so "I was refused" and "I looked and found nothing"
//!    are different values, and this file never collapses them:
//!    [`peel_residuals_offered`] returns `Result` and every caller matches on it.
//!    [`the_smallest_chain_depth_cap_never_refuses_a_single_layer_copy`] pins the other direction --
//!    a single-layer unbounded copy, which the peel genuinely supports, must NOT be refused even by
//!    the smallest configurable cap. A failure there is "the budget refuses a supported construct",
//!    a finding about the budget; it is not, and must not be reported as, a recall failure.
//!
//! No assertion in this file is a proposal-set ceiling, no proposal set is truncated, and no
//! assertion reads a clock: wall time is never an eligibility or certification input here.
//!
//! # Bounded copy has exactly one fixture, and that is a finding
//!
//! A census of every `grammar.xml` under both fixture roots for a doubled `CopyFromInput` of ONE
//! part (the shape `pg_foma::emit::classify_affix` reads as `Role::Reduplication`) finds five rules
//! in four distinct grammars. Four of the five copy an unbounded part
//! (`metathesis-phase-isolation`'s `mrRedupFull`, `suffixing-extension-slot-ordering`'s `mrRedup`,
//! `deletion-reduplication-exception-composite`'s `mrRedupFull`,
//! `circumfix-reduplication-precedence`'s `mrCircRedup`). Exactly ONE copies a fixed-width part:
//! `metathesis-phase-isolation`'s `mrRedupCV`. Its apparent second home,
//! `staging:edge-cases/recipe-ordered-generic`, is a byte-identical CLONE of that upstream grammar
//! differing only in `<Language><Name>` -- pinned by
//! [`clone_fixtures_are_pinned_as_clones_not_independent_exercises`] precisely so nobody pairs the
//! two and reports one exercise as two.
//!
//! So there is no second bounded-copy FIXTURE in the corpus, and authoring one was out of scope for
//! this task: a new fixture's `words.yaml` must be transcribed from a real oracle run
//! (`.claude/skills/conformance-grammars/SKILL.md`, and every existing fixture's own header), and a
//! hand-derived expectation would pin this file's arithmetic instead of the grammar -- exactly what
//! the discipline above exists to prevent. What is offered instead is two exercises of the ONE
//! fixture at two different LAYERS, with genuinely independent falsifiers:
//! [`bounded_copy_exercise_fixed_width_reduplicant_recalls_exactly_one_reading`] (word level,
//! oracle-anchored: a synthesis/analysis defect in the fixed-width copy fails it while the model
//! stays intact) and [`the_bounded_unbounded_copy_line_is_a_property_of_the_grammar`] (model level:
//! a loader change that collapsed `OptionalSegmentSequence`'s `min`/`max` fails it while both words
//! could still parse). That is weaker than two fixtures and is reported as weaker.
//!
//! # The gate pair is only half-independent
//!
//! `mpr-gated-exception` and `subrule-morphosyntactic-gating` are independent in the MPR direction
//! and that half is ASSERTED, not argued: `subrule-morphosyntactic-gating` declares no MPR features
//! at all ([`gate_exercise_pos_requirement_on_a_rewrite_subrule`] checks
//! `Grammar::mpr_names.is_empty()`), so no regression in `excluded_mpr` handling can be detected
//! there and `mpr-gated-exception` is the only witness for it.
//!
//! The POS direction is weaker, and saying so is the point. `mpr-gated-exception` ALSO carries
//! subrule-level `requiredPartsOfSpeech="posNasal"` on three of its rewrite subrules, so the claim
//! "only exercise 2 can fail on subrule POS gating" is FALSE as stated. What is genuinely unique to
//! exercise 2 is the CONTRASTIVE witness: `pat` and `bat` present the identical phonological
//! environment ("p" before "a") and differ only in derivation state, so the gate's licensing is the
//! only thing that can explain the difference. `mpr-gated-exception` has no such contrastive pair --
//! its `posNasal` subrules simply have no site to fire at in the non-`posNasal` words -- so a
//! regression that ignored `required_pos` would plausibly leave it green. "Plausibly" is as far as
//! this goes: it is an argument, not a measurement, and it is not asserted anywhere below.
//!
//! # Fixtures deliberately not visited
//!
//! This file names its fixtures and never sweeps, so it needs no exclusion list. For the record,
//! three fixtures are known-bad at this base and would have had to be excluded by an ANNOUNCED skip
//! (the pattern `parity_divergence_census.rs::ABORTING_FIXTURES` establishes) had it swept:
//! `machine:edge-cases/deep-optional-affix-nesting` and `staging:edge-cases/recipe-template-generic`
//! abort the whole test process (unbounded recursion in `evaluate_plans`/apply), and
//! `machine:edge-cases/loader-pattern-shapes` panics at `replace.rs:498` ("char table too large for
//! the PUA token scheme"). None of them carries any of this file's six mechanisms.
//!
//! Every fixture here is a synthetic construct-shaped probe; no identifier in this file names a
//! language, and each fixture is referred to by what it composes.

use std::collections::{BTreeMap, BTreeSet};

use pg_conformance_fixtures::{assert_matches_oracle, discover, FixtureRef, Root, WordsYaml};
use pg_foma::compose_budget::{ComposeBudget, ComposeError};
use pg_foma::parity::OccurrenceIdentities;
use pg_foma::peel::ReduplicationPeeler;
use pg_foma::tags::Candidate;
use pg_grammar::model::{
    AffixAllomorphDef, Dir, Grammar, MorphRuleDef, OutputAction, PartRef, Pattern, PatternNode,
    PhonRuleDef,
};
use pg_parse::identity::MorphemeKey;
use pg_parse::Morpher;

// ------------------------------------------------------------------------------------------------
// The exercises, named by what they compose rather than by any language.
// ------------------------------------------------------------------------------------------------

/// Compounding exercise 1: MPR gating of head and non-head at two different levels.
const COMPOUNDING_MPR_GATED: Fixture = Fixture::staged("compounding-non-recursive");
/// Compounding exercise 2: a rule whose own output re-enters its own input.
const COMPOUNDING_SELF_FEEDING: Fixture = Fixture::staged("recursive-endocentric-compounding");

/// Interdigitation exercise 1: literal segments inserted between two copies of a split root.
const INTERDIGITATION_INSERT_SEGMENTS: Fixture = Fixture::staged("infix-interdigitation");
/// Interdigitation exercise 2: a natural CLASS inserted into a split root, and a root-internal
/// modification with no affix material at all.
const INTERDIGITATION_MODIFY_INPUT: Fixture =
    Fixture::upstream("languages", "templatic-root-modification");

/// Bounded-metathesis exercise 1: right-to-left direction, multi-member switch classes, one table.
const METATHESIS_RIGHT_TO_LEFT: Fixture = Fixture::staged("right-to-left-metathesis-reversal");
/// Bounded-metathesis exercise 2: two tables declaring one spelling at misaligned raw indices.
const METATHESIS_ACROSS_TABLES: Fixture =
    Fixture::staged("multi-table-metathesis-shared-representation");

/// Gate exercise 1: an MPR-feature EXCLUSION on a morphological rule's own input.
const GATE_MPR_EXCLUSION: Fixture = Fixture::upstream("edge-cases", "mpr-gated-exception");
/// Gate exercise 2: a part-of-speech REQUIREMENT on a phonological rewrite SUBRULE.
const GATE_SUBRULE_POS: Fixture = Fixture::upstream("edge-cases", "subrule-morphosyntactic-gating");

/// Unbounded-peeled-copy exercise 1: the peel's SUFFIX scan (`redupMorphType="suffix"`).
const PEELED_COPY_SUFFIX_SCAN: Fixture =
    Fixture::upstream("languages", "suffixing-extension-slot-ordering");
/// Unbounded-peeled-copy exercise 2, and the bounded-copy exercise: the peel's PREFIX scan
/// (`redupMorphType="prefix"`), in the one grammar that carries a fixed-width copy alongside an
/// unbounded one -- which is what makes the boundedness line observable rather than asserted.
const COPY_BOUNDED_AND_UNBOUNDED: Fixture =
    Fixture::upstream("languages", "metathesis-phase-isolation");

/// Every exercise, in the order the table in this module's doc lists them. `COPY_BOUNDED_AND_
/// UNBOUNDED` appears once even though two mechanisms use it -- see
/// [`the_exercise_set_has_the_shape_it_claims`].
const EXERCISES: &[Fixture] = &[
    COMPOUNDING_MPR_GATED,
    COMPOUNDING_SELF_FEEDING,
    INTERDIGITATION_INSERT_SEGMENTS,
    INTERDIGITATION_MODIFY_INPUT,
    METATHESIS_RIGHT_TO_LEFT,
    METATHESIS_ACROSS_TABLES,
    GATE_MPR_EXCLUSION,
    GATE_SUBRULE_POS,
    PEELED_COPY_SUFFIX_SCAN,
    COPY_BOUNDED_AND_UNBOUNDED,
];

/// Fixture pairs where one is a clone of the other -- see
/// [`clone_fixtures_are_pinned_as_clones_not_independent_exercises`].
const CLONE_PAIRS: &[(Fixture, Fixture)] = &[
    (
        Fixture::staged("recipe-ordered-generic"),
        Fixture::upstream("languages", "metathesis-phase-isolation"),
    ),
    (
        Fixture::staged("recipe-gated-generic"),
        Fixture::upstream("edge-cases", "mpr-gated-exception"),
    ),
];

// ------------------------------------------------------------------------------------------------
// Fixture addressing. Both roots, because four of this file's six mechanisms have their strongest
// (or only) witness upstream.
// ------------------------------------------------------------------------------------------------

/// A fixture address, resolvable at either root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Fixture {
    root: FixtureRoot,
    category: &'static str,
    name: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FixtureRoot {
    Staging,
    Machine,
}

impl Fixture {
    const fn staged(name: &'static str) -> Self {
        Fixture {
            root: FixtureRoot::Staging,
            category: "edge-cases",
            name,
        }
    }

    const fn upstream(category: &'static str, name: &'static str) -> Self {
        Fixture {
            root: FixtureRoot::Machine,
            category,
            name,
        }
    }

    fn resolve(self) -> FixtureRef {
        let want = match self.root {
            FixtureRoot::Staging => Root::Staging,
            FixtureRoot::Machine => Root::Machine,
        };
        discover()
            .into_iter()
            .find(|fixture| {
                fixture.root == want
                    && fixture.category == self.category
                    && fixture.name == self.name
            })
            .unwrap_or_else(|| {
                panic!(
                    "missing fixture {:?}/{}/{} -- if this is an upstream fixture, the `machine` \
                     submodule may not be initialized (see rust/tools/conformance.ps1); a fixture \
                     that cannot be found must break loudly, never silently make an exercise \
                     vacuous",
                    self.root, self.category, self.name
                )
            })
    }

    /// The loaded grammar plus its committed word record, which is what every exercise starts from.
    fn open(self) -> (String, Grammar, WordsYaml) {
        let fixture = self.resolve();
        let label = fixture.label();
        let grammar = pg_grammar::load(&fixture.load_grammar_xml())
            .unwrap_or_else(|e| panic!("{label}: fixture failed to load: {e}"));
        let words = fixture.load_words_yaml();
        (label, grammar, words)
    }
}

// ------------------------------------------------------------------------------------------------
// The committed expectation record. Read from `words.yaml`; never computed from the engine.
//
// Deliberately the same shape 7.7 uses (`morphotactics_boundary_cleanup_slice.rs`), duplicated
// rather than shared: an integration test file is its own compiled crate, so there is no place to
// put a shared helper short of a new library, and 7.8's own instruction was to add exactly one file.
// ------------------------------------------------------------------------------------------------

/// One word's committed expectation. Both counts come out of the fixture's own `parses:` list, so
/// this struct cannot express a number its fixture did not already record.
#[derive(Debug)]
struct CommittedWord {
    word: String,
    /// Total `parses:` rows -- the MULTISET cardinality. `words.yaml` sorts but does not dedup, so
    /// a repeated signature here is a measured multiplicity.
    raw_parses: usize,
    /// Distinct MORPHEME-JOIN parts (the text before `|`) among those rows.
    ///
    /// A sound LOWER bound on the number of distinct `pg_parse::identity::AnalysisIdentity`s: two
    /// different morpheme joins are two different morpheme-key vectors, hence two different
    /// identities. The morpheme half is used rather than the whole signature precisely because two
    /// rows CAN differ only in their rendered surface, which would make a whole-signature count an
    /// unsound lower bound.
    distinct_morph_joins: usize,
}

impl CommittedWord {
    /// True when the committed record pins the distinct-identity count EXACTLY -- i.e. when its
    /// lower and upper bounds coincide, so every row is a distinct identity of multiplicity one.
    fn pins_exact_identity_count(&self) -> bool {
        self.distinct_morph_joins == self.raw_parses
    }
}

fn to_committed(word: &pg_conformance_fixtures::WordEntry) -> CommittedWord {
    let joins: BTreeSet<&str> = word
        .parses
        .iter()
        .map(|parse| parse.signature.split('|').next().unwrap_or(""))
        .collect();
    CommittedWord {
        word: word.word.clone(),
        raw_parses: word.parses.len(),
        distinct_morph_joins: joins.len(),
    }
}

/// Every adapter-visible, non-`expect_skip` committed word of a fixture.
///
/// `adapter_visible()` is `PROTOCOL.md` section 3's rule: a word carrying any `guess: true` parse is
/// invisible to the adapter contract `Morpher::parse_word` implements, so asserting on it would
/// compare against a record the engine was never asked to produce. `expect_skip` words raise
/// `InvalidShapeException` and have no analysis set at all.
fn committed_words(words: &WordsYaml) -> Vec<CommittedWord> {
    words
        .words
        .iter()
        .filter(|word| word.adapter_visible() && !word.expect_skip)
        .map(to_committed)
        .collect()
}

/// The committed records for a NAMED subset, asserting first that every requested word exists.
///
/// Used for the two large upstream `languages/` fixtures, whose remaining rows carry mechanisms
/// group A owns (stem-name selection, slot ordering, cascade feeding). Naming the subset keeps this
/// file's failures attributable to this file's own six mechanisms; whole-fixture SIGNATURE anchoring
/// for every discoverable fixture is already an always-on gate elsewhere
/// (`pg-parse/tests/conformance_fixtures_gate.rs::all_discovered_fixtures_match_oracle`), so
/// re-running it here would duplicate a cost and a failure surface without adding a claim.
fn committed_named(label: &str, words: &WordsYaml, wanted: &[&str]) -> Vec<CommittedWord> {
    assert!(
        !wanted.is_empty(),
        "{label}: a named subset must name at least one word"
    );
    wanted
        .iter()
        .map(|&want| {
            let entry = words
                .words
                .iter()
                .find(|word| word.word == want)
                .unwrap_or_else(|| {
                    panic!(
                        "{label}: committed word {want:?} is gone -- a renamed or removed word must \
                         break loudly rather than silently shrink this exercise"
                    )
                });
            assert!(
                entry.adapter_visible() && !entry.expect_skip,
                "{label}: word {want:?} is not adapter-visible, so no assertion here can compare \
                 against its record"
            );
            to_committed(entry)
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------
// The parity core: identity, root, multiplicity. Relation named at every assertion.
// ------------------------------------------------------------------------------------------------

/// Assert exact analysis/root/multiplicity parity for one word against its committed record.
///
/// Returns the projected occurrence so callers can make fixture-specific claims on top.
fn assert_word_parity(
    label: &str,
    grammar: &Grammar,
    morpher: &Morpher,
    expect: &CommittedWord,
) -> OccurrenceIdentities {
    let outcome = morpher.parse_word(&expect.word);
    assert!(
        !outcome.invalid_shape,
        "{label}: word {:?} unexpectedly failed to segment; a committed adapter-visible word must \
         have an analysis set (possibly empty)",
        expect.word
    );

    // A projection FAULT is an internal inconsistency, never a parity miss (`pg_foma::parity`'s
    // "Faults are not misses"). Panicking names it as such instead of letting it read as
    // disagreement.
    let occurrence =
        OccurrenceIdentities::project(&outcome.structured, grammar).unwrap_or_else(|e| {
            panic!(
                "{label}: word {:?} -- identity projection FAULTED (an engine inconsistency, not a \
                 parity miss): {e}",
                expect.word
            )
        });

    // -- MULTIPLICITY. The multiset relation, strictly stronger than the program's set-equality
    //    parity relation, which is what 7.8 asks for beyond it.
    assert_eq!(
        occurrence.raw_analyses() as usize,
        expect.raw_parses,
        "{label}: word {:?} -- MULTISET cardinality disagrees with the committed `parses:` row \
         count. Committed rows are sorted-but-not-deduped, so this count is a measured \
         multiplicity, not a formatting artifact. Observed identities: {:?}",
        expect.word,
        occurrence.entries()
    );

    // -- SET. Bounded above and below by the committed record only; where the two bounds meet, the
    //    distinct-identity count is pinned exactly and every multiplicity must be one.
    assert!(
        occurrence.len() >= expect.distinct_morph_joins,
        "{label}: word {:?} -- {} distinct identities is FEWER than the {} distinct morpheme joins \
         the fixture records; distinct morpheme joins are distinct identities by construction, so \
         an analysis was lost",
        expect.word,
        occurrence.len(),
        expect.distinct_morph_joins
    );
    assert!(
        occurrence.len() <= expect.raw_parses,
        "{label}: word {:?} -- {} distinct identities EXCEEDS the {} committed rows",
        expect.word,
        occurrence.len(),
        expect.raw_parses
    );
    if expect.pins_exact_identity_count() {
        assert_eq!(
            occurrence.len(),
            expect.raw_parses,
            "{label}: word {:?} -- the committed record pins exactly {} distinct identities",
            expect.word,
            expect.raw_parses
        );
        for entry in occurrence.entries() {
            assert_eq!(
                entry.duplicate_paths, 1,
                "{label}: word {:?} -- identity {:?} was reached by {} derivational paths, but the \
                 committed record pins one row per distinct identity",
                expect.word, entry.identity, entry.duplicate_paths
            );
        }
    }

    // -- ROOT. Well-formedness floor: a root position must index its own morpheme sequence. The
    //    root-position DISCRIMINATION claim belongs to 7.7's
    //    `root_index_discriminates_two_readings_of_one_surface`, which owns the only staged fixture
    //    whose two readings differ in nothing else; nothing here duplicates it.
    for entry in occurrence.entries() {
        let len = entry.identity.morphemes.len() as i32;
        assert!(
            entry.identity.root_index >= 0 && entry.identity.root_index < len,
            "{label}: word {:?} -- identity {:?} has root_index {} outside 0..{len}",
            expect.word,
            entry.identity,
            entry.identity.root_index
        );
    }

    // -- v1 certification scope (`pg_foma::parity`'s own): a guessed or supplied-root analysis is
    //    not evidence about the compiled grammar, so it must not reach a parity claim at all.
    assert!(
        !occurrence.any_guessed(),
        "{label}: word {:?} -- a guessed analysis reached a parity claim",
        expect.word
    );
    assert!(
        !occurrence.any_supplied_root(),
        "{label}: word {:?} -- a supplied-root analysis reached a parity claim",
        expect.word
    );

    occurrence
}

/// Anchor a whole fixture against its committed signature record, through the shared oracle replay
/// every conformance fixture already goes through.
///
/// Called first in every exercise over a small fixture: it is what makes the per-word identity work
/// a REFINEMENT of the existing ground truth rather than a second, independently-drifting one.
fn anchor_whole_fixture(label: &str, grammar: &Grammar, words: &WordsYaml) {
    let morpher = Morpher::new(grammar, usize::MAX).with_memo(true);
    let checked = assert_matches_oracle(label, words, &morpher);
    assert!(
        checked > 0,
        "{label}: replayed zero words -- a fixture that checks nothing is not an exercise"
    );
}

/// Every named word's occurrence set for one fixture, keyed by word.
fn occurrences_for(
    label: &str,
    grammar: &Grammar,
    expectations: &[CommittedWord],
) -> BTreeMap<String, OccurrenceIdentities> {
    assert!(
        !expectations.is_empty(),
        "{label}: no adapter-visible committed words -- an exercise that compares nothing is not \
         an exercise"
    );
    let morpher = Morpher::new(grammar, usize::MAX).with_memo(true);
    expectations
        .iter()
        .map(|expect| {
            (
                expect.word.clone(),
                assert_word_parity(label, grammar, &morpher, expect),
            )
        })
        .collect()
}

fn occurrence<'m>(
    label: &str,
    occurrences: &'m BTreeMap<String, OccurrenceIdentities>,
    word: &str,
) -> &'m OccurrenceIdentities {
    occurrences
        .get(word)
        .unwrap_or_else(|| panic!("{label}: the witness word {word:?} must be pinned"))
}

/// Assert one word's distinct-identity count AND its multiplicity, naming both relations.
fn assert_identities_and_multiplicity<'m>(
    label: &str,
    occurrences: &'m BTreeMap<String, OccurrenceIdentities>,
    word: &str,
    expect: usize,
    why: &str,
) -> &'m OccurrenceIdentities {
    let found = occurrence(label, occurrences, word);
    assert_eq!(
        found.len(),
        expect,
        "{label}: {word:?} -- SET relation: expected {expect} distinct identities ({why}); got {:?}",
        found.entries()
    );
    assert_eq!(
        found.raw_analyses() as usize,
        expect,
        "{label}: {word:?} -- MULTISET relation: expected {expect} analyses of multiplicity one \
         each, not one identity reached several times"
    );
    found
}

/// Assert a word has NO valid derivation at all, and that this is a determined empty set.
fn assert_no_derivation(
    label: &str,
    occurrences: &BTreeMap<String, OccurrenceIdentities>,
    word: &str,
    why: &str,
) {
    let found = occurrence(label, occurrences, word);
    assert!(
        found.is_empty(),
        "{label}: {word:?} has no valid derivation ({why}), so its identity SET must be empty; got \
         {:?}",
        found.entries()
    );
    assert_eq!(
        found.raw_analyses(),
        0,
        "{label}: {word:?} -- MULTISET relation: an empty identity set must also have zero raw \
         analyses"
    );
}

// ------------------------------------------------------------------------------------------------
// Structural helpers: width bounds, copied parts, rule lookup. All pure functions over the loaded
// model, so a claim about a grammar's SHAPE never depends on running the engine.
// ------------------------------------------------------------------------------------------------

/// A pattern's width bound in shape nodes, or `None` when it has none.
///
/// `None` propagates: an unbounded [`PatternNode::Quantifier`] (`max: None`, the loader's rendering
/// of `<OptionalSegmentSequence max="-1">`) makes the whole pattern unbounded, and so does an
/// unbounded child. Anchors contribute zero width because they are position constraints, not
/// material.
fn pattern_width_bound(pattern: &Pattern) -> Option<u32> {
    node_list_width_bound(&pattern.nodes)
}

fn node_list_width_bound(nodes: &[PatternNode]) -> Option<u32> {
    let mut total: u32 = 0;
    for node in nodes {
        total = total.checked_add(node_width_bound(node)?)?;
    }
    Some(total)
}

fn node_width_bound(node: &PatternNode) -> Option<u32> {
    match node {
        PatternNode::Context(_) | PatternNode::CharDef(_) => Some(1),
        PatternNode::Anchor(_) => Some(0),
        PatternNode::Segments { shape, .. } => u32::try_from(shape.shape.interior().count()).ok(),
        PatternNode::Quantifier { max, children, .. } => {
            let per_repeat = node_list_width_bound(children)?;
            (*max)?.checked_mul(per_repeat)
        }
    }
}

/// The LHS part indices an allomorph's RHS copies TWO OR MORE times -- the shape
/// `pg_foma::emit::classify_affix` reads as `Role::Reduplication`, computed here independently of
/// that function so the claim below is a cross-check rather than a restatement.
fn reduplicated_parts(allomorph: &AffixAllomorphDef) -> Vec<u16> {
    let mut counts: BTreeMap<u16, usize> = BTreeMap::new();
    for action in &allomorph.rhs {
        if let OutputAction::Copy(PartRef::Input(index)) = action {
            *counts.entry(*index).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .filter(|&(_, count)| count >= 2)
        .map(|(index, _)| index)
        .collect()
}

/// The width bound of the part a reduplication-shaped allomorph copies, by rule NAME.
///
/// `None` from the inner `Option` means "the copied part is UNBOUNDED"; a missing rule or a rule
/// that copies no part twice panics, because that means the fixture stopped carrying the construct
/// this file reasons about and the exercise would otherwise go quiet.
fn copy_width_bound(label: &str, grammar: &Grammar, rule_name: &str) -> Option<u32> {
    for def in &grammar.mrules {
        let MorphRuleDef::AffixProcess(rule) = def else {
            continue;
        };
        if rule.name.as_deref() != Some(rule_name) {
            continue;
        }
        for allomorph in &rule.allomorphs {
            let parts = reduplicated_parts(allomorph);
            if parts.is_empty() {
                continue;
            }
            assert_eq!(
                parts.len(),
                1,
                "{label}: rule {rule_name:?} copies {} different parts twice; this helper's \
                 'the copied part' phrasing assumes exactly one",
                parts.len()
            );
            let part = usize::from(parts[0]);
            let pattern = allomorph.lhs.get(part).unwrap_or_else(|| {
                panic!("{label}: rule {rule_name:?} copies part {part} but has no such LHS part")
            });
            return pattern_width_bound(pattern);
        }
        panic!(
            "{label}: rule {rule_name:?} exists but no allomorph copies one part twice -- the \
             construct this exercise is about is gone"
        );
    }
    panic!("{label}: no AffixProcess rule named {rule_name:?}");
}

// ------------------------------------------------------------------------------------------------
// Peel helpers. Every recall claim runs under an unbounded chain-depth budget, and a refusal is a
// typed `Err` that this file never collapses into "found nothing".
// ------------------------------------------------------------------------------------------------

/// A budget that can never trip, chain depth included (`chain_depth_cap: None`).
///
/// `ComposeBudget::unbounded()` is `#[cfg(test)] pub(crate)` and so invisible from an integration
/// test crate; `with_caps` leaves `chain_depth_cap` at `None` by construction (that constructor's
/// own doc), which is exactly the property every recall claim here needs.
fn unbounded_budget() -> ComposeBudget {
    ComposeBudget::with_caps(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        None,
    )
}

/// The residual strings the reduplication peel offers to a proposer for `word`, in first-seen order.
///
/// The `propose` closure returns no candidates on purpose: the claim being made is about the peel's
/// own SCAN -- which repeated span it recognized and what base it therefore asked about -- and that
/// is observable without a compiled FST. Returning `Err` is the peel's typed refusal; the caller
/// decides what it means, and this function never turns it into an empty list.
fn peel_residuals_offered(
    grammar: &Grammar,
    word: &str,
    budget: &ComposeBudget,
) -> Result<Vec<String>, ComposeError> {
    let peeler = ReduplicationPeeler::new(grammar);
    assert!(
        peeler.has_redup_rules(),
        "a peel exercise needs a grammar with at least one reduplication-classified rule"
    );
    let mut offered: Vec<String> = Vec::new();
    let mut propose = |residual: &str| -> Vec<Candidate> {
        if !offered.iter().any(|seen| seen == residual) {
            offered.push(residual.to_string());
        }
        Vec::new()
    };
    peeler.peel_candidates(grammar, word, budget, &mut propose)?;
    Ok(offered)
}

/// Assert the peel's scan recognizes an unbounded copy of `base` in `word`.
fn assert_peel_offers_base(label: &str, grammar: &Grammar, word: &str, base: &str) {
    let offered = peel_residuals_offered(grammar, word, &unbounded_budget()).unwrap_or_else(|e| {
        panic!(
            "{label}: the peel of {word:?} was REFUSED under an unbounded chain-depth budget, which \
             should be unreachable: {e}. Read this as NotDetermined -- it is a statement about the \
             budget wiring, never a recall failure for this word."
        )
    });
    assert!(
        offered.iter().any(|residual| residual == base),
        "{label}: the peel of {word:?} never offered the base {base:?} to a proposer, so its scan \
         did not recognize the unbounded copy at all; offered: {offered:?}"
    );
}

// ================================================================================================
// Compounding -- two exercises.
// ================================================================================================

/// Compounding exercise 1: MPR gating of the head and the non-head, at TWO levels that read the
/// same MPR groups DIFFERENTLY.
///
/// The load-bearing rows, all read out of the fixture's own `words.yaml`:
/// - `fasubel` is the positive witness -- the head carries only one member of the rule-LEVEL
///   `headProdRestrictionsMprFeatures` all-type group (admitted, because that field is read
///   group-UNAWARE) and BOTH members of the subrule's own `requiredMPRFeatures` group (admitted,
///   because that field is read group-AWARE).
/// - `tikubel` fails on exactly the difference between those two readings: its head carries one of
///   the two subrule-group members, which a flat overlap test would wrongly admit.
/// - `numobel` fails the rule-level gate outright (a head with no MPR features at all), proving
///   that gate restricts something rather than always admitting.
/// - `fasuzon` fails on part-of-speech unification rather than MPR, so the two gate families are
///   separately witnessed.
///
/// Independent of exercise 2 in a way this test ASSERTS rather than argues: this rule's `max_apps`
/// is 1, so no amount of recursion machinery is reachable here.
#[test]
fn compounding_exercise_mpr_gates_head_and_non_head_at_two_levels() {
    let (label, grammar, words) = COMPOUNDING_MPR_GATED.open();
    anchor_whole_fixture(&label, &grammar, &words);

    let compounding = compounding_rule(&label, &grammar);
    assert_eq!(
        compounding.max_apps, 1,
        "{label}: this exercise's independence from the self-feeding exercise rests on its rule \
         being single-application; `multipleApplication` has changed"
    );
    assert!(
        !compounding.head_prod_restrictions_mpr.is_empty(),
        "{label}: the RULE-level head MPR gate is what this exercise is about and it is now empty, \
         which would make every MPR assertion below vacuous"
    );
    assert!(
        compounding
            .subrules
            .iter()
            .any(|subrule| !subrule.required_mpr.is_empty()),
        "{label}: the SUBRULE-level MPR gate -- the group-AWARE half of the contrast -- is gone"
    );

    let expectations = committed_words(&words);
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "fasubel",
        1,
        "the one head/non-head pair satisfying both the group-unaware rule-level gate and the \
         group-aware subrule gate",
    );
    assert_no_derivation(
        &label,
        &occurrences,
        "tikubel",
        "the head carries only one member of the subrule's all-type MPR group, which the \
         group-AWARE reading correctly excludes and a flat overlap test would wrongly admit",
    );
    assert_no_derivation(
        &label,
        &occurrences,
        "numobel",
        "the head carries no MPR features at all, so the non-empty rule-level gate cannot match",
    );
    assert_no_derivation(
        &label,
        &occurrences,
        "fasuzon",
        "the non-head is MPR-licensed but disagrees with the rule's own nonHeadPartsOfSpeech, so \
         the gate that refuses it is syntactic rather than MPR",
    );
}

/// Compounding exercise 2: a rule whose own output part of speech re-enters its own input.
///
/// Depth 1 works twice over (`tevimafl`, `maflisra`, two different root pairs) and depth 2
/// (`tevimaflisra`) has ZERO analyses. That zero is the fixture's own committed, measured record and
/// this test pins it as such -- per its `words.yaml`, it is the confirm engine's independent
/// `AnalyzerConfig::max_stem_count` ceiling ("a word can be de-compounded once, never recursively
/// re-compounded"), not a claim that self-feeding compounding ought to be unreachable. When that
/// ceiling is raised the fixture's own record changes first and this test follows it, because every
/// number here is read from that record.
///
/// Independent of exercise 1 in a way this test ASSERTS: this grammar declares no MPR features at
/// all, so no regression in either MPR gate can be detected here, and `max_apps > 1` is the
/// structural licence exercise 1's rule does not have.
#[test]
fn compounding_exercise_self_feeding_depth_is_a_determined_refusal() {
    let (label, grammar, words) = COMPOUNDING_SELF_FEEDING.open();
    anchor_whole_fixture(&label, &grammar, &words);

    let compounding = compounding_rule(&label, &grammar);
    assert!(
        compounding.max_apps > 1,
        "{label}: self-feeding needs `multipleApplication` above the DTD default of 1; it is {}",
        compounding.max_apps
    );
    assert!(
        grammar.mpr_names.is_empty(),
        "{label}: this grammar must declare NO MPR features -- that is what makes it independent \
         of the MPR-gated compounding exercise, and it now declares {:?}",
        grammar.mpr_names
    );

    let expectations = committed_words(&words);
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    for depth_one in ["tevimafl", "maflisra"] {
        assert_identities_and_multiplicity(
            &label,
            &occurrences,
            depth_one,
            1,
            "one application of the self-feeding-CAPABLE rule still works normally",
        );
    }
    assert_no_derivation(
        &label,
        &occurrences,
        "tevimaflisra",
        "the confirm engine's own max_stem_count ceiling admits at most one split-off non-head, so \
         a second, recursive application is unreached -- the fixture's own committed record",
    );
}

fn compounding_rule<'g>(
    label: &str,
    grammar: &'g Grammar,
) -> &'g pg_grammar::model::CompoundingRuleDef {
    let mut found = grammar.mrules.iter().filter_map(|def| match def {
        MorphRuleDef::Compounding(rule) => Some(rule),
        _ => None,
    });
    let rule = found
        .next()
        .unwrap_or_else(|| panic!("{label}: a compounding exercise must have a CompoundingRule"));
    assert!(
        found.next().is_none(),
        "{label}: this exercise reasons about THE compounding rule; there is now more than one"
    );
    rule
}

// ================================================================================================
// Interdigitation -- two exercises.
// ================================================================================================

/// Interdigitation exercise 1: literal segments inserted BETWEEN two copies of a split root.
///
/// Both rules split the root into a fixed-width first part and an unbounded remainder and emit
/// `Copy(first) InsertSegments(literal) Copy(rest)`. Two independent markers (`kpfotab`, `kcvotab`)
/// on the same root prove the mechanism is the interdigitation, not one marker's accident. The bare
/// root `kotab` is the control that keeps "the rule fired" distinguishable from "the rule always
/// fires".
///
/// The falsifier is placement: an emitter that appended or prefixed the inserted material instead of
/// interleaving it at the split point loses both witnesses while every control stays green.
///
/// Independent of exercise 2 in a way this test ASSERTS: no rule in this grammar carries a
/// `ModifyFromInput`/`InsertSimpleContext` output action, so a regression in that family cannot be
/// detected here at all.
#[test]
fn interdigitation_exercise_literal_insert_between_split_root_copies() {
    let (label, grammar, words) = INTERDIGITATION_INSERT_SEGMENTS.open();
    anchor_whole_fixture(&label, &grammar, &words);

    let shapes = interdigitation_shapes(&grammar);
    assert!(
        shapes.insert_literal_between_copies >= 2,
        "{label}: this exercise needs at least two rules that insert literal segments BETWEEN two \
         copies of a split input; found {}",
        shapes.insert_literal_between_copies
    );
    assert_eq!(
        shapes.class_valued_actions, 0,
        "{label}: this grammar must carry NO ModifyFromInput/InsertSimpleContext action -- that is \
         the asserted half of its independence from the other interdigitation exercise"
    );

    let expectations = committed_words(&words);
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    for infixed in ["kpfotab", "kcvotab"] {
        assert_identities_and_multiplicity(
            &label,
            &occurrences,
            infixed,
            1,
            "one marker interleaved at the root's own split point",
        );
    }
    assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "kotab",
        1,
        "the bare root, so an always-firing rule is distinguishable from a correctly-firing one",
    );

    // The two markers must be two DIFFERENT morpheme sequences, not one identity found twice under
    // two spellings -- otherwise "two independent markers" would be one exercise with two names.
    let first = morpheme_sequences(occurrence(&label, &occurrences, "kpfotab"));
    let second = morpheme_sequences(occurrence(&label, &occurrences, "kcvotab"));
    assert!(
        first.is_disjoint(&second),
        "{label}: the two interdigitation markers resolved to the SAME morpheme sequence, so they \
         are not two independent witnesses"
    );
}

/// Interdigitation exercise 2: a natural CLASS inserted into a split root, and a root-internal
/// modification carrying no affix material at all.
///
/// `sapr` is `ModifyFromInput`/`InsertSimpleContext`: the root is split and a fully-specified vowel
/// CLASS -- not a literal shape -- is inserted between the parts. `qil` is the second half of the
/// same output-action family: the exponent is a change to the root's own medial vowel, with no
/// inserted material anywhere. `spr` and `qal` are the bare-root controls, and `samm` is the
/// negative control whose root shape is excluded from co-occurring with the rule at all, so
/// "interdigitation applies" stays distinguishable from "interdigitation always applies".
///
/// The falsifier is the output-action family itself, which exercise 1's grammar does not contain: a
/// regression in `InsertSimpleContext`'s class resolution, or in `ModifyFromInput`'s in-place
/// segment rewrite, is invisible to a grammar that only ever inserts literal segments.
///
/// Anchored on a NAMED subset rather than the whole fixture: its remaining rows carry stem-name
/// selection and cascade feeding/bleeding, which belong to group A's half of 7.8 (see
/// [`committed_named`] for why whole-fixture anchoring here would duplicate an existing gate).
#[test]
fn interdigitation_exercise_class_insert_and_root_internal_modification() {
    let (label, grammar, words) = INTERDIGITATION_MODIFY_INPUT.open();

    let shapes = interdigitation_shapes(&grammar);
    assert!(
        shapes.class_valued_actions >= 2,
        "{label}: this exercise is about the ModifyFromInput/InsertSimpleContext family and fewer \
         than two rules carry it; found {}",
        shapes.class_valued_actions
    );

    let expectations = committed_named(
        &label,
        &words,
        &["spr", "sapr", "smm", "samm", "qal", "qil"],
    );
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "sapr",
        1,
        "a vowel CLASS inserted between the root's split parts",
    );
    assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "qil",
        1,
        "the exponent is a change to the root's own medial vowel, with no inserted material",
    );
    for bare in ["spr", "smm", "qal"] {
        assert_identities_and_multiplicity(
            &label,
            &occurrences,
            bare,
            1,
            "a bare root, valid on its own -- so an always-applying rule is distinguishable",
        );
    }
    assert_no_derivation(
        &label,
        &occurrences,
        "samm",
        "this root's shape is excluded from co-occurring with the interdigitating rule, so the \
         mechanism is genuinely restricted rather than unconditional",
    );

    // `sapr` and `qil` must be two DIFFERENT morpheme sequences: one is root+affix-morpheme, the
    // other is root+a-modification-morpheme, and collapsing them would make this one witness.
    let inserted = morpheme_sequences(occurrence(&label, &occurrences, "sapr"));
    let modified = morpheme_sequences(occurrence(&label, &occurrences, "qil"));
    assert!(
        inserted.is_disjoint(&modified),
        "{label}: the class-insert and root-modification witnesses resolved to the same morpheme \
         sequence, so they are not two independent witnesses"
    );
}

/// The interdigitation-relevant RHS shapes a grammar carries.
#[derive(Debug, Default)]
struct InterdigitationShapes {
    /// Rules whose RHS is `Copy .. InsertSegments .. Copy` -- LITERAL material interleaved at a
    /// split point (exercise 1's family).
    insert_literal_between_copies: usize,
    /// Rules carrying a `ModifyFromInput` ([`OutputAction::Modify`]) or an `InsertSimpleContext`
    /// ([`OutputAction::InsertContext`]) action -- exercise 2's family. Counted together because
    /// they are the two halves of one output-action family (a natural CLASS as the exponent, rather
    /// than a literal shape) and both are absent from exercise 1's grammar.
    class_valued_actions: usize,
}

fn interdigitation_shapes(grammar: &Grammar) -> InterdigitationShapes {
    let mut shapes = InterdigitationShapes::default();
    for def in &grammar.mrules {
        let allomorphs = match def {
            MorphRuleDef::AffixProcess(rule) => &rule.allomorphs,
            MorphRuleDef::Realizational(rule) => &rule.allomorphs,
            MorphRuleDef::Compounding(_) => continue,
        };
        let mut interleaves = false;
        let mut class_valued = false;
        for allomorph in allomorphs {
            if rhs_inserts_between_copies(&allomorph.rhs) {
                interleaves = true;
            }
            if allomorph.rhs.iter().any(|action| {
                matches!(
                    action,
                    OutputAction::Modify(..) | OutputAction::InsertContext(..)
                )
            }) {
                class_valued = true;
            }
        }
        if interleaves {
            shapes.insert_literal_between_copies += 1;
        }
        if class_valued {
            shapes.class_valued_actions += 1;
        }
    }
    shapes
}

/// `Copy .. InsertSegments .. Copy`: some insert sits strictly between two copies.
fn rhs_inserts_between_copies(rhs: &[OutputAction]) -> bool {
    let first_copy = rhs
        .iter()
        .position(|action| matches!(action, OutputAction::Copy(_)));
    let last_copy = rhs
        .iter()
        .rposition(|action| matches!(action, OutputAction::Copy(_)));
    let (Some(first), Some(last)) = (first_copy, last_copy) else {
        return false;
    };
    if first >= last {
        return false;
    }
    rhs[first + 1..last]
        .iter()
        .any(|action| matches!(action, OutputAction::InsertSegments { .. }))
}

fn morpheme_sequences(occurrence: &OccurrenceIdentities) -> BTreeSet<Vec<MorphemeKey>> {
    occurrence
        .entries()
        .iter()
        .map(|entry| entry.identity.morphemes.clone())
        .collect()
}

// ================================================================================================
// Bounded metathesis -- two exercises.
//
// "Bounded" is asserted, not assumed: both rules' structural descriptions are required to have a
// finite width bound, so neither is a switch over an unbounded span.
// ================================================================================================

/// Bounded-metathesis exercise 1: RIGHT-TO-LEFT direction over multi-member switch classes, in a
/// single-table grammar.
///
/// The positives (`sq`, `tr`) are two different roots, so the decoys are a genuine
/// cross-contamination check rather than "no root has this shape". The raw underlying spellings
/// (`qs`, `rt`) must NOT surface, because metathesis is obligatory wherever its pattern matches --
/// that is what proves the rule fires at all rather than being vacuously inapplicable. The remaining
/// members of the switch classes' cross product (`sr`, `tq`) must not surface either, which is the
/// per-branch precision claim.
///
/// The falsifier: the right-to-left construction is a mirror-and-reverse of the left-to-right one,
/// so a defect in the mirroring, the switch-index remapping, or the reversal changes one of these
/// six answers. Exercise 2's rule is left-to-right and cannot detect any of it.
#[test]
fn bounded_metathesis_exercise_right_to_left_over_multi_member_classes() {
    let (label, grammar, words) = METATHESIS_RIGHT_TO_LEFT.open();
    anchor_whole_fixture(&label, &grammar, &words);

    let dir = assert_bounded_metathesis(&label, &grammar);
    assert_eq!(
        dir,
        Dir::RightToLeft,
        "{label}: this exercise's whole point is the right-to-left construction"
    );
    assert_eq!(
        grammar.char_tables.len(),
        1,
        "{label}: a single-table grammar is what makes this independent of the cross-table \
         exercise; it now has {} tables",
        grammar.char_tables.len()
    );

    let expectations = committed_words(&words);
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    for surfaced in ["sq", "tr"] {
        assert_identities_and_multiplicity(
            &label,
            &occurrences,
            surfaced,
            1,
            "a correctly-metathesized surface, one root, one analysis",
        );
    }
    for raw in ["qs", "rt"] {
        assert_no_derivation(
            &label,
            &occurrences,
            raw,
            "the raw underlying spelling; metathesis is obligatory wherever its pattern matches, so \
             this string is never a valid surface form",
        );
    }
    for decoy in ["sr", "tq"] {
        assert_no_derivation(
            &label,
            &occurrences,
            decoy,
            "a remaining member of the switch classes' cross product belonging to no root -- the \
             per-branch precision control",
        );
    }
}

/// Bounded-metathesis exercise 2: the SAME two-position switch across two character tables that
/// declare one spelling at deliberately MISALIGNED raw indices.
///
/// `xm` is the load-bearing row: its root is entered on the inner stratum's table while the
/// metathesis rule lives on the outer stratum's, and the two tables give the switched spellings
/// different raw indices. Recovering it requires both that the rule's classes resolve across tables
/// and that a physically relocated segment does not carry its origin table's index into the
/// surface-match gate. `xw` is the same-table positive control, so "cross-table works" stays
/// distinguishable from "metathesis works". The two raw spellings and the decoy segment are the
/// negatives.
///
/// The falsifier is entirely about table identity, which exercise 1's single-table grammar cannot
/// present. Conversely this rule is left-to-right, so it cannot detect exercise 1's mirror-and-reverse
/// construction -- asserted below, not argued.
#[test]
fn bounded_metathesis_exercise_across_misaligned_character_tables() {
    let (label, grammar, words) = METATHESIS_ACROSS_TABLES.open();
    anchor_whole_fixture(&label, &grammar, &words);

    let dir = assert_bounded_metathesis(&label, &grammar);
    assert_eq!(
        dir,
        Dir::LeftToRight,
        "{label}: this exercise must NOT be right-to-left, or it stops being independent of the \
         right-to-left exercise"
    );
    assert!(
        grammar.char_tables.len() >= 2,
        "{label}: the cross-table claim needs more than one character table; there are {}",
        grammar.char_tables.len()
    );

    let expectations = committed_words(&words);
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "xm",
        1,
        "the cross-table row: the root's own table and the rule's table give its switched spellings \
         different raw indices, and the analysis must survive that",
    );
    assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "xw",
        1,
        "the same-table control, so ordinary metathesis recall stays distinguishable from the \
         cross-table claim",
    );
    for raw in ["mx", "wx"] {
        assert_no_derivation(
            &label,
            &occurrences,
            raw,
            "the raw un-metathesized spelling; metathesis is obligatory",
        );
    }
    assert_no_derivation(
        &label,
        &occurrences,
        "z",
        "a decoy segment declared only to misalign the tables' raw indices, attached to no entry or \
         rule output",
    );
}

/// Assert the grammar has exactly one metathesis rule, that its structural description is
/// width-BOUNDED, and return its direction.
fn assert_bounded_metathesis(label: &str, grammar: &Grammar) -> Dir {
    let mut found = grammar.prules.iter().filter_map(|def| match def {
        PhonRuleDef::Metathesis(rule) => Some(rule),
        PhonRuleDef::Rewrite(_) => None,
    });
    let rule = found
        .next()
        .unwrap_or_else(|| panic!("{label}: a metathesis exercise must have a MetathesisRule"));
    assert!(
        found.next().is_none(),
        "{label}: these exercises reason about THE metathesis rule; there is now more than one"
    );
    let bound = pattern_width_bound(&rule.pattern).unwrap_or_else(|| {
        panic!(
            "{label}: this is a BOUNDED-metathesis exercise, but the rule's structural description \
             has no finite width bound -- an unbounded switch window is a different mechanism and \
             7.8 does not ask this file for it"
        )
    });
    assert!(
        bound >= 2,
        "{label}: a switch needs at least two positions; the width bound is {bound}"
    );
    rule.dir
}

// ================================================================================================
// Feature/POS/MPR gates -- two exercises (see the module doc for the half-independence caveat).
// ================================================================================================

/// Gate exercise 1: an MPR-feature EXCLUSION on a morphological rule's own input.
///
/// `vokadan` is the distinguishing row: its root carries the very MPR feature the suffixing rule's
/// input excludes, so it has no derivation at all. `sanitan` is the vacuous-satisfaction control
/// (same rule, a root with no MPR features, applies normally) and `vokadi` is the
/// exception-honoring positive (the same root under the one rule that carries no gate). Without both
/// controls, "the exclusion is honored" would be indistinguishable from "the rule never applies" and
/// from "the root can never be suffixed".
///
/// The falsifier: an engine that ignored `excludedMPRFeatures` accepts `vokadan`. Exercise 2's
/// grammar declares no MPR features whatsoever -- asserted there -- so it cannot detect this.
#[test]
fn gate_exercise_mpr_feature_exclusion_on_a_morphological_rule() {
    let (label, grammar, words) = GATE_MPR_EXCLUSION.open();
    anchor_whole_fixture(&label, &grammar, &words);

    assert!(
        grammar.mrules.iter().any(|def| match def {
            MorphRuleDef::AffixProcess(rule) => rule
                .allomorphs
                .iter()
                .any(|allomorph| !allomorph.excluded_mpr.is_empty()),
            _ => false,
        }),
        "{label}: no morphological rule carries an MPR EXCLUSION any more, so every assertion \
         below would be vacuous"
    );

    let expectations = committed_words(&words);
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    assert_no_derivation(
        &label,
        &occurrences,
        "vokadan",
        "the root carries exactly the MPR feature this rule's input excludes",
    );
    assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "sanitan",
        1,
        "the same rule on a root with no MPR features -- the exclusion is vacuously satisfied, so \
         the rule is not simply broken",
    );
    assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "vokadi",
        1,
        "the excluded root under the one rule carrying no gate -- so the root is suffixable and \
         only the gated rule refuses it",
    );
    assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "vokad",
        1,
        "the excluded root bare, valid on its own",
    );
}

/// Gate exercise 2: a part-of-speech REQUIREMENT on a phonological rewrite SUBRULE.
///
/// The pair is contrastive by construction: `pat` and `bat` present the IDENTICAL phonological
/// environment and differ only in whether a zero-exponence derivation has changed the word's part of
/// speech. In `pat` the subrule is unlicensed and the environment is left untouched; in `bat` it is
/// licensed and rewrites. Nothing but the gate's licensing can explain the difference, which is what
/// makes this a falsifier and not merely a true statement.
///
/// The independence half this test ASSERTS: this grammar declares no MPR features at all, so a
/// regression in MPR-exclusion handling cannot be detected here. The other half is argued only --
/// exercise 1 does carry subrule-level POS gates, just no contrastive pair -- and the module doc
/// states that limit rather than letting it be inferred.
#[test]
fn gate_exercise_pos_requirement_on_a_rewrite_subrule() {
    let (label, grammar, words) = GATE_SUBRULE_POS.open();
    anchor_whole_fixture(&label, &grammar, &words);

    assert!(
        grammar.mpr_names.is_empty(),
        "{label}: this grammar must declare NO MPR features -- that is the asserted half of its \
         independence from the MPR-exclusion exercise; it declares {:?}",
        grammar.mpr_names
    );
    assert!(
        grammar.prules.iter().any(|def| match def {
            PhonRuleDef::Rewrite(rule) => rule
                .subrules
                .iter()
                .any(|subrule| subrule.required_pos.is_some()),
            PhonRuleDef::Metathesis(_) => false,
        }),
        "{label}: no rewrite SUBRULE carries a part-of-speech requirement any more"
    );

    let expectations = committed_words(&words);
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    let ungated = assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "pat",
        1,
        "the gate is unlicensed, so the identical phonological environment is left untouched",
    );
    let gated = assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "bat",
        1,
        "a zero-exponence derivation licensed the subrule, which then rewrote the same environment",
    );

    // The contrast is only a contrast if the two surfaces resolve to DIFFERENT morpheme sequences:
    // the licensed one carries the derivation morpheme, the unlicensed one does not.
    let ungated_sequences = morpheme_sequences(ungated);
    let gated_sequences = morpheme_sequences(gated);
    assert!(
        ungated_sequences.is_disjoint(&gated_sequences),
        "{label}: the gated and ungated readings share a morpheme sequence, so the derivation state \
         that licenses the subrule is not observable and this pair proves nothing"
    );
    let longest_ungated = ungated_sequences
        .iter()
        .map(Vec::len)
        .max()
        .expect("the unlicensed reading has at least one identity, asserted just above");
    assert!(
        gated_sequences
            .iter()
            .all(|sequence| sequence.len() > longest_ungated),
        "{label}: the licensed reading must carry MORE morphemes than the unlicensed one -- the \
         derivation that flips the part of speech is itself a morpheme"
    );
}

// ================================================================================================
// Bounded copy -- one fixture, two layers. See the module doc for why there is no second fixture.
// ================================================================================================

/// Bounded-copy exercise, WORD level: a fixed-width reduplicant recalls exactly one reading, and
/// the unbounded copy of the same root in the same grammar recalls a DIFFERENT one.
///
/// The three rows are the whole point of using this fixture: one root, one bare control, and two
/// reduplications of it that differ only in whether the copied span is compile-time bounded. The
/// fixed-width copy copies the root's first consonant-vowel pair; the unbounded one copies the whole
/// stem. Each surface must have exactly one analysis, and their morpheme sequences must DIFFER --
/// which is the assertion a compiler that conflated the two rules cannot satisfy, in either
/// direction (treat the bounded copy as unbounded and the short surface loses its reading; treat the
/// unbounded copy as bounded and the long one does).
///
/// The falsifier this layer has and the model layer does not: a defect in synthesizing or
/// un-applying a fixed-width copy changes these counts while leaving the loaded model untouched.
#[test]
fn bounded_copy_exercise_fixed_width_reduplicant_recalls_exactly_one_reading() {
    let (label, grammar, words) = COPY_BOUNDED_AND_UNBOUNDED.open();
    anchor_whole_fixture(&label, &grammar, &words);

    let expectations = committed_named(&label, &words, &["tula", "tutula", "tulatula"]);
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    let bare = assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "tula",
        1,
        "the bare root, so neither reduplication reads as unconditional",
    );
    let bounded = assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "tutula",
        1,
        "the FIXED-WIDTH copy: only the root's first consonant-vowel pair is copied",
    );
    let unbounded = assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "tulatula",
        1,
        "the UNBOUNDED copy of the same root: the whole stem is copied",
    );

    let bare_sequences = morpheme_sequences(bare);
    let bounded_sequences = morpheme_sequences(bounded);
    let unbounded_sequences = morpheme_sequences(unbounded);
    assert!(
        bounded_sequences.is_disjoint(&unbounded_sequences),
        "{label}: the bounded and unbounded copies of one root resolved to the SAME morpheme \
         sequence, so the two rules are indistinguishable and neither mechanism is witnessed"
    );
    assert!(
        bare_sequences.is_disjoint(&bounded_sequences)
            && bare_sequences.is_disjoint(&unbounded_sequences),
        "{label}: a reduplicated surface shares its morpheme sequence with the bare root, so the \
         reduplication morpheme is not observable"
    );
}

/// Bounded-copy exercise, MODEL level: the bounded/unbounded line is a computed property of the
/// loaded grammar, not a label this file applied.
///
/// The fixed-width rule's copied part must have a finite width bound; the full-copy rule's must have
/// none. This is what makes "bounded copy" and "unbounded peeled copy" two mechanisms rather than
/// two names -- an unbounded copy is `{ww}`, which no finite-state relation expresses, which is why
/// the peel exists at all and why only it carries a chain-depth budget.
///
/// The falsifier this layer has and the word layer does not: a loader or model change that collapsed
/// `<OptionalSegmentSequence>`'s `min`/`max` into a fixed repetition -- or that dropped the
/// distinction from [`PatternNode::Quantifier`] -- fails here while both words could still parse
/// perfectly well.
///
/// The corpus-wide half is asserted too, because it is the reason this mechanism has only one
/// exercise: every OTHER reduplication-shaped rule under either fixture root copies an unbounded
/// part.
#[test]
fn the_bounded_unbounded_copy_line_is_a_property_of_the_grammar() {
    let (label, grammar, _words) = COPY_BOUNDED_AND_UNBOUNDED.open();

    let bounded = copy_width_bound(&label, &grammar, "redupCV").unwrap_or_else(|| {
        panic!(
            "{label}: the fixed-width reduplication rule's copied part must have a FINITE width \
             bound -- without that, this grammar has no bounded copy and 7.8's bounded-copy \
             mechanism has no exercise anywhere in the corpus"
        )
    });
    assert_eq!(
        bounded, 2,
        "{label}: the fixed-width reduplicant is one consonant plus one vowel, so its bound is 2"
    );

    assert!(
        copy_width_bound(&label, &grammar, "redupFull").is_none(),
        "{label}: the full-copy rule's copied part must be UNBOUNDED; if it acquired a finite bound \
         this grammar no longer contains the contrast that makes the two mechanisms distinguishable"
    );

    // Corpus-wide: exactly one bounded-copy rule exists, which is why the bounded-copy mechanism
    // gets one fixture and two layers rather than two fixtures. If a second one is ever authored,
    // this assertion is the place that says so.
    let mut bounded_rules: Vec<String> = Vec::new();
    let mut unbounded_rules: Vec<String> = Vec::new();
    for fixture in discover() {
        let Ok(candidate) = pg_grammar::load(&fixture.load_grammar_xml()) else {
            continue; // A grammar that will not load is another test's finding, not this one's.
        };
        for def in &candidate.mrules {
            let MorphRuleDef::AffixProcess(rule) = def else {
                continue;
            };
            for allomorph in &rule.allomorphs {
                let parts = reduplicated_parts(allomorph);
                if parts.len() != 1 {
                    continue;
                }
                let Some(pattern) = allomorph.lhs.get(usize::from(parts[0])) else {
                    continue;
                };
                let name = format!(
                    "{}:{}",
                    fixture.label(),
                    rule.name.as_deref().unwrap_or("<unnamed>")
                );
                if pattern_width_bound(pattern).is_some() {
                    bounded_rules.push(name);
                } else {
                    unbounded_rules.push(name);
                }
            }
        }
    }
    assert!(
        !unbounded_rules.is_empty(),
        "the corpus must contain at least one UNBOUNDED copy, or the peel mechanism has no exercise"
    );
    assert!(
        bounded_rules.len() <= 2,
        "more than two bounded-copy rules now exist ({bounded_rules:?}). Two was the CLONE pair \
         (see clone_fixtures_are_pinned_as_clones_not_independent_exercises); a genuine third means \
         the bounded-copy mechanism can finally have two INDEPENDENT fixtures, so add the second \
         exercise and delete this ceiling rather than raising it"
    );
    assert!(
        !bounded_rules.is_empty(),
        "no bounded-copy rule exists anywhere in the corpus, so 7.8's bounded-copy mechanism has \
         no exercise at all -- that is a gap to report, never to pass over"
    );
}

// ================================================================================================
// Unbounded peeled copy -- two exercises, one per scan direction.
// ================================================================================================

/// Unbounded-peeled-copy exercise 1: the peel's SUFFIX scan.
///
/// The copied part is unbounded (asserted), so no compiled net can express it and the surface must
/// be peeled. Two claims, at two levels:
/// - the peel's scan recognizes the repeated span and offers the base stem to a proposer;
/// - the oracle recalls exactly the committed analysis for the whole surface, at the committed
///   multiplicity.
///
/// The falsifier: this rule's reduplicant TRAILS the base, so the scan that must find it is the
/// suffix-copy scan. Exercise 2's reduplicant LEADS, so a defect in one scan direction leaves the
/// other green -- which is not hypothetical: the peel once appended unconditionally, and that was
/// correct only because the sole reduplication it had ever seen was a trailing copy.
///
/// Every peel call here runs under [`unbounded_budget`], so no assertion can be a disguised
/// chain-depth refusal.
#[test]
fn unbounded_peeled_copy_exercise_suffix_scan() {
    let (label, grammar, words) = PEELED_COPY_SUFFIX_SCAN.open();

    assert!(
        copy_width_bound(&label, &grammar, "redup").is_none(),
        "{label}: this exercise is about an UNBOUNDED copy; the copied part now has a finite width \
         bound, which would make it a bounded-copy exercise instead"
    );

    assert_peel_offers_base(&label, &grammar, "kimbiakimbia", "kimbia");

    let expectations = committed_named(&label, &words, &["kimbiakimbia"]);
    let occurrences = occurrences_for(&label, &grammar, &expectations);
    assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "kimbiakimbia",
        1,
        "the whole stem copied once, recalled at the committed multiplicity",
    );
}

/// Unbounded-peeled-copy exercise 2: the peel's PREFIX scan.
///
/// Same mechanism, opposite side: the reduplicant LEADS the base, so its morpheme must precede the
/// base's in surface order and the root position must shift accordingly. The scan that has to find
/// it is the prefix-copy scan, which exercise 1's grammar never reaches.
///
/// The root-position claim is asserted directly, because it is the specific thing the append-only
/// peel got wrong: the reduplicant morpheme must come FIRST, so the root cannot be at position 0.
#[test]
fn unbounded_peeled_copy_exercise_prefix_scan() {
    let (label, grammar, words) = COPY_BOUNDED_AND_UNBOUNDED.open();

    assert!(
        copy_width_bound(&label, &grammar, "redupFull").is_none(),
        "{label}: this exercise is about an UNBOUNDED copy"
    );

    assert_peel_offers_base(&label, &grammar, "tulatula", "tula");

    let expectations = committed_named(&label, &words, &["tulatula"]);
    let occurrences = occurrences_for(&label, &grammar, &expectations);
    let peeled = assert_identities_and_multiplicity(
        &label,
        &occurrences,
        "tulatula",
        1,
        "the whole stem copied once, with the reduplicant LEADING",
    );

    for entry in peeled.entries() {
        assert!(
            entry.identity.morphemes.len() >= 2,
            "{label}: a reduplicated analysis carries the base's morpheme and the reduplication \
             morpheme, so at least two; got {:?}",
            entry.identity
        );
        assert!(
            entry.identity.root_index > 0,
            "{label}: the reduplicant LEADS, so the reduplication morpheme is first and the root \
             cannot sit at position 0; root_index is {}",
            entry.identity.root_index
        );
    }
}

/// The smallest configurable chain-depth cap must never refuse a single-layer unbounded copy.
///
/// This is the other half of "a budget refusal must never read as a recall failure": every recall
/// claim above runs unbounded, and this pins that the budget does not refuse the constructs the peel
/// genuinely supports either. The cap gates a layer the peel is ABOUT TO USE, and a residual with no
/// further reduplication structure of its own never reaches that check, so one real layer must fit
/// under a cap of 1.
///
/// If this ever fails, the correct reading is "the chain-depth budget refuses a supported
/// single-layer construct" -- a finding about the budget's wiring. It is NOT a recall failure for
/// these words, and it must not be reported as one: nothing here asserts which analyses exist.
#[test]
fn the_smallest_chain_depth_cap_never_refuses_a_single_layer_copy() {
    let smallest = unbounded_budget().with_chain_depth_cap(1);
    for (fixture, word) in [
        (PEELED_COPY_SUFFIX_SCAN, "kimbiakimbia"),
        (COPY_BOUNDED_AND_UNBOUNDED, "tulatula"),
        (COPY_BOUNDED_AND_UNBOUNDED, "tutula"),
    ] {
        let (label, grammar, _words) = fixture.open();
        match peel_residuals_offered(&grammar, word, &smallest) {
            Ok(offered) => assert!(
                !offered.is_empty(),
                "{label}: the peel of {word:?} was not refused, but offered no residual at all, so \
                 the scan found nothing -- a different defect from a refusal, and equally real"
            ),
            Err(error) => panic!(
                "{label}: the chain-depth budget REFUSED the single-layer copy in {word:?} at the \
                 smallest cap: {error}. Read this as NotDetermined for {word:?} -- a refusal is a \
                 truncated candidate set, never evidence that this word has no analysis. The defect \
                 is that the cap gates a layer the peel supports."
            ),
        }
    }
}

// ================================================================================================
// Shape guards: the exercise set cannot quietly shrink, and clones cannot pass as independent.
// ================================================================================================

/// A guard against this file's coverage quietly shrinking.
///
/// 7.8 asks for at least two exercises of each mechanism where possible. If a future edit pointed
/// two exercise constants at one fixture, or dropped one, every other test here would still pass
/// while covering less than the module doc claims. This asserts the shape itself: ten DISTINCT
/// fixtures, every one resolvable and loadable and carrying ordinary signature ground truth, and
/// both fixture roots genuinely represented.
///
/// Ten addresses for eleven exercises: `COPY_BOUNDED_AND_UNBOUNDED` serves two mechanisms, because
/// one grammar holding both a bounded and an unbounded copy of the same root is exactly what makes
/// the boundedness line observable rather than asserted.
#[test]
fn the_exercise_set_has_the_shape_it_claims() {
    assert_eq!(EXERCISES.len(), 10, "ten exercise addresses");
    let distinct: BTreeSet<Fixture> = EXERCISES.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        EXERCISES.len(),
        "the exercise addresses must be all different; `COPY_BOUNDED_AND_UNBOUNDED` is listed once \
         even though two mechanisms use it"
    );

    let staged = EXERCISES
        .iter()
        .filter(|fixture| fixture.root == FixtureRoot::Staging)
        .count();
    let upstream = EXERCISES.len() - staged;
    assert_eq!(
        (staged, upstream),
        (5, 5),
        "four of this file's six mechanisms have their strongest (or only) witness upstream, so \
         both roots must be represented -- a change to this split means an exercise moved and its \
         independence argument needs re-checking"
    );

    for fixture in EXERCISES {
        let (label, grammar, words) = fixture.open();
        assert!(
            !words.words.is_empty(),
            "{label}: a fixture with no committed words cannot be an exercise"
        );
        assert!(
            words.skip_in_generic_replay().is_none(),
            "{label}: this fixture is now marked expect_crash/budget_ms, so it has no ordinary \
             signature ground truth and cannot anchor an exercise: {:?}",
            words.skip_in_generic_replay()
        );
        assert!(
            !grammar.char_tables.is_empty(),
            "{label}: a loaded grammar always has at least one character table"
        );
    }
}

/// Clone fixtures are pinned as clones, so nobody reports one exercise as two.
///
/// Two staged fixtures are byte-identical copies of upstream grammars, differing only in
/// `<Language><Name>`. That matters for 7.8 specifically: pairing a fixture with its own clone would
/// look like two exercises of a mechanism while sharing every possible defect, and both clones carry
/// mechanisms this file owns -- one of them is the ONLY bounded copy in the corpus.
///
/// If a clone is ever deliberately diverged, this test fails; the right response is to update the
/// list AND re-examine any "two exercises" pairing that leaned on it, not to delete the assertion.
#[test]
fn clone_fixtures_are_pinned_as_clones_not_independent_exercises() {
    for (clone, original) in CLONE_PAIRS {
        let clone_ref = clone.resolve();
        let original_ref = original.resolve();
        let clone_xml = clone_ref.load_grammar_xml();
        let original_xml = original_ref.load_grammar_xml();

        let clone_name = clone_ref.load_words_yaml().language;
        let original_name = original_ref.load_words_yaml().language;
        assert_ne!(
            clone_name,
            original_name,
            "{}: a clone must at least differ in its language name",
            clone_ref.label()
        );

        let normalize = |xml: &str, name: &str| {
            xml.replace(&format!("<Name>{name}</Name>"), "<Name>NORMALIZED</Name>")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(
            normalize(&clone_xml, &clone_name),
            normalize(&original_xml, &original_name),
            "{} is no longer a clone of {}. If that divergence is deliberate, update CLONE_PAIRS \
             AND re-check every 'two independent exercises' claim that involved either fixture -- \
             the two were previously identical modulo the language name, so any pairing across them \
             was one exercise with two names.",
            clone_ref.label(),
            original_ref.label()
        );
    }
}
