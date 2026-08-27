//! Group B: six of the eleven orthogonal-basis mechanisms, exercised at least twice where an honest second exercise exists (Group A owns the other five).
//! Methodology, relation choices, and coverage census: docs/research/orthogonal-basis-group-b-methodology.md.

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

// The exercises, named by what they compose rather than by any language.

/// Compounding exercise 1: MPR gating of head and non-head at two different levels.
const COMPOUNDING_MPR_GATED: Fixture = Fixture::staged("compounding-non-recursive");
/// Compounding exercise 2: a rule whose own output re-enters its own input.
const COMPOUNDING_SELF_FEEDING: Fixture = Fixture::staged("recursive-endocentric-compounding");

/// Interdigitation exercise 1: literal segments inserted between two copies of a split root.
const INTERDIGITATION_INSERT_SEGMENTS: Fixture = Fixture::staged("infix-interdigitation");
/// Interdigitation exercise 2: a natural class inserted into a split root, and a root-internal modification with no affix material at all.
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
/// Unbounded-peeled-copy exercise 2 and the bounded-copy exercise: the one grammar carrying a fixed-width copy alongside an unbounded one.
const COPY_BOUNDED_AND_UNBOUNDED: Fixture =
    Fixture::upstream("languages", "metathesis-phase-isolation");

/// Every exercise; `COPY_BOUNDED_AND_UNBOUNDED` appears once even though two mechanisms use it.
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

/// Fixture pairs where one is a clone of the other.
const CLONE_PAIRS: &[(Fixture, Fixture)] = &[(
    Fixture::staged("backend-ordered-generic"),
    Fixture::upstream("languages", "metathesis-phase-isolation"),
)];

// Fixture addressing. Both roots, because four of this file's six mechanisms have their strongest (or only) witness upstream.

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

// The committed expectation record, read from `words.yaml`, never computed from the engine; deliberately duplicated from `morphotactics_boundary_cleanup_slice.rs` since integration tests are separate crates.

/// One word's committed expectation; both counts come from the fixture's own `parses:` list.
#[derive(Debug)]
struct CommittedWord {
    word: String,
    /// Total `parses:` rows, the multiset cardinality; `words.yaml` sorts but does not dedup.
    raw_parses: usize,
    /// Distinct morpheme-join parts (text before `|`); a sound lower bound on distinct identities, since two rows can differ only in rendered surface, unlike the whole signature.
    distinct_morph_joins: usize,
}

impl CommittedWord {
    /// True when the lower and upper bounds coincide, so every row is a distinct identity of multiplicity one.
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

/// Every adapter-visible, non-`expect_skip` committed word of a fixture (a `guess: true` parse is invisible to `Morpher::parse_word`'s adapter contract).
fn committed_words(words: &WordsYaml) -> Vec<CommittedWord> {
    words
        .words
        .iter()
        .filter(|word| word.adapter_visible() && !word.expect_skip)
        .map(to_committed)
        .collect()
}

/// The committed records for a named subset, asserting first that every requested word exists; keeps failures attributable to this file's own six mechanisms rather than Group A's.
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

// The parity core: identity, root, multiplicity. Relation named at every assertion.

/// Assert exact analysis/root/multiplicity parity for one word; returns the projected occurrence so callers can make fixture-specific claims on top.
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

    // A projection FAULT is an internal inconsistency, never a parity miss; panicking names it as such.
    let occurrence =
        OccurrenceIdentities::project(&outcome.structured, grammar).unwrap_or_else(|e| {
            panic!(
                "{label}: word {:?} -- identity projection FAULTED (an engine inconsistency, not a \
                 parity miss): {e}",
                expect.word
            )
        });

    // MULTIPLICITY: the multiset relation, strictly stronger than the program's set-equality parity relation.
    assert_eq!(
        occurrence.raw_analyses() as usize,
        expect.raw_parses,
        "{label}: word {:?} -- MULTISET cardinality disagrees with the committed `parses:` row \
         count. Committed rows are sorted-but-not-deduped, so this count is a measured \
         multiplicity, not a formatting artifact. Observed identities: {:?}",
        expect.word,
        occurrence.entries()
    );

    // SET: bounded above and below by the committed record; where the bounds meet, every multiplicity must be one.
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

    // ROOT: well-formedness floor only, that a root position must index its own morpheme sequence.
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

    // v1 certification scope: a guessed or supplied-root analysis is not evidence about the compiled grammar.
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

/// Anchor a whole fixture against its committed signature record; called first in every exercise so the per-word identity work is a refinement of existing ground truth, not a second one.
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

// Structural helpers: width bounds, copied parts, rule lookup — pure functions over the loaded model, so a shape claim never depends on running the engine.

/// A pattern's width bound in shape nodes, or `None` when unbounded; `None` propagates from any unbounded child, and anchors contribute zero width.
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

/// The LHS part indices an allomorph's RHS copies two or more times, computed independently of `pg_foma::emit::classify_affix` so the claim below is a cross-check.
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

/// The width bound of the part a reduplication-shaped allomorph copies, by rule name; a missing rule or one that copies nothing twice panics rather than going quiet.
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

// Peel helpers. Every recall claim runs under an unbounded chain-depth budget, and a refusal is a typed `Err` this file never collapses into "found nothing".

/// A budget that can never trip, chain depth included.
fn unbounded_budget() -> ComposeBudget {
    ComposeBudget::unbounded()
}

/// The residual strings the reduplication peel offers to a proposer for `word`, in first-seen order; the closure returns no candidates since the claim is about the scan, observable without a compiled FST.
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

// Compounding: two exercises.

/// Compounding exercise 1: MPR gating of the head and the non-head at two levels that read the same MPR groups differently (`fasubel`/`tikubel`/`numobel`/`fasuzon` witness each half); `max_apps == 1` keeps it independent of exercise 2's recursion.
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

/// Compounding exercise 2: a rule whose own output part of speech re-enters its own input; depth 2 has zero analyses per `AnalyzerConfig::max_stem_count`'s ceiling, not a claim self-feeding is unreachable.
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

// Interdigitation: two exercises.

/// Interdigitation exercise 1: literal segments inserted between two copies of a split root; two independent markers (`kpfotab`, `kcvotab`) prove the mechanism, not one marker's accident, and this grammar carries no `ModifyFromInput`/`InsertSimpleContext` action, keeping it independent of exercise 2.
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

    // The two markers must be two DIFFERENT morpheme sequences, or "two independent markers" would be one exercise with two names.
    let first = morpheme_sequences(occurrence(&label, &occurrences, "kpfotab"));
    let second = morpheme_sequences(occurrence(&label, &occurrences, "kcvotab"));
    assert!(
        first.is_disjoint(&second),
        "{label}: the two interdigitation markers resolved to the SAME morpheme sequence, so they \
         are not two independent witnesses"
    );
}

/// Interdigitation exercise 2: a natural class inserted into a split root (`sapr`) and a root-internal modification with no inserted material (`qil`); anchored on a named subset since the rest carries Group A's mechanisms.
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

    // `sapr` and `qil` must be two DIFFERENT morpheme sequences, or collapsing them would make this one witness.
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
    /// Rules whose RHS is `Copy .. InsertSegments .. Copy`: literal material interleaved at a split point (exercise 1's family).
    insert_literal_between_copies: usize,
    /// `ModifyFromInput`/`InsertSimpleContext` rules (exercise 2's family): a natural class as the exponent rather than a literal shape.
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

// Bounded metathesis: two exercises. "Bounded" is asserted, not assumed: both rules' structural descriptions must have a finite width bound.

/// Bounded-metathesis exercise 1: right-to-left direction over multi-member switch classes, in a single-table grammar; obligatory firing and per-branch precision are both checked via decoys.
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

/// Bounded-metathesis exercise 2: the same two-position switch across two character tables declaring one spelling at deliberately misaligned raw indices; `xm` is the load-bearing cross-table row, `xw` the same-table control.
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

/// Assert the grammar has exactly one metathesis rule with a width-bounded structural description, and return its direction.
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
             has no finite width bound -- an unbounded switch window is a different mechanism"
        )
    });
    assert!(
        bound >= 2,
        "{label}: a switch needs at least two positions; the width bound is {bound}"
    );
    rule.dir
}

// Feature/POS/MPR gates: two exercises, only half-independent (see docs/research/orthogonal-basis-group-b-methodology.md).

/// Gate exercise 1: an MPR-feature exclusion on a morphological rule's own input; `vokadan`/`sanitan`/`vokadi` together distinguish "excluded", "vacuously satisfied", and "exception-honoring".
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

/// Gate exercise 2: a POS requirement on a phonological rewrite subrule; `pat`/`bat` present the identical environment, differing only in a zero-exponence derivation, so only the gate's licensing explains the surface difference.
/// This grammar declares no MPR features, so it cannot detect an MPR-exclusion regression (the other independence half is argued only; see docs/research/orthogonal-basis-group-b-methodology.md).
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

    // The contrast is only a contrast if the two surfaces resolve to DIFFERENT morpheme sequences.
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

// Bounded copy: one fixture, two layers (see docs/research/orthogonal-basis-group-b-methodology.md for why there is no second fixture).

/// Bounded-copy exercise, WORD level: a fixed-width reduplicant and an unbounded copy of the same root in the same grammar must recall different, disjoint readings, one each.
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

/// Bounded-copy exercise, MODEL level: the bounded/unbounded line is a computed property of the loaded grammar (finite vs. no width bound on the copied part), not a label this file applied; also asserts corpus-wide that only one bounded-copy rule exists.
#[test]
fn the_bounded_unbounded_copy_line_is_a_property_of_the_grammar() {
    let (label, grammar, _words) = COPY_BOUNDED_AND_UNBOUNDED.open();

    let bounded = copy_width_bound(&label, &grammar, "redupCV").unwrap_or_else(|| {
        panic!(
            "{label}: the fixed-width reduplication rule's copied part must have a FINITE width \
             bound -- without that, this grammar has no bounded copy and the mechanism has no \
             exercise anywhere in the corpus"
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

    // Corpus-wide: exactly one bounded-copy rule exists; if a second is ever authored, this assertion is the place that says so.
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
        "no bounded-copy rule exists anywhere in the corpus, so the mechanism has no exercise at \
         all -- that is a gap to report, never to pass over"
    );
}

// Unbounded peeled copy: two exercises, one per scan direction.

/// Unbounded-peeled-copy exercise 1: the peel's suffix scan; this rule's reduplicant trails the base, so a defect in the suffix-copy scan specifically leaves the prefix-copy scan (exercise 2) green.
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

/// Unbounded-peeled-copy exercise 2: the peel's prefix scan; the reduplicant leads the base, so the root position must shift off 0, which is the specific thing an append-only peel gets wrong.
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

/// The smallest configurable chain-depth cap must never refuse a single-layer unbounded copy: a residual with no further reduplication structure never reaches the "about to use another layer" check, so one real layer must fit under a cap of 1.
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

// Shape guards: the exercise set cannot quietly shrink, and clones cannot pass as independent.

/// A guard against this file's coverage quietly shrinking: ten distinct, resolvable, loadable fixtures with ordinary signature ground truth, both fixture roots represented.
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

/// Prevents a staging clone from being counted as an independent exercise.
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
