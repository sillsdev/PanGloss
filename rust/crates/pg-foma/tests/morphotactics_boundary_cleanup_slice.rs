//! Vertical-slice gate: the derived `Morphotactics -> BoundaryCleanup` mechanism-graph edge against the engine's actual per-word analyses, replayed for four already-staged conformance fixtures.

use std::collections::{BTreeMap, BTreeSet};

use foma::apply::apply_init;
use foma::options::FomaOptions;
use foma::regex::fsm_parse_regex;
use foma::types::Fsm;

use pg_conformance_fixtures::{assert_matches_oracle, discover, FixtureRef, Root, WordsYaml};
use pg_foma::backend_mechanism::{
    BoundaryState, MechanismBody, MechanismGraph, MechanismGraphError, MechanismId, MechanismKind,
    MechanismNode,
};
use pg_foma::grammar_semantics::GrammarSemantics;
use pg_foma::mechanism_provider::derive_mechanism_graph;
use pg_foma::parity::OccurrenceIdentities;
use pg_foma::replace::SegAlphabet;
use pg_grammar::chardef::{CharDefKind, CharDefTable};
use pg_grammar::model::Grammar;
use pg_parse::identity::MorphemeKey;
use pg_parse::Morpher;

/// Complete-template exercise 1: cross-template exclusion, plus a two-entry multiplicity row.
const TEMPLATE_EXCLUSION: &str = "template-category-sharing";
/// Complete-template exercise 2: a mandatory-but-silent slot inside one template.
const TEMPLATE_SILENT_SLOT: &str = "optional-template-composite";
/// Cleanup exercise 1: a boundary PRODUCED by morphotactics (the compounding seam).
const CLEANUP_BOUNDARY_PRODUCER: &str = "backend-strata-generic";
/// Cleanup exercise 2: a boundary CONSUMED by ordered phonology before cleanup.
const CLEANUP_BOUNDARY_CONSUMER: &str = "backend-ordered-generic";
/// Property-2 witness, not a fifth exercise: the only staged fixture whose two readings of one surface differ only in root position.
const ROOT_POSITION_WITNESS: &str = "head-ambiguous-compounding";

/// Every exercise, in the order this slice lists them.
const EXERCISES: &[&str] = &[
    TEMPLATE_EXCLUSION,
    TEMPLATE_SILENT_SLOT,
    CLEANUP_BOUNDARY_PRODUCER,
    CLEANUP_BOUNDARY_CONSUMER,
];

/// The neutral name every fixture is reloaded under by `no_language_name_routing`; deliberately not a word in any language.
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

// The committed expectation record: read from `words.yaml`, never computed from the engine.

/// One word's committed expectation; both counts come from the fixture's own `parses:` list, so this struct can't express a number the fixture didn't already record.
#[derive(Debug)]
struct CommittedWord {
    word: String,
    /// Total `parses:` rows (multiset cardinality) -- `words.yaml` sorts but does not dedup, so a repeated signature is a measured multiplicity.
    raw_parses: usize,
    /// Distinct morpheme-join parts (text before `|`): a sound lower bound on distinct identities, since two rows can differ only in rendered surface.
    distinct_morph_joins: usize,
}

impl CommittedWord {
    /// True when the committed record pins the distinct-identity count exactly (lower and upper bounds coincide).
    fn pins_exact_identity_count(&self) -> bool {
        self.distinct_morph_joins == self.raw_parses
    }
}

fn committed_words(words: &WordsYaml) -> Vec<CommittedWord> {
    words
        .words
        .iter()
        // Words with any `guess: true` parse are invisible to `Morpher::parse_word`'s adapter contract; `expect_skip` words have no analysis set at all.
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

/// Assert exact analysis/root/multiplicity parity for one word against its committed record; returns the projected occurrence for callers to layer fixture-specific claims on.
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

    // A projection FAULT is an internal inconsistency, never a parity miss; panicking names it as such rather than reading as disagreement.
    let occurrence =
        OccurrenceIdentities::project(&outcome.structured, grammar).unwrap_or_else(|e| {
            panic!(
                "{label}: word {:?} -- identity projection FAULTED (an engine inconsistency, \
                 not a parity miss): {e}",
                expect.word
            )
        });

    // -- MULTIPLICITY: the multiset relation, strictly stronger than the program's set-equality parity relation.
    assert_eq!(
        occurrence.raw_analyses() as usize,
        expect.raw_parses,
        "{label}: word {:?} -- MULTISET cardinality disagrees with the committed `parses:` row \
         count. Committed rows are sorted-but-not-deduped, so this count is a measured \
         multiplicity, not a formatting artifact. Observed identities: {:?}",
        expect.word,
        occurrence.entries()
    );

    // -- SET: bounded above and below by the committed record; where the bounds meet, the count is pinned exactly and every multiplicity is one.
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

    // -- ROOT: well-formedness floor -- a root position must index its own morpheme sequence.
    // The discrimination claim about root position is pinned by `root_index_discriminates_two_readings_of_one_surface`.
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

    // -- v1 scope: a guessed or supplied-root analysis is not evidence about the compiled grammar, so neither may reach a parity claim here.
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

/// Anchors a whole fixture against its committed signature record via the shared oracle replay, so the per-word identity work below is a refinement of existing ground truth, not a second drifting one.
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

/// The grammar's declared boundary representations, computed independently of `GrammarSemantics` so the spec below is cross-checked, not restated.
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

/// Asserts the `Morphotactics -> BoundaryCleanup` slice itself and returns the cleanup node's declared boundary symbols.
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

    // Terminal, three independent ways: last in composition order, no outgoing edge, and the only node leaving boundaries `Removed`.
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

/// Cross-template exclusion: no `AffixTemplate` mixes `mrPfxA` with `mrSfxB` (or the mirror), so both must yield the empty identity set; the empty cleanup inventory here is asserted, not skipped, since this grammar declares no `BoundaryDefinition`.
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

/// Mandatory-but-silent slot: `monu` has two analyses -- the bare root and the mandatory `mrVacuous` slot applied alone -- so composite pruning that treats a silent-output rule as prunable loses the second.
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
    // The two readings differ in morpheme SEQUENCE here, unlike the root-position witness below where sequences are equal and only root position differs.
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

    // The silent slot doubles exactly the bare roots and nothing else; expressed as counts, not literal words, since one root uses a non-ASCII letter.
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

/// A boundary produced by morphotactics (the compounding seam, authored as `BoundaryDefinition`) survives to terminal cleanup; this grammar has no boundary-consuming rule, so it can't hide a "cleanup waited for its consumer" regression.
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

    // The seam-bearing compound: one surface, two analyses, because the non-head resolves as both homophonous readings; a wrong cleanup changes this count.
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

/// A boundary consumed by ordered phonology (`mrComplexMeta`'s `<BoundaryMarker>` trigger) must still be present when its consumer runs; cleaning up first would erase the trigger, so this asserts both the surviving analysis and that the graph rejects a cleanup-before-consumer edge.
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

    // The metathesis fired across the boundary, so the boundary was present when the rule ran.
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
    // The un-metathesized neighbour has no `i +BND u` site, so the rule correctly never fires; without this control "fired" is indistinguishable from "always fires".
    let unfired = occurrences
        .get("mi")
        .unwrap_or_else(|| panic!("{label}: the no-site control must be pinned"));
    assert_eq!(
        unfired.len(),
        1,
        "{label}: the no-site control has exactly one analysis"
    );

    // Moves cleanup BEFORE its consumer on this fixture's own derived graph and requires validation to reject the edge.
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

/// `root_index` discriminates two readings of one surface that agree on everything else -- written so it cannot pass if root position is ignored: the full relation keeps two identities, and the root-blind projection of the same set collapses to one.
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

    // The falsifying half: a relation that ignored root position would see one analysis here, not two.
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

/// Mirrors `pg_foma::build`'s private `boundary_cleanup_net`: every `CharDefKind::Boundary` token maps `tok -> 0`, blanket and unconditional, since excluding any boundary family is a measured recall regression.
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

/// Applying boundary cleanup twice equals applying it once; the load-bearing input is the adjacent-doubled boundary (`seg tok tok seg`), which a once-per-position or context-restricted deletion fails, plus non-vacuity and a boundary-free-input-unchanged control.
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

        // Inputs in increasing order of what they can catch; the doubled forms are what a once-only or context-restricted deletion fails on.
        let mut inputs = vec![
            format!("{seg}{}{seg}", boundaries[0]),
            format!("{seg}{}{}{seg}", boundaries[0], boundaries[0]),
            format!("{}{seg}{}", boundaries[0], boundaries[0]),
        ];
        if boundaries.len() > 1 {
            // A mixed run of two boundary families; unreached today since both fixtures declare only one family.
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

            // THE property: reapplying the relation to every member of the first result changes nothing.
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

/// Replaces a fixture's `<Language><Name>` with a fixed neutral string; asserts the name occurs exactly once first, so a stale name breaks loudly instead of making the rename a silent no-op.
fn renamed(xml: &str, language_name: &str, label: &str) -> String {
    let needle = format!("<Name>{language_name}</Name>");
    assert_eq!(
        xml.matches(&needle).count(),
        1,
        "{label}: expected exactly one occurrence of {needle:?} to rename"
    );
    xml.replace(&needle, &format!("<Name>{NEUTRAL_LANGUAGE_NAME}</Name>"))
}

/// A grammar's behaviour must not depend on any language identity: reloading each exercise under a replaced name must leave the derived graph's `canonical_projection()` byte-identical and the oracle's per-word identity sets and multiplicities unchanged.
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

/// Guards against the slice quietly shrinking: asserts four distinct staged fixtures, two with an empty derived cleanup inventory and two with a non-empty one.
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

#[test]
fn templated_query_accepts_a_surface_with_an_explicit_boundary() {
    let fixture = staged(CLEANUP_BOUNDARY_CONSUMER);
    let label = fixture.label();
    let grammar = load(&fixture.load_grammar_xml(), &label);
    let mut output = pg_foma::templated_compile::compile_templated_morphotactics(&grammar)
        .expect("templated-underlying-tokens compile must not fail");
    let control = output.proposer.propose("mi");
    assert!(
        !control.is_empty(),
        "the boundary-free control must remain analyzable: {control:?}"
    );
    let candidates = output.proposer.propose("mu+i");
    assert!(!candidates.is_empty(), "the templated query encoder must accept the explicit boundary surface \"mu+i\" and reach the compiled metathesis path");
}
