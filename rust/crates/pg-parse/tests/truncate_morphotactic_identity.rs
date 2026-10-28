//! Pins complete morph and rule order for the truncation derivations documented in #506/#480.

use pg_conformance_fixtures::require_fixture;
use pg_grammar_model::model::{AllomorphOwner, Grammar, MorphRuleDef};
use pg_parse::{result_multiset, Morpher, ParseOptions};
use pg_rules::{
    trace::{TraceHandle, TraceType, TreeTraceSink},
    word::{MorphStatus, Word},
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ExpectedRow {
    surface: &'static str,
    morph_ids: Vec<&'static str>,
    applications: Vec<&'static str>,
}

type OccurrenceIdentity = (String, String);
type TraceIdentity = (Vec<OccurrenceIdentity>, Vec<Option<String>>);

fn morph_name(grammar: &Grammar, id: u32) -> String {
    grammar.morphemes[id as usize]
        .morph_id
        .clone()
        .unwrap_or_else(|| grammar.morphemes[id as usize].xml_key.clone())
}

fn occurrence_identity(
    grammar: &Grammar,
    allomorph_id: u32,
    morpheme_id: u32,
) -> OccurrenceIdentity {
    let name = morph_name(grammar, morpheme_id);
    let owner = grammar.allomorph_owners[allomorph_id as usize];
    let label = match owner {
        AllomorphOwner::Root(_, index) => format!("{name}:root:{index}"),
        AllomorphOwner::Affix(_, index) => format!("{name}:affix:{index}"),
    };
    (label, name)
}

fn rule_name(grammar: &Grammar, id: pg_grammar_model::model::MRuleId) -> String {
    let MorphRuleDef::AffixProcess(rule) = &grammar.mrules[id.0 as usize] else {
        panic!("expected an affix-process rule for {:?}", id);
    };
    morph_name(grammar, rule.morpheme.0)
}

fn expected_occurrences(morph_ids: &[&str]) -> Vec<OccurrenceIdentity> {
    morph_ids
        .iter()
        .map(|name| {
            let owner = if name.starts_with("ROOT") {
                "root"
            } else {
                "affix"
            };
            (format!("{name}:{owner}:0"), (*name).to_string())
        })
        .collect()
}

fn expected_signatures(rows: &[ExpectedRow]) -> Vec<String> {
    let mut signatures: Vec<_> = rows
        .iter()
        .map(|row| format!("{}|{}", row.morph_ids.join("+"), row.surface))
        .collect();
    signatures.sort();
    signatures
}

type OwnerOccurrence = (u32, &'static str);

fn owner_occurrence_rows(grammar: &Grammar, surface: &str) -> Vec<Vec<OwnerOccurrence>> {
    let outcome = Morpher::new(grammar, usize::MAX).parse_word(surface);
    assert!(!outcome.capped, "{surface}: parse must finish uncapped");
    assert!(!outcome.invalid_shape, "{surface}: input must segment");

    let mut rows: Vec<_> = outcome
        .structured
        .iter()
        .map(|analysis| {
            analysis
                .morph_occurrences
                .iter()
                .map(|occurrence| {
                    let owner = grammar.allomorph_owners[occurrence.allomorph_id as usize];
                    let owner_kind = match owner {
                        AllomorphOwner::Root(..) => "root",
                        AllomorphOwner::Affix(..) => "affix",
                    };
                    (occurrence.allomorph_id, owner_kind)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    rows.sort();
    rows
}

fn collect_successful_words(sink: &TreeTraceSink, handle: TraceHandle, out: &mut Vec<Word>) {
    let node = sink.node(handle);
    if node.type_ == TraceType::Successful {
        out.push(
            node.output
                .unwrap_or_else(|| panic!("Successful trace node has no output Word")),
        );
    }
    for child in node.children {
        collect_successful_words(sink, child, out);
    }
}

fn trace_identity(grammar: &Grammar, word: &Word) -> TraceIdentity {
    let mut morphs = word.morphs.clone();
    morphs.sort_by_key(|morph| morph.order);
    let occurrences = morphs
        .iter()
        .map(|morph| occurrence_identity(grammar, morph.allomorph.0, morph.morpheme.0))
        .collect();
    // Reverse surface-inward unapplication to compare forward application order.
    let applications = word
        .mrule_apps
        .iter()
        .rev()
        .map(|rule| rule.map(|id| rule_name(grammar, id)))
        .collect();
    (occurrences, applications)
}

fn assert_identity_rows(grammar: &Grammar, word: &str, rows: &[ExpectedRow]) {
    assert!(
        rows.iter().all(|row| row.surface == word),
        "expected rows must belong to {word}"
    );
    let morpher = Morpher::new(grammar, usize::MAX);
    let sink = TreeTraceSink::new();
    let outcome = morpher.parse_word_traced(word, &ParseOptions::default(), &sink);
    assert!(!outcome.capped, "{word}: parse must finish uncapped");
    assert!(!outcome.invalid_shape, "{word}: input must segment");

    let got_signatures = result_multiset(&outcome.analyses);
    let want_signatures = expected_signatures(rows);

    let mut got_morph_orders: Vec<_> = outcome
        .structured
        .iter()
        .map(|analysis| {
            analysis
                .morpheme_ids
                .iter()
                .map(|&id| morph_name(grammar, id))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut want_morph_orders: Vec<_> = rows
        .iter()
        .map(|row| {
            row.morph_ids
                .iter()
                .map(|name| (*name).to_string())
                .collect::<Vec<_>>()
        })
        .collect();
    got_morph_orders.sort();
    want_morph_orders.sort();

    let mut got_occurrences: Vec<_> = outcome
        .structured
        .iter()
        .map(|analysis| {
            analysis
                .morph_occurrences
                .iter()
                .map(|occurrence| {
                    occurrence_identity(grammar, occurrence.allomorph_id, occurrence.morpheme_id)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let mut want_occurrences: Vec<_> = rows
        .iter()
        .map(|row| expected_occurrences(&row.morph_ids))
        .collect();
    got_occurrences.sort();
    want_occurrences.sort();

    let root = sink.root().expect("traced parse must have a root");
    let mut successful_words = Vec::new();
    collect_successful_words(&sink, root, &mut successful_words);
    let mut got_trace_rows: Vec<_> = successful_words
        .iter()
        .map(|success| trace_identity(grammar, success))
        .collect();
    let mut want_trace_rows: Vec<_> = rows
        .iter()
        .map(|row| {
            (
                expected_occurrences(&row.morph_ids),
                row.applications
                    .iter()
                    .map(|name| Some((*name).to_string()))
                    .collect(),
            )
        })
        .collect();
    got_trace_rows.sort();
    want_trace_rows.sort();
    assert_eq!(
        (
            got_signatures,
            got_morph_orders,
            got_occurrences,
            got_trace_rows
        ),
        (
            want_signatures,
            want_morph_orders,
            want_occurrences,
            want_trace_rows
        ),
        "{word}: full parse identities and trace application order must match the independent rows"
    );
}

fn root_right_boundary_grammar() -> Grammar {
    let fixture = require_fixture("edge-cases", "truncate-morphotactic-identity");
    let xml = fixture.load_grammar_xml();
    let root_shape = "<PhoneticShape>sag</PhoneticShape>";
    assert_eq!(
        xml.matches(root_shape).count(),
        1,
        "the right-boundary discriminator must alter exactly the explicit-ID ROOT allomorph"
    );
    let root_with_required_right_boundary = format!(
        "{root_shape}\n                <RequiredEnvironments><Environment><RightEnvironment><PhoneticTemplate finalBoundaryCondition=\"true\"><PhoneticSequence /></PhoneticTemplate></RightEnvironment></Environment></RequiredEnvironments>"
    );
    let boundary_xml = xml.replacen(root_shape, &root_with_required_right_boundary, 1);
    pg_grammar::load(&boundary_xml)
        .unwrap_or_else(|error| panic!("right-boundary grammar failed to load: {error}"))
}

#[test]
fn pinned_empty_id_grammar_has_two_a_and_five_ga_derivations() {
    let fixture = require_fixture("edge-cases", "truncate-morphotactic");
    let grammar = fixture.load_grammar();
    let morpher = Morpher::new(&grammar, usize::MAX);

    let sag = morpher.parse_word("sag");
    assert!(!sag.invalid_shape && !sag.capped);
    assert_eq!(result_multiset(&sag.analyses), ["|sag"]);
    let sas = morpher.parse_word("sas");
    assert!(!sas.invalid_shape && !sas.capped);
    assert_eq!(result_multiset(&sas.analyses), ["|sas"]);

    let a = morpher.parse_word("a");
    assert!(!a.invalid_shape && !a.capped);
    assert_eq!(result_multiset(&a.analyses), ["++|a", "++|a"]);
    let a_owners = owner_occurrence_rows(&grammar, "a");

    let ga = morpher.parse_word("ga");
    assert!(!ga.invalid_shape && !ga.capped);
    assert_eq!(
        result_multiset(&ga.analyses),
        ["+++|ga", "+++|ga", "+++|ga", "++|ga", "++|ga"]
    );
    let ga_owners = owner_occurrence_rows(&grammar, "ga");
    assert_eq!(
        (a_owners, ga_owners),
        (
            vec![
                vec![(3, "root"), (0, "affix"), (1, "affix")],
                vec![(3, "root"), (1, "affix"), (0, "affix")],
            ],
            vec![
                vec![(2, "affix"), (1, "affix"), (3, "root"), (0, "affix")],
                vec![(2, "affix"), (3, "root"), (0, "affix")],
                vec![(3, "root"), (0, "affix"), (2, "affix"), (1, "affix")],
                vec![(3, "root"), (1, "affix"), (2, "affix"), (0, "affix")],
                vec![(3, "root"), (2, "affix"), (0, "affix")],
            ],
        ),
        "pinned #480 grammar: complete a/ga allomorph-owner occurrence rows must match"
    );

    let impossible = morpher.parse_word("bag");
    assert!(!impossible.capped);
    assert!(
        impossible.analyses.is_empty(),
        "bag is not derivable from sag, sas, or bubibi"
    );
}

#[test]
fn explicit_id_companion_matches_full_a_morph_and_application_rows() {
    let fixture = require_fixture("edge-cases", "truncate-morphotactic-identity");
    let grammar = fixture.load_grammar();

    let a = [
        ExpectedRow {
            surface: "a",
            morph_ids: vec!["ROOT", "A", "B"],
            applications: vec!["A", "B"],
        },
        ExpectedRow {
            surface: "a",
            morph_ids: vec!["ROOT", "B", "A"],
            applications: vec!["B", "A"],
        },
    ];
    assert_identity_rows(&grammar, "a", &a);
}

#[test]
fn explicit_id_companion_matches_full_ga_morph_and_application_rows() {
    let fixture = require_fixture("edge-cases", "truncate-morphotactic-identity");
    let grammar = fixture.load_grammar();
    let ga = [
        ExpectedRow {
            surface: "ga",
            morph_ids: vec!["ROOT", "A", "C", "B"],
            applications: vec!["A", "B", "C"],
        },
        ExpectedRow {
            surface: "ga",
            morph_ids: vec!["ROOT", "B", "C", "A"],
            applications: vec!["B", "A", "C"],
        },
        ExpectedRow {
            surface: "ga",
            morph_ids: vec!["C", "B", "ROOT", "A"],
            applications: vec!["B", "C", "A"],
        },
        ExpectedRow {
            surface: "ga",
            morph_ids: vec!["ROOT", "C", "A"],
            applications: vec!["A", "C"],
        },
        ExpectedRow {
            surface: "ga",
            morph_ids: vec!["C", "ROOT", "A"],
            applications: vec!["C", "A"],
        },
    ];
    assert_identity_rows(&grammar, "ga", &ga);
}

#[test]
fn root_right_boundary_preserves_only_combined_csharp_truncation_rows() {
    let grammar = root_right_boundary_grammar();
    let sa = [ExpectedRow {
        surface: "sa",
        morph_ids: vec!["ROOT", "A"],
        applications: vec!["A"],
    }];
    let sag = [ExpectedRow {
        surface: "sag",
        morph_ids: vec!["ROOT"],
        applications: vec![],
    }];
    let sas = [ExpectedRow {
        surface: "sas",
        morph_ids: vec!["ROOT33"],
        applications: vec![],
    }];
    let a = [
        ExpectedRow {
            surface: "a",
            morph_ids: vec!["ROOT", "A", "B"],
            applications: vec!["A", "B"],
        },
        ExpectedRow {
            surface: "a",
            morph_ids: vec!["ROOT", "B", "A"],
            applications: vec!["B", "A"],
        },
    ];
    let ga = [
        ExpectedRow {
            surface: "ga",
            morph_ids: vec!["C", "B", "ROOT", "A"],
            applications: vec!["B", "C", "A"],
        },
        ExpectedRow {
            surface: "ga",
            morph_ids: vec!["C", "ROOT", "A"],
            applications: vec!["C", "A"],
        },
    ];

    assert_identity_rows(&grammar, "sa", &sa);
    assert_identity_rows(&grammar, "sag", &sag);
    assert_identity_rows(&grammar, "sas", &sas);
    assert_identity_rows(&grammar, "a", &a);
    assert_identity_rows(&grammar, "ga", &ga);
    assert_identity_rows(&grammar, "bag", &[]);
}

#[test]
fn explicit_id_copy_u_matches_all_gau_and_au_rows_and_controls() {
    let fixture = require_fixture("edge-cases", "truncate-morphotactic-copy-u");
    let grammar = fixture.load_grammar();
    let gau = [
        ExpectedRow {
            surface: "gau",
            morph_ids: vec!["ROOT", "A", "C", "B", "U"],
            applications: vec!["A", "B", "C", "U"],
        },
        ExpectedRow {
            surface: "gau",
            morph_ids: vec!["ROOT", "B", "C", "A", "U"],
            applications: vec!["B", "A", "C", "U"],
        },
        ExpectedRow {
            surface: "gau",
            morph_ids: vec!["B", "A", "C", "ROOT", "U"],
            applications: vec!["B", "C", "A", "U"],
        },
        ExpectedRow {
            surface: "gau",
            morph_ids: vec!["C", "B", "ROOT", "A", "U"],
            applications: vec!["A", "B", "U", "C"],
        },
        ExpectedRow {
            surface: "gau",
            morph_ids: vec!["C", "A", "ROOT", "B", "U"],
            applications: vec!["B", "A", "U", "C"],
        },
        ExpectedRow {
            surface: "gau",
            morph_ids: vec!["ROOT", "U", "C", "A", "B"],
            applications: vec!["A", "U", "B", "C"],
        },
        ExpectedRow {
            surface: "gau",
            morph_ids: vec!["ROOT", "C", "A", "U"],
            applications: vec!["A", "C", "U"],
        },
        ExpectedRow {
            surface: "gau",
            morph_ids: vec!["C", "A", "ROOT", "U"],
            applications: vec!["C", "A", "U"],
        },
        ExpectedRow {
            surface: "gau",
            morph_ids: vec!["ROOT", "C", "A", "U"],
            applications: vec!["A", "U", "C"],
        },
    ];
    assert_identity_rows(&grammar, "gau", &gau);

    let au = [
        ExpectedRow {
            surface: "au",
            morph_ids: vec!["B", "ROOT", "A", "U"],
            applications: vec!["A", "B", "U"],
        },
        ExpectedRow {
            surface: "au",
            morph_ids: vec!["A", "ROOT", "B", "U"],
            applications: vec!["B", "A", "U"],
        },
        ExpectedRow {
            surface: "au",
            morph_ids: vec!["ROOT", "A", "U", "B"],
            applications: vec!["A", "U", "B"],
        },
    ];
    assert_identity_rows(&grammar, "au", &au);

    let sagu = [ExpectedRow {
        surface: "sagu",
        morph_ids: vec!["ROOT", "U"],
        applications: vec!["U"],
    }];
    assert_identity_rows(&grammar, "sagu", &sagu);
    let bau = Morpher::new(&grammar, usize::MAX).parse_word("bau");
    assert!(!bau.capped && !bau.invalid_shape);
    assert!(bau.analyses.is_empty(), "bau is an impossible control");
}

#[test]
fn explicit_id_companion_has_positive_and_impossible_controls() {
    let fixture = require_fixture("edge-cases", "truncate-morphotactic-identity");
    let grammar = fixture.load_grammar();
    let morpher = Morpher::new(&grammar, usize::MAX);
    let sag = morpher.parse_word("sag");
    assert_eq!(result_multiset(&sag.analyses), ["ROOT|sag"]);
    let sas = morpher.parse_word("sas");
    assert_eq!(result_multiset(&sas.analyses), ["ROOT33|sas"]);
    let impossible = morpher.parse_word("bag");
    assert!(
        impossible.analyses.is_empty(),
        "bag is an impossible control"
    );
}

#[test]
fn explicit_id_subsumed_first_matches_full_ag_identity() {
    let fixture = require_fixture("edge-cases", "truncate-morphotactic-subsumption");
    let grammar = fixture.load_grammar();
    let ag = [
        ExpectedRow {
            surface: "ag",
            morph_ids: vec!["ROOTAG"],
            applications: vec![],
        },
        ExpectedRow {
            surface: "ag",
            morph_ids: vec!["ROOTAG", "X", "B"],
            applications: vec!["X", "B"],
        },
    ];
    assert_identity_rows(&grammar, "ag", &ag);
}

#[test]
fn explicit_id_subsumed_child_matches_full_gag_identity() {
    let fixture = require_fixture("edge-cases", "truncate-morphotactic-subsumption");
    let grammar = fixture.load_grammar();
    let gag = [
        ExpectedRow {
            surface: "gag",
            morph_ids: vec!["X", "C", "ROOTAG"],
            applications: vec!["X", "C"],
        },
        ExpectedRow {
            surface: "gag",
            morph_ids: vec!["X", "C", "ROOTAG", "B"],
            applications: vec!["X", "B", "C"],
        },
        ExpectedRow {
            surface: "gag",
            morph_ids: vec!["X", "C", "ROOTAG", "B"],
            applications: vec!["C", "X", "B"],
        },
        ExpectedRow {
            surface: "gag",
            morph_ids: vec!["C", "ROOTAG"],
            applications: vec!["C"],
        },
    ];
    assert_identity_rows(&grammar, "gag", &gag);
}

#[test]
fn explicit_id_subsumption_companion_has_positive_and_impossible_controls() {
    let fixture = require_fixture("edge-cases", "truncate-morphotactic-subsumption");
    let grammar = fixture.load_grammar();
    let sag = [ExpectedRow {
        surface: "sag",
        morph_ids: vec!["X", "ROOTAG"],
        applications: vec!["X"],
    }];
    assert_identity_rows(&grammar, "sag", &sag);

    let impossible = Morpher::new(&grammar, usize::MAX).parse_word("bag");
    assert!(
        impossible.analyses.is_empty(),
        "bag is an impossible control"
    );
}

fn successful_word_for_applications(
    grammar: &Grammar,
    surface: &str,
    applications: &[&str],
) -> Word {
    let morpher = Morpher::new(grammar, usize::MAX);
    let sink = TreeTraceSink::new();
    let _ = morpher.parse_word_traced(surface, &ParseOptions::default(), &sink);
    let root = sink.root().expect("traced parse must have a root");
    let mut words = Vec::new();
    collect_successful_words(&sink, root, &mut words);
    let wanted: Vec<_> = applications
        .iter()
        .map(|name| Some((*name).to_string()))
        .collect();
    let matches: Vec<_> = words
        .into_iter()
        .filter(|word| trace_identity(grammar, word).1 == wanted)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "{surface}: expected one traced word for {applications:?}"
    );
    matches.into_iter().next().expect("one matching trace word")
}

fn assert_morph_anchor(grammar: &Grammar, word: &Word, id: &str, status: MorphStatus, order: u32) {
    let records: Vec<_> = word
        .morphs
        .iter()
        .filter(|record| morph_name(grammar, record.morpheme.0) == id)
        .collect();
    assert_eq!(
        records.len(),
        1,
        "{id}: expected one record, got {records:?}"
    );
    assert_eq!(
        records[0].status, status,
        "{id}: unexpected annotation state"
    );
    assert_eq!(records[0].order, order, "{id}: unexpected node anchor");
}

#[test]
fn trace_paths_cover_floating_and_both_subsumption_states() {
    let truncation = require_fixture("edge-cases", "truncate-morphotactic-identity");
    let truncation_grammar = truncation.load_grammar();
    let floating = successful_word_for_applications(&truncation_grammar, "a", &["A", "B"]);
    assert_morph_anchor(&truncation_grammar, &floating, "A", MorphStatus::Real, 0);
    assert_morph_anchor(
        &truncation_grammar,
        &floating,
        "B",
        MorphStatus::SubsumedFirst,
        0,
    );
    let reverse = successful_word_for_applications(&truncation_grammar, "a", &["B", "A"]);
    assert_morph_anchor(&truncation_grammar, &reverse, "B", MorphStatus::Floating, 0);
    assert_morph_anchor(
        &truncation_grammar,
        &reverse,
        "A",
        MorphStatus::SubsumedFirst,
        0,
    );
    let resolved = successful_word_for_applications(&truncation_grammar, "ga", &["A", "B", "C"]);
    assert_morph_anchor(
        &truncation_grammar,
        &resolved,
        "A",
        MorphStatus::SubsumedChild,
        0,
    );
    assert_morph_anchor(&truncation_grammar, &resolved, "B", MorphStatus::Real, 1);
    assert_morph_anchor(
        &truncation_grammar,
        &resolved,
        "ROOT",
        MorphStatus::SubsumedChild,
        0,
    );
    assert_morph_anchor(&truncation_grammar, &resolved, "C", MorphStatus::Real, 0);

    let subsumption = require_fixture("edge-cases", "truncate-morphotactic-subsumption");
    let subsumption_grammar = subsumption.load_grammar();
    let first = successful_word_for_applications(&subsumption_grammar, "ag", &["X", "B"]);
    assert_morph_anchor(&subsumption_grammar, &first, "X", MorphStatus::Floating, 0);
    let child = successful_word_for_applications(&subsumption_grammar, "gag", &["X", "C"]);
    assert_morph_anchor(
        &subsumption_grammar,
        &child,
        "X",
        MorphStatus::SubsumedChild,
        0,
    );
}
