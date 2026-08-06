//! Group A: the first five of eleven orthogonal-basis mechanisms, each exercised at least
//! twice; see `docs/research/pg-foma-orthogonal-basis-group-a-notes.md` for the mechanism table, relation vocabulary, and per-exercise rationale.

use std::collections::{BTreeMap, BTreeSet};

use pg_conformance_fixtures::{assert_matches_oracle, discover, FixtureRef, Root, WordsYaml};
use pg_foma::parity::OccurrenceIdentities;
use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{
    CoOccurrenceAdjacency, Grammar, MRuleId, MorphRuleDef, OutputAction, PartRef, PhonRuleDef,
};
use pg_parse::identity::MorphemeKey;
use pg_parse::Morpher;

// The exercise inventory: one table, so the shape guard below checks the whole basis.

/// Group A's five mechanisms; each slash-separated source name counts as one mechanism.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Mechanism {
    TemplateOrderCoOccurrence,
    CascadeStrata,
    LexicalClass,
    AllomorphPriority,
    ZeroMorphology,
}

/// One exercise: a mechanism, its committed fixture, and the falsifier no sibling exercise can detect.
struct Exercise {
    mechanism: Mechanism,
    root: Root,
    category: &'static str,
    name: &'static str,
    /// Human-checked prose, so a future edit that points two exercises at one falsifier is visible.
    independent_falsifier: &'static str,
}

const EX_TEMPLATE_SLOT_ORDER: &Exercise = &Exercise {
    mechanism: Mechanism::TemplateOrderCoOccurrence,
    root: Root::Machine,
    category: "languages",
    name: "suffixing-extension-slot-ordering",
    independent_falsifier:
        "an engine that stopped enforcing an OBLIGATORY template slot admits `andik`; one that \
         collapsed the two extension orders into a single identity loses the distinction between \
         `andikishila` and `andikilisha`. Neither defect involves a co-occurrence rule, and this \
         grammar declares none.",
};

const EX_TEMPLATE_DISJUNCTIVE_SLOT_AND_ENFORCED_ORDER: &Exercise = &Exercise {
    mechanism: Mechanism::TemplateOrderCoOccurrence,
    root: Root::Machine,
    category: "languages",
    name: "prefixal-discontinuous-slot-dependency",
    independent_falsifier:
        "the exact CONVERSE of the slot-ordering fixture, and the two cannot both be satisfied by \
         one mis-implementation. There, the swappable optional pair has only optional slots between \
         it and the root, so the stratum's fixpoint retry rescues the reverse order and \
         `andikilisha` is ACCEPTED. Here, THREE OBLIGATORY slots lie between the swappable pair and \
         the root, so the single-pass outer-to-inner walk dies on the first obligatory slot it \
         reaches out of turn and `gahobishiyidkal` is REFUSED. An engine that enforced template \
         order unconditionally on analysis loses `andikilisha`; one that rescued every out-of-order \
         pair by retry admits `gahobishiyidkal`. This fixture also carries the DISJUNCTIVE third of \
         the construct (one slot holding two mutually exclusive rules), which the other declares \
         none of.",
};

const EX_CO_OCCURRENCE_ADJACENCY: &Exercise = &Exercise {
    mechanism: Mechanism::TemplateOrderCoOccurrence,
    root: Root::Machine,
    category: "languages",
    name: "suffixing-evidential-adjacency-chain",
    independent_falsifier:
        "an engine that collapsed `adjacentToLeft` into `somewhereToLeft` admits \
         `walaknichikwas`; one that collapsed it the other way rejects `walaknichiktan`; one that \
         evaluated co-occurrence per MORPHEME rather than per ALLOMORPH admits `kantancha`. None of \
         those three is a template-slot defect, and this grammar's co-occurrence constraints are \
         not expressed through template order at all.",
};

const EX_CASCADE_ORDERED_RULES: &Exercise = &Exercise {
    mechanism: Mechanism::CascadeStrata,
    root: Root::Machine,
    category: "languages",
    name: "suffixing-vowel-harmony",
    independent_falsifier:
        "this grammar has ONE stratum, so no stratum-ordering defect can reach it. Its falsifier is \
         intra-stratum rule ORDER: `kutagida` needs three phonological rules applied in the \
         stratum's declared sequence, and `unitide` needs the harmony rule's bounded transparency \
         span to STOP it while the epenthesis rule still fires twice in one pass.",
};

const EX_STRATA_CROSS_STRATUM_FEED: &Exercise = &Exercise {
    mechanism: Mechanism::CascadeStrata,
    root: Root::Machine,
    category: "languages",
    name: "polysynthetic-stratal-derivation-chain",
    independent_falsifier:
        "the derivational rule and the inflectional rule live on DIFFERENT strata, so one identity \
         (`nunaliqvuq`) spans both. An engine that failed to feed stratum N's output into stratum \
         N+1 loses it; one that ignored stratum boundaries altogether admits `nunavuq`, which must \
         have no analysis. Both grammars' character tables are irrelevant here -- every stratum in \
         this grammar shares one table, which is what makes the third exercise independent.",
};

const EX_STRATA_PER_STRATUM_TABLE: &Exercise = &Exercise {
    mechanism: Mechanism::CascadeStrata,
    root: Root::Machine,
    category: "edge-cases",
    name: "bistratal-overlapping-segment-representation",
    independent_falsifier:
        "the two strata declare DIFFERENT character-definition tables that share a representation \
         string. An engine that merged them into one table makes the inner-stratum roots \
         tokenizable, so `basi`/`abis` stop being invalid-shape. That defect is invisible to both \
         other cascade/strata exercises, neither of which has more than one table.",
};

const EX_LEXICAL_CLASS_ALLOMORPH_REGIONS: &Exercise = &Exercise {
    mechanism: Mechanism::LexicalClass,
    root: Root::Machine,
    category: "languages",
    name: "fusional-realizational-morphology",
    independent_falsifier:
        "the class lives on the ALLOMORPH (`Allomorph@stemName`) and is checked by unifying the \
         stem name's REGIONS against the word's own feature structure -- with two classes that \
         OVERLAP on one person value and an unrestricted default allomorph. A plain \
         identity/inequality implementation flips at least one of the twelve committed cells; \
         forgetting that the two classes jointly exhaust the person space wrongly admits the \
         default allomorph with a person suffix.",
};

const EX_LEXICAL_CLASS_RULE_LEVEL: &Exercise = &Exercise {
    mechanism: Mechanism::LexicalClass,
    root: Root::Machine,
    category: "languages",
    name: "suffixing-extension-slot-ordering",
    independent_falsifier:
        "the class is read by the RULE (`AffixProcessRule.RequiredStemName`), a different check from \
         the allomorph-level one above -- this fixture's own grammar comment separates the two by \
         name. This grammar declares exactly ONE class with exactly ONE region, so the \
         overlapping-region algebra the other exercise turns on is not expressible here at all: \
         only presence versus absence of the class label on the selected root allomorph can decide \
         anything.",
};

const EX_ALLOMORPH_PRIORITY_EARLIER_BLOCKS: &Exercise = &Exercise {
    mechanism: Mechanism::AllomorphPriority,
    root: Root::Machine,
    category: "edge-cases",
    name: "disjunctive-recheck",
    independent_falsifier:
        "priority here runs in the BLOCKING direction: a synthesis using a later-indexed allomorph \
         is REJECTED when an earlier-indexed, non-free-fluctuating alternative would also have \
         matched. An engine that ignored allomorph index order over-accepts (`wakta`, `pakda`); one \
         that applied the rejection without the free-fluctuation escape under-accepts (`grey`). \
         Both are over/under-acceptance defects that the other priority exercise cannot see, since \
         nothing there is ever blocked.",
};

const EX_ALLOMORPH_PRIORITY_LATER_REACHABLE: &Exercise = &Exercise {
    mechanism: Mechanism::AllomorphPriority,
    // `Root::Staging`, not `Machine`: fixture lives under `conformance-staging/edge-cases/circumfix-non-first-allomorph-selection`.
    root: Root::Staging,
    category: "edge-cases",
    name: "circumfix-non-first-allomorph-selection",
    independent_falsifier:
        "priority here runs in the REACHABILITY direction: the rule's SECOND-declared allomorph is \
         structurally a circumfix while its first is an ordinary suffix, so an engine that \
         classified or indexed a rule by its FIRST allomorph alone loses `kemitan` entirely -- a \
         recall gap, the exact opposite failure direction from the blocking exercise above.",
};

const EX_ZERO_MORPH_SILENT_TEMPLATE_SLOT: &Exercise = &Exercise {
    mechanism: Mechanism::ZeroMorphology,
    root: Root::Staging,
    category: "edge-cases",
    name: "optional-template-composite",
    independent_falsifier:
        "the zero morpheme sits in a MANDATORY AffixTemplate slot and is freely available, so it \
         creates AMBIGUITY: one surface, two identities differing by exactly its key. An engine \
         whose template-composite pruning treats a silent-output rule as prunable loses the second \
         one. That pruning path is a template path; the other zero-morphology exercise's zero rule \
         is in no template at all.",
};

const EX_ZERO_MORPH_ZERO_DERIVATION: &Exercise = &Exercise {
    mechanism: Mechanism::ZeroMorphology,
    root: Root::Machine,
    category: "edge-cases",
    name: "subrule-morphosyntactic-gating",
    independent_falsifier:
        "this zero morpheme is DISAMBIGUATING, not ambiguity-creating: it changes only the \
         category, and its sole observable trace is that a downstream category-gated rewrite fires. \
         So `bat` must have exactly one analysis WITH it and `pat` exactly one WITHOUT it -- the \
         second half is an OVER-generation falsifier (a spuriously insertable zero morpheme) that \
         the other exercise structurally cannot have, because there the zero morpheme genuinely is \
         insertable everywhere.",
};

/// Every group-A exercise, in the order 7.8 lists the mechanisms.
const EXERCISES: &[&Exercise] = &[
    EX_TEMPLATE_SLOT_ORDER,
    EX_TEMPLATE_DISJUNCTIVE_SLOT_AND_ENFORCED_ORDER,
    EX_CO_OCCURRENCE_ADJACENCY,
    EX_CASCADE_ORDERED_RULES,
    EX_STRATA_CROSS_STRATUM_FEED,
    EX_STRATA_PER_STRATUM_TABLE,
    EX_LEXICAL_CLASS_ALLOMORPH_REGIONS,
    EX_LEXICAL_CLASS_RULE_LEVEL,
    EX_ALLOMORPH_PRIORITY_EARLIER_BLOCKS,
    EX_ALLOMORPH_PRIORITY_LATER_REACHABLE,
    EX_ZERO_MORPH_SILENT_TEMPLATE_SLOT,
    EX_ZERO_MORPH_ZERO_DERIVATION,
];

// Fixture plumbing.

fn fixture_of(exercise: &Exercise) -> FixtureRef {
    discover()
        .into_iter()
        .find(|found| {
            found.root == exercise.root
                && found.category == exercise.category
                && found.name == exercise.name
        })
        .unwrap_or_else(|| {
            panic!(
                "missing conformance fixture {}:{}/{}. If it is an upstream (machine:) fixture, \
                 this worktree's `machine` submodule is not initialized -- run \
                 `rust/tools/conformance.ps1` (pg.ps1 -Mode test/-Mode corpus-test both do it in \
                 preflight). Panicking rather than skipping on purpose: a fixture this file could \
                 not LOOK at must never read as a fixture that passed.",
                exercise.root.label(),
                exercise.category,
                exercise.name
            )
        })
}

fn load(xml: &str, label: &str) -> Grammar {
    pg_grammar::load(xml).unwrap_or_else(|e| panic!("{label}: fixture failed to load: {e}"))
}

// The committed expectation record: read out of `words.yaml`, never computed from the engine.

/// One word's committed expectation, read from the fixture's own `parses:` list.
#[derive(Debug)]
struct CommittedWord {
    word: String,
    /// Total `parses:` rows -- the MULTISET cardinality.
    raw_parses: usize,
    /// Distinct morpheme-join parts (text before `|`) among those rows: a sound lower bound on distinct identities.
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
        // `adapter_visible()` implements PROTOCOL.md's guess-word rule; `expect_skip` words are asserted separately.
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

// The parity core: identity, root, and multiplicity, each relation named at its assertion site.

fn assert_word_parity(
    label: &str,
    grammar: &Grammar,
    morpher: &Morpher,
    expect: &CommittedWord,
) -> OccurrenceIdentities {
    let outcome = morpher.parse_word(&expect.word);
    assert!(
        !outcome.invalid_shape,
        "{label}: word {:?} unexpectedly failed to segment; a committed adapter-visible, \
         non-expect_skip word must have an analysis set (possibly empty)",
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

    // -- MULTIPLICITY: the multiset relation, strictly stronger than the set-equality parity relation.
    assert_eq!(
        occurrence.raw_analyses() as usize,
        expect.raw_parses,
        "{label}: word {:?} -- MULTISET cardinality disagrees with the committed `parses:` row \
         count. Committed rows are sorted-but-not-deduped, so this count is a measured \
         multiplicity, not a formatting artifact. Observed identities: {:?}",
        expect.word,
        occurrence.entries()
    );

    // -- SET, bounded by the committed record only.
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
    }

    // -- ROOT: a well-formedness floor only (a root position must index its own morpheme sequence).
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

    // -- v1 scope: a guessed or supplied-root analysis is not evidence about the compiled grammar.
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

/// One exercise, run: the fixture anchored against its signatures, then every word's identity set.
struct ExerciseRun {
    label: String,
    grammar: Grammar,
    words: WordsYaml,
    occurrences: BTreeMap<String, OccurrenceIdentities>,
}

impl ExerciseRun {
    /// The identity set for one pinned word; panics if the word is not pinned, so a dropped row breaks loudly.
    fn at(&self, word: &str) -> &OccurrenceIdentities {
        self.occurrences.get(word).unwrap_or_else(|| {
            panic!(
                "{}: the word {word:?} this exercise reasons about is not pinned in the fixture's \
                 own words.yaml (or is guess-only / expect_skip, hence excluded)",
                self.label
            )
        })
    }

    /// Assert one pinned word has EXACTLY `n` distinct identities.
    fn expect_identities(&self, word: &str, n: usize, why: &str) {
        let occurrence = self.at(word);
        assert_eq!(
            occurrence.len(),
            n,
            "{}: word {word:?} must have exactly {n} distinct identities ({why}); got {:?}",
            self.label,
            occurrence.entries()
        );
    }

    /// Assert one pinned word has NO analysis at all -- a negative control.
    fn expect_refused(&self, word: &str, why: &str) {
        let occurrence = self.at(word);
        assert!(
            occurrence.is_empty(),
            "{}: word {word:?} must have NO analysis ({why}); got {:?}",
            self.label,
            occurrence.entries()
        );
    }

    /// The single identity of a word pinned to exactly one, as its ordered morpheme-key sequence.
    fn sole_sequence(&self, word: &str) -> Vec<MorphemeKey> {
        let occurrence = self.at(word);
        assert_eq!(
            occurrence.len(),
            1,
            "{}: word {word:?} must have exactly one identity for this claim; got {:?}",
            self.label,
            occurrence.entries()
        );
        occurrence.entries()[0].identity.morphemes.clone()
    }
}

/// Anchors the fixture against its committed signatures, so per-word work below refines existing ground truth rather than drifting independently.
fn anchor_against_committed_signatures(label: &str, grammar: &Grammar, words: &WordsYaml) {
    let morpher = Morpher::new(grammar, usize::MAX).with_memo(true);
    let checked = assert_matches_oracle(label, words, &morpher);
    assert!(
        checked > 0,
        "{label}: replayed zero words -- a fixture that checks nothing is not an exercise"
    );
}

/// Every committed word's identity set for one fixture, keyed by word; kept separate so `Morpher`'s borrow of `grammar` ends here.
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

fn run_exercise(exercise: &Exercise) -> ExerciseRun {
    let fixture = fixture_of(exercise);
    let label = fixture.label();
    let grammar = load(&fixture.load_grammar_xml(), &label);
    let words = fixture.load_words_yaml();

    assert!(
        words.skip_in_generic_replay().is_none(),
        "{label}: this fixture is skipped by the generic replay ({:?}), so it has no signature \
         ground truth to refine and cannot be a 7.8 exercise",
        words.skip_in_generic_replay()
    );

    anchor_against_committed_signatures(&label, &grammar, &words);

    let expectations = committed_words(&words);
    assert!(
        !expectations.is_empty(),
        "{label}: no adapter-visible, non-expect_skip committed words"
    );
    let occurrences = occurrences_for(&label, &grammar, &expectations);

    ExerciseRun {
        label,
        grammar,
        words,
        occurrences,
    }
}

// Grammar-side (TOP end) predicates, computed from `pg_grammar::model` rather than restated from a fixture comment.

fn prule_xml_id(def: &PhonRuleDef) -> &str {
    match def {
        PhonRuleDef::Rewrite(rule) => &rule.xml_id,
        PhonRuleDef::Metathesis(rule) => &rule.xml_id,
    }
}

/// The morpheme key whose committed `<MorphemeId>` is `morph_id`; panics unless exactly one match resolves.
fn morpheme_key_of(grammar: &Grammar, morph_id: &str, label: &str) -> MorphemeKey {
    let matches: Vec<&str> = grammar
        .morphemes
        .iter()
        .filter(|info| info.morph_id.as_deref() == Some(morph_id))
        .map(|info| info.xml_key.as_str())
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "{label}: expected exactly one morpheme with <MorphemeId>{morph_id}</MorphemeId>, found \
         {matches:?}"
    );
    Some(matches[0].to_string())
}

/// The morpheme key of the morpheme a morphological rule realizes, or `None` for a compounding rule.
fn morpheme_key_of_mrule(grammar: &Grammar, id: MRuleId) -> MorphemeKey {
    let morpheme = match grammar.mrules.get(id.0 as usize) {
        Some(MorphRuleDef::AffixProcess(def)) => def.morpheme,
        Some(MorphRuleDef::Realizational(def)) => def.morpheme,
        _ => return None,
    };
    grammar
        .morphemes
        .get(morpheme.0 as usize)
        .map(|info| info.xml_key.clone())
}

/// Every morphological rule in `grammar` whose exponence is zero: every subrule copies its single input part verbatim, excluding both inserting and truncating rules.
fn zero_exponence_rules(grammar: &Grammar) -> Vec<(MorphemeKey, String)> {
    let mut out = Vec::new();
    for rule in &grammar.mrules {
        let (morpheme, name) = match rule {
            MorphRuleDef::AffixProcess(def) => (def.morpheme, def.name.clone()),
            MorphRuleDef::Realizational(def) => (def.morpheme, def.name.clone()),
            MorphRuleDef::Compounding(_) => continue,
        };
        let Some(allomorphs) = rule.affix_allomorphs() else {
            continue;
        };
        if allomorphs.is_empty() {
            continue;
        }
        let zero = allomorphs.iter().all(|allomorph| {
            allomorph.lhs.len() == 1 && allomorph.rhs == [OutputAction::Copy(PartRef::Input(0))]
        });
        if zero {
            // `MorphemeKey` is `Option<String>`; `None` is reserved for a fabricated guessed root, never a table row.
            let key: MorphemeKey = grammar
                .morphemes
                .get(morpheme.0 as usize)
                .map(|info| info.xml_key.clone());
            out.push((key, name.unwrap_or_default()));
        }
    }
    out
}

/// The distinct `Representation` strings a character-definition table declares for its SEGMENTS.
fn segment_representations(grammar: &Grammar, table_index: usize) -> BTreeSet<String> {
    grammar.char_tables[table_index]
        .iter()
        .filter(|(_, def)| def.kind() == CharDefKind::Segment)
        .flat_map(|(_, def)| def.representations().iter().cloned())
        .collect()
}

/// The maximum number of distinct phonological rules any single committed parse records, per `words.yaml`'s own `rules:` lists.
fn committed_cascade_depth(run: &ExerciseRun) -> usize {
    let declared: BTreeSet<&str> = run.grammar.prules.iter().map(prule_xml_id).collect();
    run.words
        .words
        .iter()
        .flat_map(|word| word.parses.iter())
        .map(|parse| {
            parse
                .rules
                .iter()
                .filter(|rule| declared.contains(rule.as_str()))
                .count()
        })
        .max()
        .unwrap_or(0)
}

// Mechanism 1: template order / co-occurrence.

/// Exercise 1 of template order/co-occurrence: AffixTemplate slot order, obligatory slot outermost.
/// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.
#[test]
fn template_order_exercise_slot_sequence_and_obligatory_slot() {
    let run = run_exercise(EX_TEMPLATE_SLOT_ORDER);
    let label = &run.label;

    // -- TOP end.
    let ordering_template = run
        .grammar
        .templates
        .iter()
        .find(|template| {
            template.slots.len() >= 3
                && template.slots.iter().any(|slot| !slot.optional)
                && template.slots.iter().filter(|slot| slot.optional).count() >= 2
        })
        .unwrap_or_else(|| {
            panic!(
                "{label}: a template-ORDER exercise needs a template with >=3 slots, >=1 \
                 obligatory and >=2 optional; this grammar declares {:?}",
                run.grammar
                    .templates
                    .iter()
                    .map(|t| (t.name.clone(), t.slots.len()))
                    .collect::<Vec<_>>()
            )
        });
    for slot in &ordering_template.slots {
        assert!(
            !slot.rules.is_empty(),
            "{label}: an empty template slot cannot order anything ({:?})",
            slot.name
        );
    }

    // -- TOP end: the obligatory slot is outermost and no slot holds more than one rule, keeping this independent of the other template exercise.
    assert!(
        !ordering_template
            .slots
            .last()
            .expect("a template with >=3 slots has a last slot")
            .optional,
        "{label}: the obligatory slot must be OUTERMOST here; that placement is what makes the \
         reverse-order acceptance below reachable, and it is what distinguishes this exercise from \
         the obligatory-innermost one"
    );
    for slot in &ordering_template.slots {
        assert_eq!(
            slot.rules.len(),
            1,
            "{label}: no slot here may hold more than one rule -- a disjunctive slot is the OTHER \
             template exercise's construct, and if this grammar grew one the two exercises would \
             stop being independent ({:?})",
            slot.name
        );
    }

    // -- BOTTOM end, claim 1: the obligatory slot is enforced.
    run.expect_refused(
        "andik",
        "the template's obligatory final slot is unfilled, so the template can never produce this \
         output",
    );

    // -- claim 2: the two optional slots compose independently.
    run.expect_identities(
        "andika",
        1,
        "obligatory slot only, both optional slots skipped",
    );
    run.expect_identities("andikisha", 1, "optional slot 1 + obligatory slot");
    run.expect_identities("andikila", 1, "optional slot 2 + obligatory slot");

    // -- claim 3 and 4.
    run.expect_identities(
        "andikishila",
        1,
        "all three slots filled, declared slot order",
    );
    run.expect_identities(
        "andikilisha",
        1,
        "all three slots filled, reverse peel order",
    );

    let forward = run.sole_sequence("andikishila");
    let reverse = run.sole_sequence("andikilisha");
    assert_ne!(
        forward, reverse,
        "{label}: the two extension orders must be DISTINCT ordered morpheme sequences -- if they \
         are equal, template order is not observable in the identity at all and this exercise \
         proves nothing"
    );
    let forward_set: BTreeSet<&MorphemeKey> = forward.iter().collect();
    let reverse_set: BTreeSet<&MorphemeKey> = reverse.iter().collect();
    assert_eq!(
        forward_set, reverse_set,
        "{label}: the two orders must use the SAME morphemes -- an unequal set would mean the pair \
         differs in something other than order, and the order claim would be unproven"
    );
    assert!(
        !run.at("andikishila").same_identities(run.at("andikilisha")),
        "{label}: the PROGRAM's parity relation (deduplicated identity SET equality) must \
         distinguish the two orders; if it does not, order is invisible to certification"
    );
}

/// Exercise 2 of template order/co-occurrence: a disjunctive slot, with template order enforced (the converse of exercise 1).
/// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.
#[test]
fn template_order_exercise_disjunctive_slot_and_enforced_order() {
    let run = run_exercise(EX_TEMPLATE_DISJUNCTIVE_SLOT_AND_ENFORCED_ORDER);
    let label = &run.label;

    // -- TOP end.
    let template = run
        .grammar
        .templates
        .iter()
        .find(|template| {
            template.slots.len() >= 6
                && template.slots.iter().filter(|slot| !slot.optional).count() >= 3
                && template.slots.iter().filter(|slot| slot.optional).count() >= 3
        })
        .unwrap_or_else(|| {
            panic!(
                "{label}: this exercise needs a template with >=6 slots, >=3 obligatory and >=3 \
                 optional; this grammar declares {:?}",
                run.grammar
                    .templates
                    .iter()
                    .map(|t| (t.name.clone(), t.slots.len()))
                    .collect::<Vec<_>>()
            )
        });

    // The obligatory-innermost shape: two outermost slots optional, an obligatory slot inward of them.
    let count = template.slots.len();
    assert!(
        template.slots[count - 2..].iter().all(|slot| slot.optional),
        "{label}: the two OUTERMOST slots must both be optional -- they are the swappable pair \
         claim 3 is about"
    );
    assert!(
        template.slots[..count - 2]
            .iter()
            .any(|slot| !slot.optional),
        "{label}: at least one slot INWARD of the swappable pair must be obligatory -- that is what \
         stops a fixpoint retry from rescuing the reverse order, and it is the mirror image of the \
         obligatory-OUTERMOST placement the other template exercise asserts for itself"
    );

    // The DISJUNCTIVE slot, and its members as morpheme keys.
    let disjunctive = template
        .slots
        .iter()
        .find(|slot| slot.rules.len() >= 2)
        .unwrap_or_else(|| {
            panic!(
                "{label}: this exercise needs one slot holding TWO OR MORE rules -- that is the \
                 DISJUNCTIVE third of the construct, and it is what the other template exercise \
                 asserts it does not have"
            )
        });
    let members: Vec<MorphemeKey> = disjunctive
        .rules
        .iter()
        .map(|id| morpheme_key_of_mrule(&run.grammar, *id))
        .collect();
    assert!(
        members.len() >= 2 && members.iter().all(|key| key.is_some()),
        "{label}: every rule in the disjunctive slot must resolve to a morpheme key; got {members:?}"
    );

    // -- BOTTOM end. Claim 1: obligatory slots enforced from the inner edge.
    run.expect_refused(
        "kal",
        "the bare root omits all three obligatory slots at once",
    );
    run.expect_refused(
        "yidkal",
        "one obligatory slot is omitted -- the template cannot complete without it, the mirror of \
         the other template exercise's missing-outermost-slot control",
    );

    // Claim 2: the obligatory core, then each optional slot added.
    run.expect_identities(
        "shiyidkal",
        1,
        "the minimal well-formed word: the three obligatory slots only",
    );
    run.expect_identities(
        "bishiyidkal",
        1,
        "the obligatory core plus the innermost optional slot",
    );
    run.expect_identities("gabishiyidkal", 1, "plus the next optional slot outward");
    run.expect_identities(
        "hogabishiyidkal",
        1,
        "all six slots filled, in the template's own declared order",
    );

    // Claim 3: THE order claim, and the one exercise 1 cannot make.
    run.expect_refused(
        "gahobishiyidkal",
        "the two outermost optional slots in REVERSE order must be REFUSED here, because three \
         obligatory slots lie between that pair and the root: the single-pass outer-to-inner \
         analysis walk reaches an obligatory slot out of turn and dies. An engine that rescued every \
         out-of-order optional pair by fixpoint retry -- the behaviour the OTHER template exercise \
         pins as correct for ITS grammar -- admits this word",
    );

    // Claim 4: the discontinuous dependency is a real gate, with its own non-vacuity control.
    run.expect_identities(
        "shiwodkal",
        1,
        "the other member of the disjunctive slot is independently well-formed on its own terms",
    );
    run.expect_refused(
        "bishiwodkal",
        "the outer optional slot's requirement is set TWO slots inward, and the intervening \
         disjunctive choice went the other way -- every slot's own requirement holds individually, \
         so without the discontinuous gate this word would parse",
    );

    // Claim 5: the disjunctive slot admits exactly one member per identity, and both are reachable.
    for (word, occurrence) in &run.occurrences {
        for entry in occurrence.entries() {
            let present = members
                .iter()
                .filter(|member| entry.identity.morphemes.contains(member))
                .count();
            assert!(
                present <= 1,
                "{label}: word {word:?} -- an identity carries {present} members of one DISJUNCTIVE \
                 slot ({:?}); a disjunctive slot admits at most one",
                entry.identity.morphemes
            );
        }
    }
    for member in &members {
        assert!(
            run.occurrences.values().any(|occurrence| {
                occurrence
                    .entries()
                    .iter()
                    .any(|entry| entry.identity.morphemes.contains(member))
            }),
            "{label}: the disjunctive-slot member {member:?} appears in no identity at all -- \
             without every member being reachable, the at-most-one claim above holds vacuously"
        );
    }
}

/// Exercise 3 of template order/co-occurrence: morpheme and allomorph co-occurrence.
/// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.
#[test]
fn co_occurrence_exercise_adjacency_and_granularity() {
    let run = run_exercise(EX_CO_OCCURRENCE_ADJACENCY);
    let label = &run.label;

    // -- TOP end: morpheme-level constraints, both polarities, >=4 adjacency kinds.
    let morpheme_rules: Vec<_> = run
        .grammar
        .morphemes
        .iter()
        .flat_map(|info| info.co_occurrence.iter())
        .collect();
    assert!(
        morpheme_rules.len() >= 4,
        "{label}: a co-occurrence exercise needs several MorphemeCoOccurrenceRules; found {}",
        morpheme_rules.len()
    );
    let adjacencies: BTreeSet<&str> = morpheme_rules
        .iter()
        .map(|rule| match rule.adjacency {
            CoOccurrenceAdjacency::Anywhere => "anywhere",
            CoOccurrenceAdjacency::SomewhereToLeft => "somewhereToLeft",
            CoOccurrenceAdjacency::SomewhereToRight => "somewhereToRight",
            CoOccurrenceAdjacency::AdjacentToLeft => "adjacentToLeft",
            CoOccurrenceAdjacency::AdjacentToRight => "adjacentToRight",
        })
        .collect();
    assert!(
        adjacencies.len() >= 4,
        "{label}: the adjacency KINDS are what the word pairs below discriminate between; at least \
         four must be declared, found {adjacencies:?}"
    );
    assert!(
        morpheme_rules.iter().any(|rule| rule.require),
        "{label}: no `require`-polarity co-occurrence rule"
    );
    assert!(
        morpheme_rules.iter().any(|rule| !rule.require),
        "{label}: no `exclude`-polarity co-occurrence rule"
    );

    // -- TOP end: at least one ALLOMORPH-level constraint, the second granularity.
    let allomorph_rules = run
        .grammar
        .entries
        .iter()
        .flat_map(|entry| entry.allomorphs.iter())
        .flat_map(|allomorph| allomorph.co_occurrence.iter())
        .count()
        + run
            .grammar
            .mrules
            .iter()
            .filter_map(|rule| rule.affix_allomorphs())
            .flatten()
            .flat_map(|allomorph| allomorph.co_occurrence.iter())
            .count();
    assert!(
        allomorph_rules >= 1,
        "{label}: this exercise's granularity claim needs an AllomorphCoOccurrenceRule; found none"
    );

    // -- BOTTOM end. Pair 1: an `exclude`-polarity constraint (anywhere).
    run.expect_identities("sipuncha", 1, "both exclusion constraints pass");
    run.expect_refused("walaknkicha", "an `exclude` co-occurrence constraint fires");

    // Pair 2: a `require ... somewhereToLeft` constraint, satisfied vs unsatisfied.
    run.expect_identities(
        "walaknitan",
        1,
        "the required partner is somewhere to the left",
    );
    run.expect_refused("walakntan", "the required partner is absent entirely");

    // Pair 3 vs pair 4: the adjacency discrimination -- same configuration, opposite verdicts.
    run.expect_identities(
        "walaknichiktan",
        1,
        "`somewhereToLeft` tolerates an intervening morpheme",
    );
    run.expect_identities(
        "walakniwas",
        1,
        "`adjacentToLeft` satisfied with no intervener",
    );
    run.expect_refused(
        "walaknichikwas",
        "`adjacentToLeft` must REFUSE the same intervening morpheme `somewhereToLeft` tolerates -- \
         collapsing the two kinds admits this word",
    );

    // Pair 5: the rightward mirror, again distinguishing `somewhere` from `adjacent`.
    run.expect_identities("siputukumi", 1, "`somewhereToRight` satisfied");
    run.expect_refused(
        "siputuku",
        "`somewhereToRight` requirement unmet -- no partner at all",
    );
    run.expect_identities(
        "sipulupami",
        1,
        "`adjacentToRight` satisfied with no intervener",
    );
    run.expect_refused(
        "sipulupachikmi",
        "`adjacentToRight` must REFUSE an intervening morpheme",
    );

    // Pair 6: GRANULARITY. Two allomorphs of one morpheme; only one carries the constraint.
    run.expect_identities(
        "takincha",
        1,
        "this root's allomorph carries no co-occurrence constraint",
    );
    run.expect_refused(
        "kantancha",
        "the OTHER allomorph of the same morpheme does carry one -- an engine evaluating \
         co-occurrence per MORPHEME rather than per ALLOMORPH admits this word",
    );
}

// Mechanism 2: cascade / strata.

/// Exercise 1 of cascade/strata: an ordered phonological-rule cascade inside one stratum.
/// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.
#[test]
fn cascade_exercise_ordered_phonological_rule_chain() {
    let run = run_exercise(EX_CASCADE_ORDERED_RULES);
    let label = &run.label;

    // -- TOP end.
    assert_eq!(
        run.grammar.strata.len(),
        1,
        "{label}: this exercise's independence from the two STRATA exercises rests on it having \
         exactly one stratum, so that no stratum-ordering defect can reach it"
    );
    let cascade = &run.grammar.strata[0].prules;
    assert!(
        cascade.len() >= 3,
        "{label}: a CASCADE needs at least three ordered phonological rules; the stratum declares \
         {}",
        cascade.len()
    );
    let ordered_ids: Vec<&str> = cascade
        .iter()
        .map(|id| prule_xml_id(&run.grammar.prules[id.0 as usize]))
        .collect();
    assert_eq!(
        ordered_ids.iter().collect::<BTreeSet<_>>().len(),
        ordered_ids.len(),
        "{label}: the stratum's phonological-rule order lists a rule twice ({ordered_ids:?}); the \
         cascade claim assumes each stage is distinct"
    );
    let depth = committed_cascade_depth(&run);
    assert!(
        depth >= 3,
        "{label}: the fixture's own committed records must show a single parse driving at least \
         three phonological rules, or there is no measured cascade here; deepest observed is \
         {depth}"
    );

    // -- BOTTOM end.
    run.expect_identities(
        "kutagida",
        1,
        "the deep-cascade word: three phonological rules in the stratum's declared order",
    );
    run.expect_identities(
        "semitide",
        1,
        "the harmony stage correctly does not fire (nearest vowel is of the wrong class), the \
         epenthesis stage still does",
    );
    run.expect_identities(
        "unitide",
        1,
        "the harmony stage correctly does not fire (its bounded transparency span is exhausted \
         before any vowel is reached) while the epenthesis stage fires at two seams in one pass",
    );
    run.expect_refused(
        "kutak",
        "the obligatory morphological slot is unfilled, so no cascade output exists",
    );
    run.expect_refused(
        "kutakler",
        "the same obligatory slot is unfilled with a further suffix present",
    );
    run.expect_refused("untda", "the un-epenthesized surface is not reachable");

    // The three positives must be three DIFFERENT analyses, not one word seen thrice.
    let sets: Vec<BTreeSet<Vec<MorphemeKey>>> = ["kutagida", "semitide", "unitide"]
        .into_iter()
        .map(|word| {
            run.at(word)
                .entries()
                .iter()
                .map(|entry| entry.identity.morphemes.clone())
                .collect()
        })
        .collect();
    for (left, right) in [(0, 1), (0, 2), (1, 2)] {
        assert!(
            sets[left].is_disjoint(&sets[right]),
            "{label}: the three cascade words must have DISJOINT morpheme sequences, or they are \
             not three independent traversals of the cascade"
        );
    }
}

/// Exercise 2 of cascade/strata: a derivation chain across strata.
/// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.
#[test]
fn strata_exercise_cross_stratum_derivation_feed() {
    let run = run_exercise(EX_STRATA_CROSS_STRATUM_FEED);
    let label = &run.label;

    // -- TOP end.
    assert!(
        run.grammar.strata.len() >= 2,
        "{label}: a STRATA exercise needs at least two strata; found {}",
        run.grammar.strata.len()
    );
    let deriv = morpheme_key_of(&run.grammar, "DERIV", label);
    let infl = morpheme_key_of(&run.grammar, "INFL", label);
    let stratum_of = |key: &MorphemeKey| {
        let wanted = key
            .as_deref()
            .expect("a committed morpheme key is never None");
        run.grammar
            .morphemes
            .iter()
            .find(|info| info.xml_key == wanted)
            .map(|info| info.stratum)
            .unwrap_or_else(|| panic!("{label}: no morpheme with xml key {wanted:?}"))
    };
    assert_ne!(
        stratum_of(&deriv),
        stratum_of(&infl),
        "{label}: the derivational and inflectional rules must be owned by DIFFERENT strata, or \
         nothing here crosses a stratum boundary and this is a second cascade exercise wearing a \
         strata label"
    );

    // -- TOP end: this grammar must not be a per-stratum-table grammar, keeping it independent of the third cascade/strata exercise.
    let tables: BTreeSet<u16> = run
        .grammar
        .strata
        .iter()
        .map(|stratum| stratum.table.0)
        .collect();
    assert_eq!(
        tables.len(),
        1,
        "{label}: every stratum here shares ONE character table; the per-stratum-table claim \
         belongs to the bistratal exercise, and if this grammar grew a second table the two \
         exercises would stop being independent"
    );

    // -- BOTTOM end.
    run.expect_identities("nuna", 1, "the bare root on the deep stratum");
    run.expect_identities("nunaliq", 1, "the deep stratum's derivation applied");
    run.expect_identities(
        "nunaliqvuq",
        1,
        "the deep stratum's output feeds the shallow stratum's inflection",
    );
    run.expect_refused(
        "nunavuq",
        "the shallow stratum's inflection cannot attach to the un-derived root -- an engine \
         ignoring stratum membership admits this",
    );

    let spanning = run.sole_sequence("nunaliqvuq");
    assert!(
        spanning.contains(&deriv) && spanning.contains(&infl),
        "{label}: the cross-stratum word's single identity must contain BOTH the deep and the \
         shallow morpheme key; got {spanning:?}"
    );
    let intermediate = run.sole_sequence("nunaliq");
    assert!(
        intermediate.contains(&deriv) && !intermediate.contains(&infl),
        "{label}: the intermediate word must carry the deep morpheme and NOT the shallow one, or \
         the two strata are not distinguishable here; got {intermediate:?}"
    );
}

/// Exercise 3 of cascade/strata: one character-definition table per stratum.
/// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.
#[test]
fn strata_exercise_per_stratum_character_table() {
    let run = run_exercise(EX_STRATA_PER_STRATUM_TABLE);
    let label = &run.label;

    // -- TOP end.
    let per_stratum: Vec<usize> = run
        .grammar
        .strata
        .iter()
        .map(|stratum| stratum.table.0 as usize)
        .collect();
    let distinct: BTreeSet<usize> = per_stratum.iter().copied().collect();
    assert!(
        distinct.len() >= 2,
        "{label}: this exercise needs at least two DIFFERENT per-stratum tables; strata reference \
         {per_stratum:?}"
    );
    let mut indices: Vec<usize> = distinct.into_iter().collect();
    indices.sort_unstable();
    let first = segment_representations(&run.grammar, indices[0]);
    let second = segment_representations(&run.grammar, indices[1]);
    assert!(
        !first.is_disjoint(&second),
        "{label}: the two tables must SHARE at least one segment representation -- the shared \
         spelling with divergent segment identity IS this fixture's mechanism; {first:?} vs \
         {second:?}"
    );
    assert!(
        first.difference(&second).next().is_some() && second.difference(&first).next().is_some(),
        "{label}: each table must declare at least one representation the other lacks, or 'two \
         tables' is a distinction with no observable content"
    );

    // -- BOTTOM end: the final stratum's table scopes surface tokenization.
    let morpher = Morpher::new(&run.grammar, usize::MAX).with_memo(true);
    let skipped: Vec<&str> = run
        .words
        .words
        .iter()
        .filter(|word| word.expect_skip)
        .map(|word| word.word.as_str())
        .collect();
    assert!(
        !skipped.is_empty(),
        "{label}: this exercise's load-bearing claim is about words that do NOT tokenize; the \
         fixture pins none, so there is nothing to assert"
    );
    for word in &skipped {
        assert!(
            morpher.parse_word(word).invalid_shape,
            "{label}: {word:?} is declared only on a NON-final stratum's table, so it must not \
             tokenize against the final stratum's table -- an engine that merged the two tables \
             makes it tokenize, which is exactly this exercise's falsifier"
        );
    }

    run.expect_identities(
        "des",
        1,
        "a final-stratum root over the final stratum's own segments",
    );
    run.expect_identities("sed", 1, "a second final-stratum root, same table");
    run.expect_refused(
        "eds",
        "a well-formed string over the final table's alphabet that is no entry's shape -- without \
         this control, alphabet coverage alone could manufacture analyses",
    );
}

// Mechanism 3: lexical class -- a class declared on the entry/allomorph that partitions the lexicon.
// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.

/// Exercise 1 of lexical class: allomorph-level class with overlapping regions and a default.
/// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.
#[test]
fn lexical_class_exercise_allomorph_level_stem_name_regions() {
    let run = run_exercise(EX_LEXICAL_CLASS_ALLOMORPH_REGIONS);
    let label = &run.label;

    // -- TOP end.
    assert!(
        run.grammar.stem_names.len() >= 2,
        "{label}: this exercise's overlap claim needs at least two stem names; found {}",
        run.grammar.stem_names.len()
    );
    for (index, stem_name) in run.grammar.stem_names.iter().enumerate() {
        assert!(
            stem_name.regions.len() >= 2,
            "{label}: stem name {index} ({:?}) declares {} region(s); the region ALGEBRA this \
             exercise turns on needs at least two per class",
            stem_name.name,
            stem_name.regions.len()
        );
    }
    let partitioned_entry = run
        .grammar
        .entries
        .iter()
        .find(|entry| {
            let restricted: BTreeSet<u32> = entry
                .allomorphs
                .iter()
                .filter_map(|allomorph| allomorph.stem_name.map(|id| id.0))
                .collect();
            restricted.len() >= 2
                && entry
                    .allomorphs
                    .iter()
                    .any(|allomorph| allomorph.stem_name.is_none())
        })
        .unwrap_or_else(|| {
            panic!(
                "{label}: no lexical entry carries two DIFFERENTLY-classed allomorphs plus an \
                 unrestricted default; without all three, the twelve-cell table below is not a \
                 class partition"
            )
        });
    assert!(
        partitioned_entry.allomorphs.len() >= 3,
        "{label}: the partitioned entry must have at least three allomorphs"
    );

    // -- BOTTOM end: the committed twelve-cell table (default allomorph admitted bare only).
    run.expect_identities(
        "mun",
        1,
        "unrestricted default allomorph, no class feature assigned",
    );
    for refused in ["muno", "muns", "munt"] {
        run.expect_refused(
            refused,
            "the two classes jointly exhaust the feature space, so the region-less default \
             allomorph can never combine with a class-bearing suffix",
        );
    }
    // Class A: regions {feature1, feature2}.
    run.expect_identities("mano", 1, "class A allomorph in its own first region");
    run.expect_identities("mans", 1, "class A allomorph in its own second region");
    run.expect_refused("mant", "class A's regions do not include this feature");
    run.expect_refused(
        "man",
        "a class-restricted allomorph has no region to satisfy when bare",
    );
    // Class B: regions {feature1, feature3} -- overlapping class A on feature1.
    run.expect_identities(
        "mino",
        1,
        "class B allomorph in the region SHARED with class A",
    );
    run.expect_refused("mins", "class B's regions do not include this feature");
    run.expect_identities("mint", 1, "class B allomorph in its own non-shared region");
    run.expect_refused("min", "same reason as class A's bare form");

    // -- The overlap claim, stated with the PROGRAM's parity relation.
    assert!(
        run.at("mano").same_identities(run.at("mino")),
        "{label}: the two class allomorphs are allomorphs of ONE morpheme, so their analyses in the \
         SHARED region must be the same identity SET; only in the class-A reading: {:?}; only in \
         the class-B reading: {:?}. An inequality here means the identity relation became \
         allomorph-sensitive, and lexical class would be indistinguishable from two lexemes.",
        run.at("mano").identities_absent_from(run.at("mino")),
        run.at("mino").identities_absent_from(run.at("mano"))
    );
    assert_eq!(
        run.at("mano").raw_analyses(),
        run.at("mino").raw_analyses(),
        "{label}: the MULTIPLICITY must match too -- the set equality just asserted is blind to it \
         by design"
    );
}

/// Exercise 2 of lexical class: rule-level class requirement, independent of exercise 1's region algebra.
/// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.
#[test]
fn lexical_class_exercise_rule_level_required_stem_name() {
    let run = run_exercise(EX_LEXICAL_CLASS_RULE_LEVEL);
    let label = &run.label;

    // -- TOP end.
    let required: BTreeSet<u32> = run
        .grammar
        .mrules
        .iter()
        .filter_map(|rule| match rule {
            MorphRuleDef::AffixProcess(def) => def.required_stem_name.map(|id| id.0),
            _ => None,
        })
        .collect();
    assert!(
        !required.is_empty(),
        "{label}: this exercise needs a rule declaring `requiredStemName`; none does"
    );
    let classed = run
        .grammar
        .entries
        .iter()
        .flat_map(|entry| entry.allomorphs.iter())
        .filter(|allomorph| {
            allomorph
                .stem_name
                .is_some_and(|id| required.contains(&id.0))
        })
        .count();
    let unclassed = run
        .grammar
        .entries
        .iter()
        .flat_map(|entry| entry.allomorphs.iter())
        .filter(|allomorph| allomorph.stem_name.is_none())
        .count();
    assert!(
        classed >= 1 && unclassed >= 1,
        "{label}: the lexicon must be PARTITIONED by the required class -- {classed} root \
         allomorph(s) carry it and {unclassed} carry none; a requirement nothing can fail is not a \
         partition"
    );

    // -- TOP end: exactly one stem name with exactly one region -- a single region cannot overlap another, keeping this independent of exercise 1's region algebra.
    assert_eq!(
        run.grammar.stem_names.len(),
        1,
        "{label}: this exercise's independence from the region-algebra exercise rests on there \
         being exactly ONE class here; found {}",
        run.grammar.stem_names.len()
    );
    assert_eq!(
        run.grammar.stem_names[0].regions.len(),
        1,
        "{label}: the single class must declare exactly ONE region -- a single region cannot \
         overlap anything, which is what makes this a presence/absence exercise rather than a \
         region-algebra one"
    );

    // -- BOTTOM end.
    run.expect_identities(
        "daleh",
        1,
        "the root allomorph carries the class the rule requires",
    );
    run.expect_refused(
        "daheh",
        "this root allomorph carries no class at all, so the rule's requirement can never be \
         satisfied",
    );
}

// Mechanism 4: allomorph priority.

/// Exercise 1 of allomorph priority: an earlier-indexed allomorph blocks a later one.
/// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.
#[test]
fn allomorph_priority_exercise_earlier_index_blocks() {
    let run = run_exercise(EX_ALLOMORPH_PRIORITY_EARLIER_BLOCKS);
    let label = &run.label;

    // -- TOP end: root disjunctivity (constrained EARLIER, elsewhere LATER).
    let disjunctive_root = run.grammar.entries.iter().any(|entry| {
        entry.allomorphs.len() >= 2
            && !entry.allomorphs[0].environments.is_empty()
            && entry.allomorphs[1].environments.is_empty()
    });
    assert!(
        disjunctive_root,
        "{label}: no lexical entry has an environment-constrained allomorph at an EARLIER index \
         than an unconstrained one; index priority would then have nothing to order"
    );

    // -- TOP end: affix disjunctivity, the same shape one level up.
    let disjunctive_affix = run
        .grammar
        .mrules
        .iter()
        .filter_map(|rule| rule.affix_allomorphs())
        .any(|allomorphs| {
            allomorphs.len() >= 2
                && !allomorphs[0].environments.is_empty()
                && allomorphs[1].environments.is_empty()
        });
    assert!(
        disjunctive_affix,
        "{label}: no affix rule has an environment-constrained subrule at an EARLIER index than an \
         unconstrained one"
    );

    // -- TOP end: the free-fluctuation pair.
    let free_variants = run.grammar.entries.iter().any(|entry| {
        entry.allomorphs.len() >= 2
            && entry
                .allomorphs
                .iter()
                .all(|allomorph| allomorph.environments.is_empty())
    });
    assert!(
        free_variants,
        "{label}: no lexical entry has two allomorphs with IDENTICAL (empty) constraint sets; \
         without the escape case, the blocking claims below could hold for an unconditional \
         rejection"
    );

    // -- BOTTOM end: root disjunctivity.
    run.expect_identities(
        "wak",
        1,
        "the elsewhere allomorph, with the earlier allomorph's environment unsatisfied at word end",
    );
    run.expect_refused(
        "wok",
        "the USED allomorph's own environment fails -- a control, not the priority claim",
    );
    run.expect_refused(
        "wakta",
        "THE blocking row: the later-indexed allomorph was used, but the earlier-indexed \
         alternative's environment is satisfied at the same position and the two do not \
         free-fluctuate. An engine ignoring allomorph index order admits this",
    );
    run.expect_identities(
        "wokta",
        1,
        "the mirror: the earliest-indexed allomorph was used, so there is no earlier alternative to \
         be rejected in favour of",
    );

    // -- BOTTOM end: affix disjunctivity, structurally the same claim one level up.
    run.expect_identities(
        "pakza",
        1,
        "the constrained subrule's own environment is satisfied",
    );
    run.expect_refused(
        "pakda",
        "THE blocking row at the affix level: the elsewhere subrule was used while the earlier, \
         constrained subrule's environment also held at that position",
    );
    run.expect_refused(
        "pitza",
        "the USED subrule's own environment fails -- a control",
    );
    run.expect_identities(
        "pitda",
        1,
        "the mirror: the earlier subrule's candidate environment fails, so no rejection applies",
    );

    // -- BOTTOM end: the free-fluctuation escape, plus the relation claim.
    run.expect_identities("gray", 1, "the first of two free variants");
    run.expect_identities(
        "grey",
        1,
        "the later-indexed free variant is NOT rejected -- identical constraint sets make the two \
         free variants, which short-circuits the disjunctive rejection",
    );
    assert!(
        run.at("gray").same_identities(run.at("grey")),
        "{label}: two free-fluctuating allomorphs of ONE morpheme must project to the SAME identity \
         set -- an identity is allomorph-blind by specification. Only in the first: {:?}; only in \
         the second: {:?}",
        run.at("gray").identities_absent_from(run.at("grey")),
        run.at("grey").identities_absent_from(run.at("gray"))
    );
}

/// Exercise 2 of allomorph priority: a later-indexed allomorph must remain reachable.
/// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.
#[test]
fn allomorph_priority_exercise_later_index_is_reachable() {
    let run = run_exercise(EX_ALLOMORPH_PRIORITY_LATER_REACHABLE);
    let label = &run.label;

    // -- TOP end. "Inserts before the copy" is the structural signature of a wrapping allomorph.
    let inserts_before_copy = |actions: &[OutputAction]| -> bool {
        let copy_at = actions
            .iter()
            .position(|action| matches!(action, OutputAction::Copy(_)));
        match copy_at {
            None => false,
            Some(index) => actions[..index].iter().any(|action| {
                matches!(
                    action,
                    OutputAction::InsertSegments { .. } | OutputAction::InsertContext(_)
                )
            }),
        }
    };
    let mixed_rule = run
        .grammar
        .mrules
        .iter()
        .filter_map(|rule| rule.affix_allomorphs())
        .find(|allomorphs| {
            allomorphs.len() >= 2
                && !inserts_before_copy(&allomorphs[0].rhs)
                && allomorphs[1..]
                    .iter()
                    .any(|allomorph| inserts_before_copy(&allomorph.rhs))
        })
        .unwrap_or_else(|| {
            panic!(
                "{label}: this exercise needs one rule whose FIRST allomorph inserts on one side \
                 only and a LATER allomorph wraps the stem; without that asymmetry, a \
                 first-allomorph-only classification could not go wrong here"
            )
        });
    assert!(
        mixed_rule.len() >= 2,
        "{label}: the mixed rule must declare at least two allomorphs"
    );

    // -- BOTTOM end.
    run.expect_identities(
        "mit",
        1,
        "the bare root, no allomorph of the mixed rule applied",
    );
    run.expect_identities(
        "mits",
        1,
        "the FIRST-declared allomorph: an ordinary one-sided affix",
    );
    run.expect_identities(
        "kemitan",
        1,
        "the SECOND-declared allomorph: the wrapping shape. An engine classifying the rule by its \
         first allomorph alone loses this surface entirely",
    );

    // -- The priority claim: same morpheme, different position.
    let first = run.sole_sequence("mits");
    let later = run.sole_sequence("kemitan");
    assert_ne!(
        first, later,
        "{label}: the two allomorphs must place the affix DIFFERENTLY in the ordered morpheme \
         sequence, or 'non-first allomorph' has no observable consequence"
    );
    assert_eq!(
        first.iter().collect::<BTreeSet<_>>(),
        later.iter().collect::<BTreeSet<_>>(),
        "{label}: the two derivations must use the SAME morphemes -- they are two allomorphs of one \
         rule, and an unequal set would mean something else differs"
    );
    let root_positions: BTreeSet<i32> = ["mits", "kemitan"]
        .into_iter()
        .map(|word| run.at(word).entries()[0].identity.root_index)
        .collect();
    assert_eq!(
        root_positions.len(),
        2,
        "{label}: the one-sided and wrapping allomorphs must put the ROOT at different positions; \
         got {root_positions:?}"
    );
    assert!(
        !run.at("mits").same_identities(run.at("kemitan")),
        "{label}: the two derivations are two analyses, not one -- the program's parity relation \
         must distinguish them"
    );
}

// Mechanism 5: zero morphology.

/// Exercise 1 of zero morphology: a silent rule in a mandatory template slot.
/// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.
#[test]
fn zero_morphology_exercise_silent_mandatory_template_slot() {
    let run = run_exercise(EX_ZERO_MORPH_SILENT_TEMPLATE_SLOT);
    let label = &run.label;

    // -- TOP end.
    let zero = morpheme_key_of(&run.grammar, "VAC", label);
    let structural = zero_exponence_rules(&run.grammar);
    assert_eq!(
        structural.len(),
        1,
        "{label}: exactly one rule in this grammar must be zero-exponence, or the word-level counts \
         below stop identifying which morpheme is silent; found {structural:?}"
    );
    assert_eq!(
        structural[0].0, zero,
        "{label}: the structurally zero-exponence rule must be the one the committed signatures \
         name"
    );
    let mandatory_slot_rules: BTreeSet<u32> = run
        .grammar
        .templates
        .iter()
        .flat_map(|template| template.slots.iter())
        .filter(|slot| !slot.optional)
        .flat_map(|slot| slot.rules.iter().map(|id| id.0))
        .collect();
    let zero_is_mandatory_slot = run.grammar.mrules.iter().enumerate().any(|(index, rule)| {
        let morpheme = match rule {
            MorphRuleDef::AffixProcess(def) => Some(def.morpheme),
            MorphRuleDef::Realizational(def) => Some(def.morpheme),
            MorphRuleDef::Compounding(_) => None,
        };
        morpheme.is_some_and(|morpheme| {
            run.grammar
                .morphemes
                .get(morpheme.0 as usize)
                .is_some_and(|info| Some(info.xml_key.clone()) == zero)
                && mandatory_slot_rules.contains(&(index as u32))
        })
    });
    assert!(
        zero_is_mandatory_slot,
        "{label}: the zero-exponence rule must sit in a NON-optional template slot -- 'mandatory \
         but silent' is the conjunction this exercise is named for, and it is also what makes the \
         template-composite-pruning falsifier reachable"
    );

    // -- BOTTOM end: the silent morpheme creates ambiguity.
    run.expect_identities(
        "monu",
        2,
        "one surface, two readings: the bare root, and the mandatory-but-silent slot applied alone",
    );
    assert_eq!(
        run.at("monu").raw_analyses(),
        2,
        "{label}: two identities of multiplicity one each, not one identity reached twice"
    );
    assert_eq!(
        run.at("monu").collapsed_paths(),
        0,
        "{label}: nothing should have been deduplicated away here"
    );

    // Every doubled word's extra key must be the SAME key: one zero morpheme, not an assortment.
    let mut doubled = 0usize;
    for (word, occurrence) in &run.occurrences {
        if occurrence.len() != 2 {
            continue;
        }
        doubled += 1;
        let mut sequences: Vec<Vec<MorphemeKey>> = occurrence
            .entries()
            .iter()
            .map(|entry| entry.identity.morphemes.clone())
            .collect();
        sequences.sort_by_key(|sequence| sequence.len());
        let (shorter, longer) = (&sequences[0], &sequences[1]);
        assert_eq!(
            longer.len(),
            shorter.len() + 1,
            "{label}: word {word:?} -- the two readings must differ by exactly ONE morpheme; got \
             {shorter:?} versus {longer:?}"
        );
        let extra: Vec<&MorphemeKey> = {
            let short_set: BTreeSet<&MorphemeKey> = shorter.iter().collect();
            longer
                .iter()
                .filter(|key| !short_set.contains(key))
                .collect()
        };
        assert_eq!(
            extra,
            vec![&zero],
            "{label}: word {word:?} -- the extra morpheme in the longer reading must be the ONE \
             zero morpheme this grammar declares"
        );
    }
    assert!(
        doubled >= 2,
        "{label}: the zero morpheme must double more than one word, or the 'same key everywhere' \
         claim is vacuous; {doubled} word(s) doubled"
    );

    // The zero morpheme is a morpheme, not a root: no identity is it alone.
    let zero_alone: Vec<MorphemeKey> = vec![zero.clone()];
    for (word, occurrence) in &run.occurrences {
        for entry in occurrence.entries() {
            assert_ne!(
                entry.identity.morphemes, zero_alone,
                "{label}: word {word:?} -- no identity may consist of the zero morpheme alone"
            );
        }
    }
}

/// Exercise 2 of zero morphology: zero derivation, outside any template (the converse of exercise 1).
/// See `docs/research/pg-foma-orthogonal-basis-group-a-notes.md`.
#[test]
fn zero_morphology_exercise_zero_derivation_changes_only_category() {
    let run = run_exercise(EX_ZERO_MORPH_ZERO_DERIVATION);
    let label = &run.label;

    // -- TOP end.
    let zero = morpheme_key_of(&run.grammar, "DERIVE", label);
    let structural = zero_exponence_rules(&run.grammar);
    assert_eq!(
        structural.len(),
        1,
        "{label}: exactly one rule in this grammar must be zero-exponence; found {structural:?}"
    );
    assert_eq!(
        structural[0].0, zero,
        "{label}: the structurally zero-exponence rule must be the one the committed signatures \
         name"
    );
    let zero_rule = run
        .grammar
        .mrules
        .iter()
        .filter_map(|rule| match rule {
            MorphRuleDef::AffixProcess(def) => Some(def),
            _ => None,
        })
        .find(|def| {
            run.grammar
                .morphemes
                .get(def.morpheme.0 as usize)
                .is_some_and(|info| Some(info.xml_key.clone()) == zero)
        })
        .unwrap_or_else(|| {
            panic!("{label}: the zero-exponence rule must be an affix-process rule")
        });
    assert!(
        !zero_rule.is_template_rule,
        "{label}: this zero rule must NOT be referenced by any template slot -- that is exactly \
         what keeps it independent of the silent-template-slot exercise, whose falsifier is a \
         template-composite-pruning defect"
    );
    assert_ne!(
        zero_rule.out_syn_fs, zero_rule.required_syn_fs,
        "{label}: a zero-DERIVATION rule's only effect is a category change, so its output feature \
         structure must differ from its requirement"
    );

    // -- BOTTOM end.
    run.expect_identities(
        "pat",
        1,
        "the un-derived word: the zero morpheme is NOT applied, so the category-gated rewrite is \
         not licensed",
    );
    run.expect_identities(
        "bat",
        1,
        "the derived word: the zero morpheme IS applied, licensing the category-gated rewrite",
    );

    let underived = run.sole_sequence("pat");
    assert_eq!(
        underived.len(),
        1,
        "{label}: the un-derived word's identity must be the bare root alone; got {underived:?}"
    );
    assert!(
        !underived.contains(&zero),
        "{label}: the un-derived word must NOT carry the zero morpheme -- a zero morpheme that were \
         freely insertable would show up here as a second analysis or an extra key. This is the \
         over-generation falsifier the silent-template-slot exercise structurally cannot have."
    );

    let derived = run.sole_sequence("bat");
    assert_eq!(
        derived.len(),
        2,
        "{label}: the derived word's identity must be root + zero morpheme; got {derived:?}"
    );
    assert!(
        derived.contains(&zero),
        "{label}: the derived word's identity must contain the zero morpheme's key -- it is the \
         only thing that explains its surface, and it contributes no segment of its own"
    );

    // The equal-length claim is read from the fixture's own pinned word list, not written as literals, so a fixture edit that broke it would break loudly.
    let pinned = |word: &str| -> usize {
        run.words
            .words
            .iter()
            .find(|entry| entry.word == word)
            .map(|entry| entry.word.chars().count())
            .unwrap_or_else(|| panic!("{label}: {word:?} is not pinned in this fixture"))
    };
    assert_eq!(
        pinned("pat"),
        pinned("bat"),
        "{label}: the un-derived and derived surfaces must be the SAME length -- that equality is \
         the direct statement that the zero morpheme contributed no segment, since one of the two \
         carries an extra morpheme and neither carries an extra character"
    );
}

// The basis shape guard.

/// A guard against group A's half of the basis quietly shrinking (every mechanism, at least two exercises each, no shared fixtures).
#[test]
fn group_a_basis_has_two_independent_exercises_per_mechanism() {
    let all = [
        Mechanism::TemplateOrderCoOccurrence,
        Mechanism::CascadeStrata,
        Mechanism::LexicalClass,
        Mechanism::AllomorphPriority,
        Mechanism::ZeroMorphology,
    ];

    // Keyed by mechanism, valued by each exercise's fixture label; a shared fixture shows up as a duplicate here.
    let mut by_mechanism: BTreeMap<Mechanism, Vec<String>> = BTreeMap::new();
    for exercise in EXERCISES {
        assert!(
            !exercise.independent_falsifier.trim().is_empty(),
            "{:?}/{}: every exercise must record the falsifier no sibling exercise can detect",
            exercise.mechanism,
            exercise.name
        );
        by_mechanism
            .entry(exercise.mechanism)
            .or_default()
            .push(format!(
                "{}:{}/{}",
                exercise.root.label(),
                exercise.category,
                exercise.name
            ));
    }

    for mechanism in all {
        let exercises = by_mechanism.get(&mechanism).unwrap_or_else(|| {
            panic!("{mechanism:?} is one of group A's five mechanisms and has no exercise at all")
        });
        assert!(
            exercises.len() >= 2,
            "{mechanism:?} has {} exercise(s); 7.8 asks for at least two",
            exercises.len()
        );
        let distinct: BTreeSet<&String> = exercises.iter().collect();
        assert_eq!(
            distinct.len(),
            exercises.len(),
            "{mechanism:?} points two of its exercises at the SAME fixture ({exercises:?}); two \
             exercises that are one fixture under two names are one exercise"
        );
    }

    assert_eq!(
        by_mechanism.len(),
        all.len(),
        "the exercise table must cover exactly group A's five mechanisms and no others -- the other \
         six of 7.8's list belong to group B"
    );

    // Every named fixture really exists and really loads; silently skipping one would test nothing.
    for exercise in EXERCISES {
        let fixture = fixture_of(exercise);
        let label = fixture.label();
        let grammar = load(&fixture.load_grammar_xml(), &label);
        assert!(
            !grammar.strata.is_empty(),
            "{label}: a grammar with no stratum cannot exercise anything"
        );
        let words = fixture.load_words_yaml();
        assert!(
            !words.words.is_empty(),
            "{label}: a fixture with no pinned words is not an exercise"
        );
        assert!(
            words.skip_in_generic_replay().is_none(),
            "{label}: a fixture the generic replay skips has no signature ground truth and cannot \
             be a 7.8 exercise"
        );
    }
}
