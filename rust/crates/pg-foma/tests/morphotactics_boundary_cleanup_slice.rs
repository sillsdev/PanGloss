//! Task 7.7 of `openspec/changes/cleanup-and-recipe-parity`: the first
//! `Morphotactics -> BoundaryCleanup` vertical slice.
//!
//! # What a "vertical slice" is here
//! Two ends joined by one gate. The TOP end is the typed mechanism graph
//! ([`pg_foma::mechanism_provider::derive_mechanism_graph`] over
//! [`pg_foma::grammar_semantics::GrammarSemantics`]): a `Morphotactics` node, a terminal
//! `BoundaryCleanup` node, and a directed path from the first to the second. The BOTTOM end is what
//! the engine actually produces for that same grammar's own pinned words, projected through the
//! program's parity vocabulary ([`pg_foma::parity::OccurrenceIdentities`]). A gate that asserted
//! only the graph would be asserting a description of a pipeline; a gate that asserted only the
//! analyses would not have touched the slice. Both ends, one fixture at a time, is the slice.
//!
//! # The four exercises, and why they are four rather than two pairs of names
//! Every exercise is an ALREADY-COMMITTED staged conformance fixture whose `words.yaml` was
//! measured against `pg_parse::Morpher` by the person who authored it (see each fixture's own
//! `STAGING.md`). Nothing in this file hand-derives a signature, an analysis count, or a
//! multiplicity: every number an assertion compares against is READ OUT of the committed
//! `words.yaml`, so this gate cannot certify a claim its own author invented.
//!
//! | Exercise | Fixture | What only IT can fail on |
//! |---|---|---|
//! | Template 1 | `template-category-sharing` | cross-template OVER-generation: two structurally impossible mixes (`pakolola`, `takolosa`) must have empty identity sets |
//! | Template 2 | `optional-template-composite` | zero-exponence UNDER-generation: a mandatory-but-silent slot must contribute a SECOND distinct identity for a surface-identical word (`monu`) |
//! | Cleanup 1 | `recipe-strata-generic` | a boundary PRODUCED by morphotactics (the compounding seam) survives to a terminal cleanup; that grammar has no boundary-consuming phonological rule at all |
//! | Cleanup 2 | `recipe-ordered-generic` | a boundary CONSUMED by ordered phonology (`mrComplexMeta`'s `BoundaryMarker`) must still be present when its consumer runs; that grammar has no compounding at all |
//!
//! **Why the two template exercises are independent.** Template 1's load-bearing claim is a NEGATIVE
//! one about over-generation across template boundaries; Template 2's is a POSITIVE one about a
//! silent rule inside a single template surviving composite pruning. Template 1's grammar contains
//! no zero-output rule, so a regression that pruned silent rules leaves it green; Template 2's
//! expectations are stated over its own grammar's own templates, so Template 1's documented
//! mutation (re-adding its four rules to the Stratum's `morphologicalRules=` list, which is what
//! `template-category-sharing/words.yaml`'s header records as having been MEASURED producing the
//! wrong answer) cannot touch it. The honest limit of the claim: a defect in the shared
//! `ApplyMorphologicalRules(input).Concat(ApplyTemplates(input))` interleaving itself would fail
//! both, because both are template exercises and 7.7 asks for two of those. What is genuinely
//! independent is the falsifier, and each has one the other does not detect.
//!
//! **Why the two cleanup exercises are independent.** One has a boundary producer and no boundary
//! consumer; the other has a boundary consumer and no producer. Neither grammar can exercise the
//! other's mechanism, so neither regression can hide behind the other.
//!
//! # The four properties, each with a stated falsifier
//!
//! **1. Exact analysis/root/multiplicity parity -- and WHICH relation each assertion uses.**
//! This matters enough to spell out, because a relation chosen for convenience is how the v1 scope
//! was once made invisible (`pg_foma::parity`'s own module doc, and the 2026-08-01 fix that
//! restored it). Three distinct relations appear below, named at every use site:
//! - The PROGRAM's parity relation is deduplicated [`pg_foma::parity::OccurrenceIdentities`] SET
//!   equality ([`OccurrenceIdentities::same_identities`]); multiplicity is deliberately NOT part of
//!   it. That relation is used for the language-rename invariance check, where "the same analyses"
//!   is exactly the question.
//! - 7.7 additionally asks for MULTIPLICITY, which set equality drops. So the per-word check below
//!   asserts the MULTISET cardinality too, via [`OccurrenceIdentities::raw_analyses`] against the
//!   committed `parses:` row count -- `words.yaml` is documented as sorted-but-NOT-deduped
//!   (`pg_parse::result_signature`, `WordEntry::expected_signature`), so a repeated signature there
//!   is a real, measured multiplicity and not a typo.
//! - Neither is full `WordAnalysis` equality, which would make engine internals (`syn_fs`, `mpr`,
//!   dense ordinals) observable as disagreement. It is not used anywhere here.
//!
//! *Falsifier:* an engine that reached one identity by two derivational paths where the fixture
//! recorded one path fails the `raw_analyses` assertion while still passing set equality; an engine
//! that lost an analysis outright fails both.
//!
//! **2. `root_index` is load-bearing.** The per-word check pins that every identity's `root_index`
//! indexes its own morpheme sequence, which is a well-formedness floor and not a discrimination
//! claim. The discrimination claim is [`root_index_discriminates_two_readings_of_one_surface`],
//! which uses the fixture staged for exactly this
//! (`conformance-staging/edge-cases/head-ambiguous-compounding`, whose two `dakimo` readings render
//! to the IDENTICAL flat signature string and so cannot be told apart by any `words.yaml`
//! signature diff). It asserts BOTH halves: the full relation keeps two identities, and the
//! root-blind projection of that same set collapses to ONE.
//!
//! *Falsifier:* that test cannot pass if root position is ignored -- ignoring it is precisely the
//! `assert_eq!(root_blind.len(), 1)` half, which is asserted to be STRICTLY smaller than the full
//! set. A relation that dropped `root_index` would report the two readings as one analysis.
//!
//! **3. Cleanup idempotence.** [`boundary_cleanup_applied_twice_equals_once`] builds the cleanup
//! relation the way `pg_foma::build`'s own private `boundary_cleanup_net` builds it (every
//! `CharDefKind::Boundary` token, `tok -> 0`, blanket and unconditional -- that module's doc records
//! why excluding any boundary family is a measured recall regression), then applies it twice with
//! `apply_down` and requires the second application to change nothing.
//!
//! *Falsifier / what input would expose non-idempotence:* the ADJACENT-DOUBLED boundary input
//! (`seg tok tok seg`). A once-per-position or leftmost-only replacement, or a context-restricted
//! `tok -> 0 || _ seg` rewrite, leaves a surviving boundary token after the first pass, so
//! `down(down(x)) != down(x)`. Non-vacuity is asserted separately (the first pass must actually
//! delete something), because an accidentally-empty boundary inventory would make an idempotence
//! assertion pass by asserting nothing. The companion assertion that a boundary-FREE input is
//! returned unchanged is the cleanup dossier's "no non-boundary symbol is deleted" obligation.
//!
//! **4. No language-name routing.** [`no_language_name_routing`] reloads each of the four fixtures
//! with its `<Language><Name>` replaced by a fixed neutral string and requires the derived graph's
//! `canonical_projection()` to be BYTE-identical and the oracle's per-word identity sets and
//! multiplicities to be unchanged.
//!
//! *Falsifier:* any code path that branched on a language identity -- a name-keyed lookup, a
//! per-language special case, a name-derived tie-break -- changes one of those two artifacts and
//! fails. The rename is asserted to hit exactly one occurrence first, so a fixture that stopped
//! carrying the assumed name breaks loudly instead of silently testing nothing.
//!
//! # Two hard constraints this file observes
//! No assertion is a proposal-set ceiling or a truncation, and no assertion reads a clock: wall time
//! is never an eligibility or certification input here. Every fixture is a synthetic construct-shaped
//! probe; no identifier in this file names a language.

use std::collections::{BTreeMap, BTreeSet};

use foma::apply::apply_init;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;

use pg_conformance_fixtures::{assert_matches_oracle, discover, FixtureRef, Root, WordsYaml};
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::mechanism_provider::derive_mechanism_graph;
use pg_foma::parity::OccurrenceIdentities;
use pg_foma::recipe_mechanism::{
    BoundaryState, MechanismBody, MechanismGraph, MechanismGraphError, MechanismId, MechanismKind,
    MechanismNode,
};
use pg_foma::replace::SegAlphabet;
use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::model::Grammar;
use pg_parse::identity::MorphemeKey;
use pg_parse::Morpher;

// ------------------------------------------------------------------------------------------------
// The four exercises. Named by what they compose, never by a language (this repo's standing
// conformance rule, and 7.7's own "no language-name routing" requirement restated at the naming
// level).
// ------------------------------------------------------------------------------------------------

/// Complete-template exercise 1: cross-template exclusion, plus a two-entry multiplicity row.
const TEMPLATE_EXCLUSION: &str = "template-category-sharing";
/// Complete-template exercise 2: a mandatory-but-silent slot inside one template.
const TEMPLATE_SILENT_SLOT: &str = "optional-template-composite";
/// Cleanup exercise 1: a boundary PRODUCED by morphotactics (the compounding seam).
const CLEANUP_BOUNDARY_PRODUCER: &str = "recipe-strata-generic";
/// Cleanup exercise 2: a boundary CONSUMED by ordered phonology before cleanup.
const CLEANUP_BOUNDARY_CONSUMER: &str = "recipe-ordered-generic";
/// Property-2 witness (NOT a fifth exercise): the only staged fixture whose two readings of one
/// surface differ ONLY in root position.
const ROOT_POSITION_WITNESS: &str = "head-ambiguous-compounding";

/// Every exercise, in the order 7.7 lists them.
const EXERCISES: &[&str] = &[
    TEMPLATE_EXCLUSION,
    TEMPLATE_SILENT_SLOT,
    CLEANUP_BOUNDARY_PRODUCER,
    CLEANUP_BOUNDARY_CONSUMER,
];

/// The neutral name every fixture is reloaded under by [`no_language_name_routing`]. Deliberately
/// not a word in any language.
const NEUTRAL_LANGUAGE_NAME: &str = "Zq0NeutralControl";

fn staged(name: &str) -> FixtureRef {
    discover()
        .into_iter()
        .find(|fixture| fixture.root == Root::Staging && fixture.name == name)
        .unwrap_or_else(|| panic!("missing staged fixture conformance-staging/edge-cases/{name}"))
}

fn load(xml: &str, label: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("{label}: fixture failed to load: {e}"))
}

// ------------------------------------------------------------------------------------------------
// The committed expectation record. Read from `words.yaml`; never computed from the engine.
// ------------------------------------------------------------------------------------------------

/// One word's committed expectation.
///
/// Both counts come out of the fixture's own `parses:` list, so this struct cannot express a number
/// its fixture did not already record.
#[derive(Debug)]
struct CommittedWord {
    word: String,
    /// Total `parses:` rows -- the MULTISET cardinality. `words.yaml` sorts but does not dedup, so a
    /// repeated signature string here is a measured multiplicity.
    raw_parses: usize,
    /// Distinct MORPHEME-JOIN parts (the text before `|`) among those rows.
    ///
    /// A sound LOWER bound on the number of distinct [`pg_parse::identity::AnalysisIdentity`]s:
    /// two different morpheme joins are two different morpheme-key vectors, hence two different
    /// identities. The morpheme half is used rather than the whole signature precisely because two
    /// rows CAN differ only in their rendered surface, which would make the whole-signature count an
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

fn committed_words(words: &WordsYaml) -> Vec<CommittedWord> {
    words
        .words
        .iter()
        // `adapter_visible()` is PROTOCOL.md section 3's rule: a word carrying any `guess: true`
        // parse is invisible to the adapter contract `Morpher::parse_word` implements, so asserting
        // on it would compare against a record the engine was never asked to produce. `expect_skip`
        // words raise `InvalidShapeException` and have no analysis set at all.
        .filter(|word| word.adapter_visible() && !word.expect_skip)
        .map(|word| {
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
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------
// The parity core: identity, root, multiplicity.
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
        "{label}: word {:?} unexpectedly failed to segment; \
         a committed adapter-visible word must have an analysis set (possibly empty)",
        expect.word
    );

    // A projection FAULT is an internal inconsistency, never a parity miss (`pg_foma::parity`'s
    // "Faults are not misses"). Panicking names it as such instead of letting it read as
    // disagreement.
    let occurrence =
        OccurrenceIdentities::project(&outcome.structured, grammar).unwrap_or_else(|e| {
            panic!(
                "{label}: word {:?} -- identity projection FAULTED (an engine inconsistency, \
                 not a parity miss): {e}",
                expect.word
            )
        });

    // -- MULTIPLICITY. The multiset relation, strictly stronger than the program's set-equality
    //    parity relation, which is what 7.7 asks for beyond it.
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
        "{label}: word {:?} -- {} distinct identities is FEWER than the {} distinct morpheme \
         joins the fixture records; distinct morpheme joins are distinct identities by \
         construction, so an analysis was lost",
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
    //    DISCRIMINATION claim about root position lives in
    //    `root_index_discriminates_two_readings_of_one_surface`, which is written so that it cannot
    //    pass if root position is ignored.
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
    //    not evidence about the compiled grammar, so it must not reach a parity claim at all. These
    //    fixtures are replayed through the plain adapter contract, so both must be absent; asserting
    //    it keeps a future `guess:`-carrying row from silently widening the scope.
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
/// Called first in every exercise: it is what makes the per-word identity work a REFINEMENT of the
/// existing ground truth rather than a second, independently-drifting one.
fn anchor_against_committed_signatures(label: &str, grammar: &Grammar, words: &WordsYaml) {
    let morpher = Morpher::new(grammar, usize::MAX).with_memo(true);
    let checked = assert_matches_oracle(label, words, &morpher);
    assert!(
        checked > 0,
        "{label}: replayed zero words -- a fixture that checks nothing is not an exercise"
    );
}

/// Every committed word's occurrence set for one fixture, keyed by word.
fn occurrences_for(
    label: &str,
    grammar: &Grammar,
    expectations: &[CommittedWord],
) -> BTreeMap<String, OccurrenceIdentities> {
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

// ------------------------------------------------------------------------------------------------
// The graph half of the slice.
// ------------------------------------------------------------------------------------------------

fn node_of<'g>(graph: &'g MechanismGraph, kind: MechanismKind) -> Option<&'g MechanismNode> {
    graph.nodes.iter().find(|node| node.kind() == kind)
}

/// Is there a directed path `from -> ... -> to`?
fn reaches(graph: &MechanismGraph, from: &MechanismId, to: &MechanismId) -> bool {
    let mut seen: BTreeSet<&MechanismId> = BTreeSet::new();
    let mut pending = vec![from];
    while let Some(current) = pending.pop() {
        if current == to {
            return true;
        }
        if !seen.insert(current) {
            continue;
        }
        for edge in &graph.edges {
            if &edge.producer == current {
                pending.push(&edge.consumer);
            }
        }
    }
    false
}

/// The declared boundary representations of a grammar's primary table, computed here independently
/// of `GrammarSemantics` so the spec below is CROSS-checked rather than restated.
fn declared_boundary_symbols(table: &CharDefTable) -> Vec<String> {
    let mut symbols: Vec<String> = table
        .iter()
        .filter(|(_, def)| def.kind() == CharDefKind::Boundary)
        .flat_map(|(_, def)| def.representations().iter().cloned())
        .collect();
    symbols.sort();
    symbols.dedup();
    symbols
}

/// Assert the `Morphotactics -> BoundaryCleanup` slice itself, and return the cleanup node's
/// declared boundary symbols.
fn assert_slice(label: &str, grammar: &Grammar) -> Vec<String> {
    let graph = derive_mechanism_graph(&GrammarSemantics::derive(grammar));
    graph
        .validate()
        .unwrap_or_else(|e| panic!("{label}: derived mechanism graph must validate: {e}"));

    let morphotactics = node_of(&graph, MechanismKind::Morphotactics).unwrap_or_else(|| {
        panic!("{label}: a morphotactics exercise must derive a Morphotactics mechanism")
    });

    let cleanups: Vec<&MechanismNode> = graph
        .nodes
        .iter()
        .filter(|node| node.kind() == MechanismKind::BoundaryCleanup)
        .collect();
    assert_eq!(
        cleanups.len(),
        1,
        "{label}: exactly one terminal cleanup, never two -- a second cleanup is what \
         'apply cleanup twice' would have to look like as a graph, and it is not representable"
    );
    let cleanup = cleanups[0];

    // Terminal, three independent ways: last in the canonical composition order, no outgoing edge,
    // and the only node that leaves boundaries `Removed`.
    assert_eq!(
        graph
            .nodes
            .last()
            .expect("a validated non-empty graph has a last node")
            .kind(),
        MechanismKind::BoundaryCleanup,
        "{label}: cleanup must be last in MechanismKind::COMPOSITION_ORDER"
    );
    assert!(
        !graph.edges.iter().any(|edge| edge.producer == cleanup.id),
        "{label}: cleanup must have no outgoing edge"
    );
    for node in &graph.nodes {
        assert_eq!(
            node.boundary_input(),
            BoundaryState::Present,
            "{label}: every mechanism requires boundaries present on input -- including cleanup, \
             which needs the symbols it removes"
        );
        let expected_output = if node.kind() == MechanismKind::BoundaryCleanup {
            BoundaryState::Removed
        } else {
            BoundaryState::Present
        };
        assert_eq!(
            node.boundary_output(),
            expected_output,
            "{label}: only cleanup removes boundaries ({:?} did)",
            node.kind()
        );
    }

    assert!(
        reaches(&graph, &morphotactics.id, &cleanup.id),
        "{label}: no directed path Morphotactics -> ... -> BoundaryCleanup; \
         the vertical slice does not exist in this graph"
    );

    let MechanismBody::BoundaryCleanup(spec) = &cleanup.body else {
        panic!("{label}: the cleanup node's body must be a BoundaryCleanupSpec");
    };
    let declared = declared_boundary_symbols(&grammar.char_tables[0]);
    assert_eq!(
        spec.boundary_symbols, declared,
        "{label}: the cleanup spec's symbol inventory must be exactly the primary table's declared \
         boundary representations"
    );
    spec.boundary_symbols.clone()
}

// ------------------------------------------------------------------------------------------------
// Exercise 1 and 2 -- complete templates.
// ------------------------------------------------------------------------------------------------

/// Complete-template exercise 1. Cross-template exclusion plus a genuine two-identity row.
///
/// The two mixes are the load-bearing NEGATIVE controls: no single `AffixTemplate` contains
/// `mrPfxA` with `mrSfxB` (or the mirror), so both must produce the EMPTY identity set. The
/// fixture's own `words.yaml` header records that an earlier draft, with all four rules also listed
/// in the Stratum's `morphologicalRules=` attribute, was MEASURED parsing `pakolola` successfully --
/// so this is a pin on a real, once-observed failure, not a hypothetical.
///
/// The cleanup end of the slice is derived here with an EMPTY symbol inventory, which is the honest
/// answer for a grammar that declares no `BoundaryDefinition`: the template exercises carry the
/// morphotactics half of the slice and the terminality of cleanup, and the two cleanup exercises
/// below carry the non-empty symbol inventory. Asserting emptiness rather than skipping it keeps a
/// future edit that adds a boundary to this grammar from silently changing what this test means.
#[test]
fn template_exercise_cross_template_exclusion() {
    let fixture = staged(TEMPLATE_EXCLUSION);
    let label = fixture.label();
    let grammar = load(&fixture.load_grammar_xml(), &label);
    let words = fixture.load_words_yaml();

    anchor_against_committed_signatures(&label, &grammar, &words);

    let symbols = assert_slice(&label, &grammar);
    assert!(
        symbols.is_empty(),
        "{label}: this grammar declares no BoundaryDefinition, so the derived cleanup inventory \
         must be empty; it is {symbols:?}"
    );

    let expectations = committed_words(&words);
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    for mix in ["pakolola", "takolosa"] {
        let occurrence = occurrences
            .get(mix)
            .unwrap_or_else(|| panic!("{label}: the cross-template mix {mix:?} must be pinned"));
        assert!(
            occurrence.is_empty(),
            "{label}: the cross-template mix {mix:?} has no valid derivation at all, so its \
             identity set must be empty; got {:?}",
            occurrence.entries()
        );
    }
    for control in ["pakolosa", "takolola"] {
        let occurrence = occurrences.get(control).unwrap_or_else(|| {
            panic!("{label}: the template-internal control {control:?} must be pinned")
        });
        assert_eq!(
            occurrence.len(),
            1,
            "{label}: the template-internal control {control:?} has exactly one analysis"
        );
    }
    // Multiplicity in the other direction: one surface, two lexically distinct roots.
    let ambiguous = occurrences
        .get("mbili")
        .unwrap_or_else(|| panic!("{label}: the two-entry multiplicity row must be pinned"));
    assert_eq!(
        ambiguous.len(),
        2,
        "{label}: two distinct roots share this surface, so two distinct identities"
    );
    assert_eq!(
        ambiguous.raw_analyses(),
        2,
        "{label}: two identities of multiplicity one each, not one identity found twice"
    );
    assert_eq!(
        ambiguous.collapsed_paths(),
        0,
        "{label}: nothing should have been deduplicated away here"
    );
}

/// Complete-template exercise 2. A mandatory-but-SILENT slot inside one template.
///
/// The load-bearing POSITIVE claim: `monu` is one surface with TWO analyses -- the bare root, and
/// template2's mandatory `mrVacuous` slot applied alone, a real morpheme that changes nothing
/// visible. An engine whose composite pruning treats a silent-output rule as prunable loses the
/// second one, which is the recall trap
/// `docs/fst-plan/morphotactic-composite-pruning.md` records. This is a different failure DIRECTION
/// from exercise 1 (under-generation inside one template, versus over-generation across two), and
/// exercise 1's grammar has no silent rule for this regression to touch.
#[test]
fn template_exercise_mandatory_silent_slot() {
    let fixture = staged(TEMPLATE_SILENT_SLOT);
    let label = fixture.label();
    let grammar = load(&fixture.load_grammar_xml(), &label);
    let words = fixture.load_words_yaml();

    anchor_against_committed_signatures(&label, &grammar, &words);

    let symbols = assert_slice(&label, &grammar);
    assert!(
        symbols.is_empty(),
        "{label}: this grammar declares no BoundaryDefinition, so the derived cleanup inventory \
         must be empty; it is {symbols:?}"
    );

    let expectations = committed_words(&words);
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    let silent = occurrences
        .get("monu")
        .unwrap_or_else(|| panic!("{label}: the vacuous-mandatory-slot witness must be pinned"));
    assert_eq!(
        silent.len(),
        2,
        "{label}: the bare root and the silent-slot derivation are TWO distinct identities of one \
         surface; got {:?}",
        silent.entries()
    );
    assert_eq!(
        silent.raw_analyses(),
        2,
        "{label}: two identities of multiplicity one each"
    );
    // The two readings differ in their morpheme SEQUENCE (one carries the silent morpheme), which
    // is what distinguishes this exercise from the root-position witness further down -- there the
    // sequences are equal and only the root position differs.
    let sequences: BTreeSet<Vec<MorphemeKey>> = silent
        .entries()
        .iter()
        .map(|entry| entry.identity.morphemes.clone())
        .collect();
    assert_eq!(
        sequences.len(),
        2,
        "{label}: the silent slot must show up as a distinct morpheme SEQUENCE, not merely as a \
         duplicate path"
    );

    // The whole-fixture SHAPE of the silent slot: it doubles exactly the bare roots and nothing
    // else. Every affixed word resolves to one clean analysis once the templates are self-contained
    // (the fixture's own header records that, having first measured the opposite). Expressed as
    // counts rather than a list of literal words on purpose -- one of this fixture's roots is
    // spelled with a non-ASCII modifier letter, and a mistyped literal would turn a real assertion
    // into a lookup miss.
    let doubled = occurrences
        .values()
        .filter(|occurrence| occurrence.len() == 2)
        .count();
    let single = occurrences
        .values()
        .filter(|occurrence| occurrence.len() == 1)
        .count();
    assert_eq!(
        doubled, 4,
        "{label}: exactly the four bare roots carry the extra silent-slot analysis"
    );
    assert_eq!(
        single + doubled,
        occurrences.len(),
        "{label}: no committed word may have zero or three-plus analyses here"
    );
}

// ------------------------------------------------------------------------------------------------
// Exercise 3 and 4 -- cleanup.
// ------------------------------------------------------------------------------------------------

/// Cleanup exercise 1. A boundary PRODUCED by morphotactics survives to a terminal cleanup.
///
/// The compounding seam is authored as a `BoundaryDefinition`, never a plain `SegmentDefinition`
/// (the fixture's own grammar comment records that as a re-verified gotcha), so the boundary this
/// exercise cleans is one the MORPHOTACTICS end of the slice created. This grammar declares no
/// boundary-consuming phonological rule at all, which is exactly why a regression in "cleanup waited
/// for its consumer" cannot hide here -- and why exercise 4, which has such a consumer and no
/// compounding, is independent of it.
#[test]
fn cleanup_exercise_boundary_produced_by_morphotactics() {
    let fixture = staged(CLEANUP_BOUNDARY_PRODUCER);
    let label = fixture.label();
    let grammar = load(&fixture.load_grammar_xml(), &label);
    let words = fixture.load_words_yaml();

    anchor_against_committed_signatures(&label, &grammar, &words);

    let symbols = assert_slice(&label, &grammar);
    assert!(
        !symbols.is_empty(),
        "{label}: a cleanup exercise must have a NON-EMPTY boundary inventory, or it certifies \
         nothing about cleanup"
    );

    let expectations = committed_words(&words);
    let occurrences = occurrences_for(&label, &grammar, &expectations);
    assert!(
        !occurrences.is_empty(),
        "{label}: no adapter-visible committed words"
    );

    // The seam-bearing compound row: one surface, two analyses, because the non-head resolves as
    // both of its homophonous readings. A cleanup that deleted a symbol it should not have, or that
    // ran before the seam existed, changes this count.
    let seam = occurrences
        .get("akutat")
        .unwrap_or_else(|| panic!("{label}: the compounding-seam row must be pinned"));
    assert_eq!(
        seam.len(),
        2,
        "{label}: the seam-bearing compound has two distinct identities; got {:?}",
        seam.entries()
    );
    assert_eq!(
        seam.raw_analyses(),
        2,
        "{label}: two identities of multiplicity one each"
    );
}

/// Cleanup exercise 2. A boundary CONSUMED by ordered phonology must still be present when its
/// consumer runs.
///
/// `mrComplexMeta` is a metathesis rule whose structural description contains a
/// `<BoundaryMarker boundary="cBnd" />` between its two switch roles: the boundary is its TRIGGER.
/// Cleaning up before it runs erases that trigger, which is the cleanup dossier's first rejected
/// architecture. Two assertions carry it: the surviving analysis of the boundary-crossing word, and
/// the graph mutation below, which moves cleanup ahead of its consumer and requires validation to
/// refuse the edge.
#[test]
fn cleanup_exercise_boundary_consumed_before_cleanup() {
    let fixture = staged(CLEANUP_BOUNDARY_CONSUMER);
    let label = fixture.label();
    let grammar = load(&fixture.load_grammar_xml(), &label);
    let words = fixture.load_words_yaml();

    anchor_against_committed_signatures(&label, &grammar, &words);

    let symbols = assert_slice(&label, &grammar);
    assert!(
        !symbols.is_empty(),
        "{label}: a cleanup exercise must have a NON-EMPTY boundary inventory"
    );

    let expectations = committed_words(&words);
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    // The boundary-crossing consumer's own word: the metathesis fired ACROSS the boundary, so the
    // boundary was present when it ran.
    let crossed = occurrences
        .get("mu+i")
        .unwrap_or_else(|| panic!("{label}: the boundary-crossing metathesis row must be pinned"));
    assert_eq!(
        crossed.len(),
        1,
        "{label}: the boundary-crossing word has exactly one analysis; got {:?}",
        crossed.entries()
    );
    assert_eq!(
        crossed.raw_analyses(),
        1,
        "{label}: reached by exactly one derivation"
    );
    // The un-metathesized neighbour has no `i +BND u` site, so the same rule correctly never fires.
    // Without this control, "the rule fired" would be indistinguishable from "the rule always
    // fires".
    let unfired = occurrences
        .get("mi")
        .unwrap_or_else(|| panic!("{label}: the no-site control must be pinned"));
    assert_eq!(
        unfired.len(),
        1,
        "{label}: the no-site control has exactly one analysis"
    );

    // -- The mutation the cleanup dossier's exercise 2 asks for: move cleanup BEFORE its consumer
    //    and require graph validation to reject the edge. Performed on this fixture's own DERIVED
    //    graph, so it is a statement about the real spine, not about a hand-built graph.
    let graph = derive_mechanism_graph(&GrammarSemantics::derive(&grammar));
    graph
        .validate()
        .expect("the unmutated derived graph validates");
    let cleanup_id = node_of(&graph, MechanismKind::BoundaryCleanup)
        .expect("a cleanup node")
        .id
        .clone();
    let mut mutated = graph.clone();
    let incoming: Vec<usize> = mutated
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.consumer == cleanup_id)
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        incoming.len(),
        1,
        "{label}: the canonical spine has exactly one edge into cleanup"
    );
    let index = incoming[0];
    let consumer_before_cleanup = mutated.edges[index].producer.clone();
    mutated.edges[index].producer = cleanup_id.clone();
    mutated.edges[index].consumer = consumer_before_cleanup;
    let error = mutated
        .validate()
        .expect_err("cleanup placed before its consumer must be REJECTED, not validated");
    assert!(
        matches!(error, MechanismGraphError::CleanupNotTerminal { .. }),
        "{label}: the refusal must name cleanup's non-terminality; got {error:?}"
    );
}

// ------------------------------------------------------------------------------------------------
// Property 2 -- root_index is load-bearing.
// ------------------------------------------------------------------------------------------------

/// `root_index` discriminates two readings of ONE surface that agree on everything else.
///
/// This is the falsifier for 7.7's root-position requirement, and it is written so that it CANNOT
/// pass if root position is ignored: the full relation must keep two identities, and the root-blind
/// projection of that same set must collapse to one. An assertion that only checked "two analyses
/// exist" would pass under a root-blind relation as long as something else distinguished them; here
/// nothing else does, by construction (the fixture's own `words.yaml` records that both readings
/// render to the IDENTICAL flat signature string, which is why no signature diff can pin this).
#[test]
fn root_index_discriminates_two_readings_of_one_surface() {
    let fixture = staged(ROOT_POSITION_WITNESS);
    let label = fixture.label();
    let grammar = load(&fixture.load_grammar_xml(), &label);
    let words = fixture.load_words_yaml();

    let witness = "dakimo";
    assert!(
        words.words.iter().any(|word| word.word == witness),
        "{label}: the root-position witness must come from the pinned fixture"
    );

    let morpher = Morpher::new(&grammar, usize::MAX).with_memo(true);
    let outcome = morpher.parse_word(witness);
    let occurrence = OccurrenceIdentities::project(&outcome.structured, &grammar)
        .expect("identity projection must not fault");

    assert_eq!(
        occurrence.len(),
        2,
        "{label}: both headedness readings must survive; got {:?}",
        occurrence.entries()
    );

    let sequences: BTreeSet<Vec<MorphemeKey>> = occurrence
        .entries()
        .iter()
        .map(|entry| entry.identity.morphemes.clone())
        .collect();
    assert_eq!(
        sequences.len(),
        1,
        "{label}: the two readings must share ONE morpheme sequence, or root position is not the \
         only thing distinguishing them and this witness proves nothing"
    );
    let categories: BTreeSet<Option<String>> = occurrence
        .entries()
        .iter()
        .map(|entry| entry.identity.category.clone())
        .collect();
    assert_eq!(
        categories.len(),
        1,
        "{label}: the two readings must share ONE category, for the same reason"
    );

    let roots: BTreeSet<i32> = occurrence
        .entries()
        .iter()
        .map(|entry| entry.identity.root_index)
        .collect();
    assert_eq!(
        roots,
        BTreeSet::from([0, 1]),
        "{label}: the two readings must differ in root position, 0 versus 1"
    );

    // The half that makes this falsifiable rather than merely true: a relation that ignored root
    // position would see ONE analysis here, not two.
    let root_blind: BTreeSet<(Vec<MorphemeKey>, Option<String>)> = occurrence
        .entries()
        .iter()
        .map(|entry| {
            (
                entry.identity.morphemes.clone(),
                entry.identity.category.clone(),
            )
        })
        .collect();
    assert_eq!(
        root_blind.len(),
        1,
        "{label}: the root-BLIND projection must collapse to one member"
    );
    assert!(
        root_blind.len() < occurrence.len(),
        "{label}: the root-blind projection must be STRICTLY smaller than the full identity set -- \
         that strict inequality IS the claim that root_index is load-bearing"
    );
}

// ------------------------------------------------------------------------------------------------
// Property 3 -- cleanup idempotence.
// ------------------------------------------------------------------------------------------------

/// The boundary-token cleanup relation, built exactly as `pg_foma::build`'s own private
/// `boundary_cleanup_net` builds it: every `CharDefKind::Boundary` token, `tok -> 0`, blanket and
/// unconditional. That module's doc records why excluding ANY boundary family is a measured recall
/// regression, so this mirrors it rather than inventing a narrower relation to test.
fn cleanup_net(table: &CharDefTable, alphabet: &SegAlphabet) -> Option<Fsm> {
    let tokens: Vec<char> = table
        .iter()
        .filter(|(_, def)| def.kind() == CharDefKind::Boundary)
        .map(|(id, _)| alphabet.token(id))
        .collect();
    if tokens.is_empty() {
        return None;
    }
    let regex = tokens
        .iter()
        .map(|token| format!("{token} -> 0"))
        .collect::<Vec<_>>()
        .join(", ");
    fsm_parse_regex(&FomaOptions::default(), &regex, None, None)
}

fn apply_down_set(net: &Fsm, input: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut handle = apply_init(net);
    for result in handle.down(input) {
        out.insert(result);
    }
    out
}

/// Applying boundary cleanup twice equals applying it once, for both cleanup exercises.
///
/// The load-bearing input is the ADJACENT-DOUBLED boundary (`seg tok tok seg`): a once-per-position,
/// leftmost-only, or context-restricted deletion leaves a surviving boundary token after the first
/// pass, so the second pass changes the result. Three companion assertions keep this from passing
/// vacuously: the boundary inventory must be non-empty, the first pass must actually delete
/// something, and its output must contain no boundary token. A fourth asserts the dossier's "no
/// non-boundary symbol is deleted" obligation, which is the other way an over-eager cleanup could be
/// idempotent and still wrong.
#[test]
fn boundary_cleanup_applied_twice_equals_once() {
    for name in [CLEANUP_BOUNDARY_PRODUCER, CLEANUP_BOUNDARY_CONSUMER] {
        let fixture = staged(name);
        let label = fixture.label();
        let grammar = load(&fixture.load_grammar_xml(), &label);
        let table = &grammar.char_tables[0];
        let alphabet = SegAlphabet::new(table);

        let boundaries: Vec<char> = table
            .iter()
            .filter(|(_, def)| def.kind() == CharDefKind::Boundary)
            .map(|(id, _)| alphabet.token(id))
            .collect();
        let segments: Vec<char> = table
            .iter()
            .filter(|(_, def)| def.kind() == CharDefKind::Segment)
            .map(|(id, _)| alphabet.token(id))
            .collect();
        assert!(
            !boundaries.is_empty(),
            "{label}: no boundary token, so an idempotence assertion here would assert nothing"
        );
        assert!(!segments.is_empty(), "{label}: no segment token");

        let net = cleanup_net(table, &alphabet)
            .unwrap_or_else(|| panic!("{label}: cleanup relation must compile"));
        let seg = segments[0];

        // Inputs, in increasing order of what they can catch. The doubled forms are the ones a
        // once-only or context-restricted deletion fails on.
        let mut inputs = vec![
            format!("{seg}{}{seg}", boundaries[0]),
            format!("{seg}{}{}{seg}", boundaries[0], boundaries[0]),
            format!("{}{seg}{}", boundaries[0], boundaries[0]),
        ];
        if boundaries.len() > 1 {
            // A mixed run: two DIFFERENT boundary families adjacent, which is the shape a
            // multi-representation marker followed by a plain separator has.
            inputs.push(format!("{seg}{}{}{seg}", boundaries[0], boundaries[1]));
        }

        for input in &inputs {
            let once = apply_down_set(&net, input);
            assert!(
                !once.is_empty(),
                "{label}: cleanup produced no output for {input:?}"
            );
            assert!(
                !(once.len() == 1 && once.contains(input.as_str())),
                "{label}: cleanup was a no-op on {input:?}, which contains boundary tokens -- the \
                 idempotence assertion below would then be vacuous"
            );
            for output in &once {
                for boundary in &boundaries {
                    assert!(
                        !output.contains(*boundary),
                        "{label}: cleanup left a boundary token in {output:?} (from {input:?})"
                    );
                }
            }

            // THE property: applying the same relation to every member of the first result changes
            // nothing.
            let twice: BTreeSet<String> = once
                .iter()
                .flat_map(|output| apply_down_set(&net, output))
                .collect();
            assert_eq!(
                twice, once,
                "{label}: cleanup is NOT idempotent on {input:?} -- a second application changed \
                 the result"
            );
        }

        // No non-boundary symbol is deleted: a boundary-free input comes back unchanged.
        let clean = if segments.len() > 1 {
            format!("{seg}{}", segments[1])
        } else {
            format!("{seg}{seg}")
        };
        assert_eq!(
            apply_down_set(&net, &clean),
            BTreeSet::from([clean.clone()]),
            "{label}: cleanup must be the identity on a boundary-free input {clean:?}"
        );
    }
}

// ------------------------------------------------------------------------------------------------
// Property 4 -- no language-name routing.
// ------------------------------------------------------------------------------------------------

/// Replace a fixture's `<Language><Name>` with a fixed neutral string.
///
/// Asserts the name occurs EXACTLY once first: a fixture that stopped carrying the assumed name must
/// break loudly rather than silently make the rename a no-op and the test vacuous.
fn renamed(xml: &str, language_name: &str, label: &str) -> String {
    let needle = format!("<Name>{language_name}</Name>");
    assert_eq!(
        xml.matches(&needle).count(),
        1,
        "{label}: expected exactly one occurrence of {needle:?} to rename"
    );
    xml.replace(&needle, &format!("<Name>{NEUTRAL_LANGUAGE_NAME}</Name>"))
}

/// A grammar's behaviour must not depend on any language identity.
///
/// For each of the four exercises: reload it with its language name replaced, and require the
/// derived mechanism graph's `canonical_projection()` to be BYTE-identical and the oracle's per-word
/// identity sets and multiplicities to be unchanged. Any name-keyed lookup, per-language special
/// case, or name-derived tie-break anywhere on either path changes one of those two artifacts.
///
/// The identity comparison here uses the PROGRAM's parity relation
/// ([`OccurrenceIdentities::same_identities`]) -- deduplicated set equality -- because "did renaming
/// change which analyses exist" is exactly the question that relation answers. Multiplicity is
/// compared separately and explicitly, since set equality is blind to it by design.
#[test]
fn no_language_name_routing() {
    for name in EXERCISES {
        let fixture = staged(name);
        let label = fixture.label();
        let xml = fixture.load_grammar_xml();
        let words = fixture.load_words_yaml();

        let original = load(&xml, &label);
        let control = load(&renamed(&xml, &words.language, &label), &label);

        assert_eq!(
            derive_mechanism_graph(&GrammarSemantics::derive(&original)).canonical_projection(),
            derive_mechanism_graph(&GrammarSemantics::derive(&control)).canonical_projection(),
            "{label}: the derived mechanism graph changed when only the language NAME changed -- \
             something routes on a language identity"
        );

        let expectations = committed_words(&words);
        assert!(
            !expectations.is_empty(),
            "{label}: no adapter-visible committed words to compare"
        );
        let original_morpher = Morpher::new(&original, usize::MAX).with_memo(true);
        let control_morpher = Morpher::new(&control, usize::MAX).with_memo(true);
        for expect in &expectations {
            let left = OccurrenceIdentities::project(
                &original_morpher.parse_word(&expect.word).structured,
                &original,
            )
            .expect("identity projection must not fault");
            let right = OccurrenceIdentities::project(
                &control_morpher.parse_word(&expect.word).structured,
                &control,
            )
            .expect("identity projection must not fault");
            assert!(
                left.same_identities(&right),
                "{label}: word {:?} -- renaming the language changed the identity SET; \
                 only in the original: {:?}; only in the renamed control: {:?}",
                expect.word,
                left.identities_absent_from(&right),
                right.identities_absent_from(&left)
            );
            assert_eq!(
                left.raw_analyses(),
                right.raw_analyses(),
                "{label}: word {:?} -- renaming the language changed the MULTIPLICITY, which set \
                 equality above is blind to by design",
                expect.word
            );
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Slice-wide invariant: the four exercises really are four distinct fixtures, and each is staged.
// ------------------------------------------------------------------------------------------------

/// A guard against the slice quietly shrinking.
///
/// 7.7 asks for two independent template exercises and two cleanup exercises. If a future edit
/// pointed two of the four names at one fixture, or dropped one, every other test in this file would
/// still pass while the slice covered less than it claims. This asserts the shape itself: four
/// distinct staged fixtures, two with an empty derived cleanup inventory and two with a non-empty
/// one.
#[test]
fn the_slice_has_four_distinct_exercises() {
    let names: BTreeSet<&str> = EXERCISES.iter().copied().collect();
    assert_eq!(names.len(), 4, "the four exercises must be four fixtures");

    let mut with_boundaries = 0;
    let mut without_boundaries = 0;
    for name in EXERCISES {
        let fixture = staged(name);
        let label = fixture.label();
        let grammar = load(&fixture.load_grammar_xml(), &label);
        if declared_boundary_symbols(&grammar.char_tables[0]).is_empty() {
            without_boundaries += 1;
        } else {
            with_boundaries += 1;
        }
    }
    assert_eq!(
        (with_boundaries, without_boundaries),
        (2, 2),
        "two cleanup exercises must declare boundary symbols and two template exercises must not"
    );
}
