//! Metathesis that can swap segments of two morphs is refused with a typed cause, and one that cannot stays admitted.

use std::collections::{BTreeMap, BTreeSet};

use pg_conformance_fixtures::discover;
use pg_foma::analyzer::{compile_proposer, FomaError};
use pg_health::capability::{CapabilityRefusal, ProposalRecallGap};

fn refused_as_metathesis_across_morphs(label: &str) -> bool {
    let fixture = discover()
        .into_iter()
        .find(|f| f.label() == label)
        .unwrap_or_else(|| panic!("missing {label}"));
    refuses_by_cause(&fixture.load_grammar())
}

fn refuses_by_cause(grammar: &pg_grammar::model::Grammar) -> bool {
    match compile_proposer(grammar) {
        Err(FomaError::CapabilityRefused(refusals)) => refusals.iter().any(|d| {
            d.refusal
                == CapabilityRefusal::ProposalRecall(ProposalRecallGap::MetathesisAcrossMorphs)
        }),
        _ => false,
    }
}

#[test]
fn metathesis_that_can_straddle_a_morph_boundary_is_refused_by_cause() {
    for name in [
        "metathesis-boundary-morph-order-prefix-root",
        "metathesis-boundary-morph-order-compound",
        "metathesis-boundary-between-switches-prefix",
        "metathesis-boundary-between-switches-compound",
        "metathesis-boundary-between-switches-marker",
        "metathesis-boundary-right-to-left-prefix",
        "metathesis-boundary-right-to-left-suffix",
        "metathesis-boundary-context-right",
        "metathesis-boundary-context-left-right-to-left",
        // A right context across the root's boundary with segment-class nodes: the skipped boundary joins t's group, so s ends up after it.
        "metathesis-boundary-context-segments",
        // The s of one suffix and the t of the next swap across their boundary without changing the morph order.
        "metathesis-boundary-two-suffixes",
        // A segment class matches the union of its members' features, so root a is a switch too.
        "metathesis-segment-class-union",
    ] {
        assert!(
            refused_as_metathesis_across_morphs(&format!("machine:edge-cases/{name}")),
            "{name}"
        );
    }
    for name in [
        "metathesis-boundary-between-switches-root",
        // A rule whose `leftSwitch` names the earlier node, which FieldWorks never writes.
        "metathesis-reversed-switch-order",
    ] {
        assert!(
            refused_as_metathesis_across_morphs(&format!("staging:edge-cases/{name}")),
            "{name}"
        );
    }
}

#[test]
fn metathesis_inside_one_morph_stays_admitted() {
    // The last two have no affix, so every word is one root; one also merges two segments, the other has two character tables.
    for key in [
        "machine:edge-cases/fw-metathesis-switch-indices",
        "machine:edge-cases/merge-then-metathesis",
        "staging:edge-cases/multi-table-metathesis-shared-representation",
    ] {
        assert!(!refused_as_metathesis_across_morphs(key), "{key}");
    }
}

/// An earlier pass turns root `st` into `ts`, so the later pass swaps prefix `s` with root `t` and C# gives `tss` as `Z+PRE`.
#[test]
fn an_earlier_metathesis_pass_that_changes_a_morph_edge_is_refused() {
    for (name, xml) in [
        (
            "one rule in two strata",
            include_str!("fixtures/metathesis/one-rule-two-strata.xml"),
        ),
        (
            "two rules in two strata",
            include_str!("fixtures/metathesis/two-rules-two-strata.xml"),
        ),
    ] {
        let grammar = pg_grammar::load(xml).expect("the probe grammar loads");
        assert!(refuses_by_cause(&grammar), "{name}");
    }
}

/// Its swap `mi+u` to `mu+i` puts segments of two morphs in one match, which the conservative test refuses even though this fixture keeps the morph order.
#[test]
fn a_cross_morph_swap_that_keeps_the_morph_order_is_refused() {
    assert!(refused_as_metathesis_across_morphs(
        "staging:edge-cases/pos-scoped-deletion-blocks-metathesis"
    ));
}

/// A null prefix `^0+` has no segment; C# moves only segments, so the root segment moves in front of its boundaries and C# lists it after the root (`PRE+R0+NUL`); the surface skeleton cannot tag that.
#[test]
fn a_morph_with_no_segment_next_to_a_cross_morph_swap_is_refused() {
    assert!(refused_as_metathesis_across_morphs(
        "machine:edge-cases/metathesis-boundary-null-prefix"
    ));
}

/// A deletion can expose a segment at the edge of a morph that holds no switch segment, and a morph left with no segment can change sides of a swap. Prefix `pa` loses its `a`, so its `p` becomes the context of a swap inside root `st`. Prefix `apa` loses both `a`s, so the swap of `s` and `t` reaches across it from prefix `s` to root `t`; Rust HC analyses `tps` and C# does not (#388), so the FST must refuse or match Rust HC.
#[test]
fn a_morph_edge_a_deletion_or_an_empty_morph_moves_is_exact_or_refused() {
    for (name, xml, words) in [
        (
            "context behind a deleted segment",
            include_str!("fixtures/metathesis/context-behind-deleted-segment.xml"),
            "pts pst st ts ast ats pats past pt ps ppts",
        ),
        (
            "switches around a deleted edge",
            include_str!("fixtures/metathesis/switches-around-a-deleted-edge.xml"),
            "t st at tps spt sapat apat apst tpat ps tp pts stpt tpst",
        ),
        // A deletion empties prefix `t`, then the swap inside root `ps` makes `sp`; C# keeps the prefix first (`PR0+R0`).
        (
            "emptied prefix beside a swap",
            include_str!("fixtures/metathesis/emptied-prefix-beside-a-swap.xml"),
            include_str!("fixtures/metathesis/emptied-prefix-beside-a-swap.words"),
        ),
        // Root `a` can be deleted, so prefix `ps` and suffix `tp` meet and swap `s` and `t`.
        (
            "root that can vanish",
            include_str!("fixtures/metathesis/root-that-can-vanish.xml"),
            include_str!("fixtures/metathesis/root-that-can-vanish.words"),
        ),
        // A copy-only affix in a template slot beside a swap at the root's first segment.
        (
            "zero affix slot beside a swap",
            include_str!("fixtures/metathesis/zero-affix-slot-beside-a-swap.xml"),
            include_str!("fixtures/metathesis/zero-affix-slot-beside-a-swap.words"),
        ),
    ] {
        let words: Vec<String> = words.split_whitespace().map(String::from).collect();
        if let Some((_, _, differences)) = fst_differences(xml, &words, &mut BTreeMap::new()) {
            assert!(differences.is_empty(), "{name}: {differences:?}");
        }
    }
}

/// Root `a` can be deleted, so prefix `ps` and suffix `tp` can meet and a swap can take a segment from each.
#[test]
fn a_prefix_and_suffix_that_meet_over_a_vanishing_root_are_refused() {
    let grammar = pg_grammar::load(include_str!("fixtures/metathesis/root-that-can-vanish.xml"))
        .expect("the probe grammar loads");
    assert!(refuses_by_cause(&grammar));
}

fn refuses_as_emptied_morph(grammar: &pg_grammar::model::Grammar) -> bool {
    match compile_proposer(grammar) {
        Err(FomaError::CapabilityRefused(refusals)) => refusals.iter().any(|d| {
            d.refusal == CapabilityRefusal::ProposalRecall(ProposalRecallGap::EmptiedMorphReorder)
        }),
        _ => false,
    }
}

/// Every word over `a p s t` of up to four letters, plus `extra`.
fn short_words(extra: &[&str]) -> Vec<String> {
    let mut words = vec![String::new()];
    let mut out = Vec::new();
    for _ in 0..4 {
        words = words
            .iter()
            .flat_map(|w| ['a', 'p', 's', 't'].map(|c| format!("{w}{c}")))
            .collect();
        out.extend(words.iter().cloned());
    }
    out.extend(extra.iter().map(|w| w.to_string()));
    out
}

/// A deletion empties prefix `PR0` (or the prefix half of circumfix `CI0`), directly or after a rule changes its segments into deleted ones, and a later stratum adds suffix `SU1`; Rust HC, like C#, then gives `R0+PR0+SU1`, which the surface skeleton does not, so the FST must refuse each grammar or match Rust HC. Metathesis plays no part: in the `n-` grammars the swap rule is defined but not listed.
#[test]
fn a_morph_a_deletion_empties_before_a_later_affix_is_refused_or_exact() {
    let mut failures = Vec::new();
    for (name, xml, witnesses) in [
        (
            "deleted prefix, later suffix",
            include_str!("fixtures/emptied-morph/p1-deleted-prefix-later-suffix.xml"),
            &["psa", "pspa"][..],
        ),
        (
            "prefix deleted in hiatus, later suffix",
            include_str!("fixtures/emptied-morph/p2-hiatus-deleted-prefix-later-suffix.xml"),
            &["asp"][..],
        ),
        (
            "emptied prefix, suffix in a later stratum",
            include_str!("fixtures/emptied-morph/n-k3a-emptied-prefix-suffix-later-stratum.xml"),
            &["psa", "pspa"][..],
        ),
        (
            "emptied prefix, suffix with t in a later stratum",
            include_str!(
                "fixtures/emptied-morph/n-k3c-emptied-prefix-suffix-with-t-later-stratum.xml"
            ),
            &["psta", "pspta"][..],
        ),
        (
            "emptied prefix, deletion in both strata",
            include_str!("fixtures/emptied-morph/n-k3h-emptied-prefix-deletion-in-both-strata.xml"),
            &["psa", "pspa"][..],
        ),
        (
            "circumfix prefix half emptied",
            include_str!(
                "fixtures/emptied-morph/n-k4h-circumfix-prefix-half-emptied-two-strata.xml"
            ),
            &["psaa", "pspaa"][..],
        ),
        (
            "circumfix half emptied, swap rule unlisted",
            include_str!(
                "fixtures/emptied-morph/n-k5e-circumfix-half-emptied-swap-not-at-edge.xml"
            ),
            &["apsaa", "apspaa"][..],
        ),
        (
            "circumfix half emptied, right-to-left swap rule unlisted",
            include_str!("fixtures/emptied-morph/n-k5g-circumfix-half-emptied-rtl.xml"),
            &["psaa", "pspaa", "spaa", "sppaa"][..],
        ),
        (
            "circumfix prefix half emptied, swap in the later stratum",
            include_str!("fixtures/emptied-morph/k4h-circumfix-prefix-half-emptied-two-strata.xml"),
            &["spaa", "sppaa"][..],
        ),
        (
            "circumfix half emptied, swap away from the edge",
            include_str!("fixtures/emptied-morph/k5e-circumfix-half-emptied-swap-not-at-edge.xml"),
            &["aspaa", "asppaa", "apsaa", "apspaa"][..],
        ),
        (
            "circumfix half emptied, right-to-left swap",
            include_str!("fixtures/emptied-morph/k5g-circumfix-half-emptied-rtl.xml"),
            &["spaa", "sppaa", "psaa", "pspaa"][..],
        ),
        (
            "prefix p changed to t, then t deleted",
            include_str!("fixtures/emptied-morph/v1-change-then-delete.xml"),
            &["paa", "papa"][..],
        ),
        (
            "prefix p changed to t, then t deleted, mid-stratum",
            include_str!("fixtures/emptied-morph/v2-change-then-delete-mid-stratum.xml"),
            &["paa", "papa"][..],
        ),
        (
            "a change feeds a deletion of the prefix",
            include_str!("fixtures/emptied-morph/a1-change-feeds-deletion-prefix.xml"),
            &[][..],
        ),
        (
            "a change in context feeds a deletion of the prefix",
            include_str!("fixtures/emptied-morph/a2-context-change-feeds-deletion-prefix.xml"),
            &[][..],
        ),
        (
            "a change feeds a deletion in the next stratum",
            include_str!("fixtures/emptied-morph/a3-change-then-deletion-next-stratum.xml"),
            &[][..],
        ),
        (
            "a change feeds a deletion of a circumfix half",
            include_str!("fixtures/emptied-morph/a5-change-feeds-deletion-circumfix-half.xml"),
            &[][..],
        ),
        (
            "a class change feeds a class deletion",
            include_str!("fixtures/emptied-morph/a9-class-change-feeds-class-deletion.xml"),
            &["asa", "saa"][..],
        ),
    ] {
        let words = short_words(witnesses);
        if let Some((_, _, differences)) = fst_differences(xml, &words, &mut BTreeMap::new()) {
            if !differences.is_empty() {
                failures.push(format!("{name}: {differences:?}"));
            }
        }
        let grammar = pg_grammar::load(xml).expect("the probe grammar loads");
        if !refuses_as_emptied_morph(&grammar) {
            failures.push(format!("{name}: not refused as an emptied morph"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The deletion runs before the change, so the changed prefix is never deleted, but any segment deletion beside affixes is refused until the refinement in #430.
#[test]
fn a_change_after_the_deletion_is_refused_as_a_junction_deletion() {
    let xml = include_str!("fixtures/emptied-morph/a7-counterfeeding-control.xml");
    let mut refused = BTreeMap::new();
    assert!(fst_differences(xml, &short_words(&[]), &mut refused).is_none());
    assert!(refused.contains_key(&format!(
        "{:?}",
        CapabilityRefusal::ProposalRecall(ProposalRecallGap::JunctionDeletion)
    )));
}

/// For each differing word, the analyses Rust HC gives that the FST does not, and those the FST gives that HC does not; a word whose HC run hit the work cap is counted, not compared. `None` when the FST will not compile the grammar.
fn fst_differences(
    xml: &str,
    words: &[String],
    refused: &mut BTreeMap<String, usize>,
) -> Option<(usize, usize, Differences)> {
    let g = pg_grammar::load(xml).unwrap_or_else(|e| panic!("grammar loads: {e:?}"));
    let proposer = match pg_foma::analyzer::compile_proposer(&g) {
        Ok(proposer) => proposer,
        Err(FomaError::CapabilityRefused(refusals)) => {
            for d in refusals {
                *refused.entry(format!("{:?}", d.refusal)).or_insert(0) += 1;
            }
            return None;
        }
        Err(e) => {
            *refused.entry(format!("error: {e}")).or_insert(0) += 1;
            return None;
        }
    };
    let morpher = pg_parse::Morpher::new(&g, fuzz::WORK_CAP);
    let mut analyzer = pg_foma::composite::FomaAnalyzer::from_cached_with_morpher(
        &g,
        proposer,
        pg_foma::peel::ReduplicationPeeler::new(&g),
        pg_foma::confirm::build_morpheme_owners(&g),
        pg_parse::Morpher::new(&g, fuzz::WORK_CAP),
        pg_foma::candidate_filter::shadow::CandidateFilterSettings::off(),
    );
    let budget = pg_foma::compose_budget::ApplyBudget::unbounded();
    let (mut compared, mut incomplete) = (0, 0);
    let mut differences = Differences::new();
    for (index, word) in words.iter().enumerate() {
        let row = pg_foma::differential::measure_word(
            &g,
            &morpher,
            Some(&mut analyzer),
            index,
            word,
            &budget,
            None,
        )
        .unwrap_or_else(|e| panic!("{word}: {e}"));
        if row.incomplete {
            incomplete += 1;
            continue;
        }
        compared += 1;
        let hc: Identities = row.oracle.iter().map(|a| a.identity.clone()).collect();
        let fst: Identities = row.confirmed.iter().map(|a| a.identity.clone()).collect();
        if hc != fst {
            differences.insert(
                word.clone(),
                (
                    hc.difference(&fst).cloned().collect(),
                    fst.difference(&hc).cloned().collect(),
                ),
            );
        }
    }
    Some((compared, incomplete, differences))
}

type Identities = BTreeSet<pg_parse::AnalysisIdentity>;
type Differences = BTreeMap<String, (Identities, Identities)>;

/// Seeded random metathesis grammars: every one the FST admits must give Rust HC's analyses on every word, including words long enough to hold a root and two affixes. A word whose difference recurs exactly in a control grammar that runs no metathesis rule (the rules unlisted, or each replaced by an epenthesis that never fires) is an older FST gap that metathesis does not cause, such as #385, #386 and #387; such a word is excused only when `fuzz::EXCUSED` names it, so a new excusal fails.
#[test]
fn every_admitted_random_metathesis_grammar_matches_rust_hc() {
    let mut admitted = 0;
    let (mut compared, mut incomplete) = (0, 0);
    let mut misses = Vec::new();
    let mut older = Vec::new();
    let mut refused = BTreeMap::new();
    let mut first_xml = String::new();
    for k in 0..fuzz::GRAMMARS {
        let (xml, words) = fuzz::grammar(fuzz::SEED, k);
        let Some((c, i, differences)) = fst_differences(&xml, &words, &mut refused) else {
            continue;
        };
        admitted += 1;
        compared += c;
        incomplete += i;
        if differences.is_empty() {
            continue;
        }
        let controls: Vec<Differences> =
            [fuzz::Variant::NoMetathesis, fuzz::Variant::InertEpenthesis]
                .into_iter()
                .filter_map(|variant| {
                    let (control, _) = fuzz::grammar_variant(fuzz::SEED, k, variant);
                    fst_differences(&control, &words, &mut BTreeMap::new()).map(|(_, _, d)| d)
                })
                .collect();
        for (word, (missing, extra)) in differences {
            let line = format!("grammar {k}, word {word}: missing {missing:?}, extra {extra:?}");
            if controls
                .iter()
                .any(|control| control.get(&word) == Some(&(missing.clone(), extra.clone())))
            {
                older.push(line);
            } else {
                if misses.is_empty() {
                    first_xml = xml.clone();
                }
                misses.push(line);
            }
        }
    }
    eprintln!(
        "{admitted} admitted of {}, {compared} words compared, {incomplete} skipped because HC did not finish, {} differences shared with a control grammar (older FST gaps); refusals {refused:?}",
        fuzz::GRAMMARS,
        older.len()
    );
    for line in older.iter().take(10) {
        eprintln!("older gap: {line}");
    }
    assert_eq!(
        older,
        fuzz::EXCUSED,
        "the words excused as older FST gaps changed; name each in fuzz::EXCUSED or fix it"
    );
    assert!(
        admitted >= fuzz::MIN_ADMITTED && compared >= 20 * fuzz::MIN_ADMITTED,
        "the fuzz must exercise admitted grammars: {admitted} admitted, {compared} words compared, {incomplete} skipped because HC did not finish; refusals {refused:?}"
    );
    assert!(
        incomplete * 20 <= compared,
        "too many words skipped because HC did not finish: {incomplete} of {}",
        compared + incomplete
    );
    assert!(
        misses.is_empty(),
        "{} admitted words differ from Rust HC (seed {}, {admitted} admitted, {incomplete} words skipped because HC did not finish):\n{}\nfirst failing grammar:\n{first_xml}",
        misses.len(),
        fuzz::SEED,
        misses
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The same seeded grammars with their metathesis rules unlisted, so deletions, epenthesis, two strata and every affix shape remain: every one the FST admits must give Rust HC's analyses, except the words `older-gaps-without-metathesis.txt` lists, whose FST rows are identical on v0.27.0 (older circumfix and boundary gaps). This is the sweep for a morph a deletion empties before a later stratum's affix, which needs no metathesis.
#[test]
fn every_admitted_random_grammar_without_metathesis_matches_rust_hc() {
    sweep(
        fuzz::Variant::NoMetathesis,
        fuzz::MIN_ADMITTED_WITHOUT_METATHESIS,
        include_str!("fixtures/metathesis/older-gaps-without-metathesis.txt"),
    );
}

/// The same seeded grammars without metathesis, each with a rule changing a segment that a later rule deletes, written into affixes and roots: every one the FST admits must give Rust HC's analyses, except the words `older-gaps-change-feeds-deletion.txt` lists, whose FST rows are identical on v0.27.0.
#[test]
fn every_admitted_random_grammar_where_a_change_feeds_a_deletion_matches_rust_hc() {
    sweep(
        fuzz::Variant::ChangeFeedsDeletion,
        fuzz::MIN_ADMITTED_CHANGE_FEEDS_DELETION,
        include_str!("fixtures/metathesis/older-gaps-change-feeds-deletion.txt"),
    );
}

/// Every seeded grammar written as `variant` that the FST admits gives Rust HC's analyses, except the lines of `excused`.
fn sweep(variant: fuzz::Variant, min_admitted: usize, excused: &str) {
    let (mut admitted, mut compared, mut incomplete) = (0, 0, 0);
    let mut misses = Vec::new();
    let mut refused = BTreeMap::new();
    for k in 0..fuzz::GRAMMARS {
        let (xml, words) = fuzz::grammar_variant(fuzz::SEED, k, variant);
        let Some((c, i, differences)) = fst_differences(&xml, &words, &mut refused) else {
            continue;
        };
        admitted += 1;
        compared += c;
        incomplete += i;
        for (word, (missing, extra)) in differences {
            misses.push(format!(
                "grammar {k}, word {word}: missing {missing:?}, extra {extra:?}"
            ));
        }
    }
    eprintln!(
        "{admitted} admitted of {}, {compared} words compared, {incomplete} skipped because HC did not finish, {} differ; refusals {refused:?}",
        fuzz::GRAMMARS,
        misses.len()
    );
    for line in misses.iter().take(40) {
        eprintln!("differs: {line}");
    }
    assert!(
        admitted >= min_admitted && compared >= 20 * min_admitted,
        "the sweep must exercise admitted grammars: {admitted} admitted, {compared} words compared"
    );
    assert!(
        incomplete * 20 <= compared,
        "too many words skipped because HC did not finish: {incomplete} of {}",
        compared + incomplete
    );
    let excused: Vec<&str> = excused.lines().collect();
    assert_eq!(
        misses, excused,
        "the admitted grammars that differ from Rust HC changed; a new line is a new FST gap"
    );
}

/// A port of the round-4 review fuzz (`gen5.py`): segments `a p t s`, boundary `+`, literal, segment-class and feature-class switches, contexts, deletion, prefixes, suffixes, circumfixes, templates and two strata.
mod fuzz {
    use std::collections::BTreeSet;
    use std::fmt::Write;

    pub const SEED: u64 = 0x374_0005;
    pub const GRAMMARS: usize = 1500;
    pub const MIN_ADMITTED: usize = 120;
    pub const MIN_ADMITTED_WITHOUT_METATHESIS: usize = 800;
    pub const MIN_ADMITTED_CHANGE_FEEDS_DELETION: usize = 100;
    /// Words excused as older FST gaps at `SEED`, each line as the gate prints it.
    pub const EXCUSED: &[&str] = &[];
    pub const WORK_CAP: usize = 200_000;
    const WORDS: usize = 400;
    const SEGS: [char; 4] = ['a', 'p', 't', 's'];
    /// Class id and the segments it lists or names; a segment class matches the union of its members' features.
    const CLASSES: [(&str, &str); 10] = [
        ("scS", "s"),
        ("scT", "t"),
        ("scA", "a"),
        ("scSA", "sa"),
        ("scTA", "ta"),
        ("scST", "st"),
        ("T", "t"),
        ("P", "p"),
        ("Sx", "s"),
        ("A", "a"),
    ];

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
        fn chance(&mut self, p: f64) -> bool {
            (self.next() >> 11) as f64 / (1u64 << 53) as f64 <= p
        }
        fn seg(&mut self) -> char {
            SEGS[self.below(4)]
        }
    }

    #[derive(Clone)]
    enum Node {
        Seg(char),
        Class(&'static str, &'static str),
        Boundary,
    }

    impl Node {
        fn members(&self) -> Vec<char> {
            match self {
                Node::Seg(c) => vec![*c],
                Node::Class(_, m) => m.chars().collect(),
                Node::Boundary => Vec::new(),
            }
        }
        fn xml(&self, id: &str) -> String {
            match self {
                Node::Seg(c) => format!(
                    "<Segment id=\"{id}\" segment=\"c{}\" />",
                    c.to_ascii_uppercase()
                ),
                Node::Class(c, _) => format!("<SimpleContext id=\"{id}\" naturalClass=\"{c}\" />"),
                Node::Boundary => "<BoundaryMarker boundary=\"b\" />".to_string(),
            }
        }
    }

    fn node(r: &mut Rng) -> Node {
        if r.chance(0.7) {
            Node::Seg(r.seg())
        } else {
            let (c, m) = CLASSES[r.below(CLASSES.len())];
            Node::Class(c, m)
        }
    }

    fn string(r: &mut Rng, lo: usize, hi: usize, end: &[char], start: &[char]) -> String {
        let n = lo + r.below(hi - lo + 1);
        let mut s: Vec<char> = (0..n).map(|_| r.seg()).collect();
        if !end.is_empty() && r.chance(0.6) {
            s[n - 1] = end[r.below(end.len())];
        }
        if !start.is_empty() && r.chance(0.6) {
            s[0] = start[r.below(start.len())];
        }
        s.into_iter().collect()
    }

    fn bounded(r: &mut Rng, s: String, lead: f64, trail: f64, inner: f64) -> String {
        let mut s = s;
        if inner > 0.0 && s.len() > 1 && r.chance(inner) {
            let i = 1 + r.below(s.len() - 1);
            s.insert(i, '+');
        }
        if r.chance(lead) {
            s.insert(0, '+');
        }
        if r.chance(trail) {
            s.push('+');
        }
        s
    }

    enum Affix {
        Prefix(String),
        Suffix(String),
        Circumfix(String, String),
        /// Copies the stem and adds nothing: a zero affix.
        Copy,
        /// Joins two stems, with this literal between them.
        Compound(String),
    }

    fn affix_xml(id: &str, affix: &Affix) -> String {
        let insert = |s: &str| {
            format!("<InsertSegments><PhoneticShape>{s}</PhoneticShape></InsertSegments>")
        };
        let any = "<OptionalSegmentSequence min=\"1\" max=\"-1\"><SimpleContext naturalClass=\"any\" /></OptionalSegmentSequence>";
        if let Affix::Compound(link) = affix {
            let link = if link.is_empty() {
                String::new()
            } else {
                insert(link)
            };
            return format!("<CompoundingRule id=\"{id}\"><Name>{id}</Name><CompoundingSubrules><CompoundingSubrule><HeadMorphologicalInput><PhoneticSequence id=\"{id}h\">{any}</PhoneticSequence></HeadMorphologicalInput><NonHeadMorphologicalInput><PhoneticSequence id=\"{id}n\">{any}</PhoneticSequence></NonHeadMorphologicalInput><MorphologicalOutput><CopyFromInput index=\"{id}h\" />{link}<CopyFromInput index=\"{id}n\" /></MorphologicalOutput></CompoundingSubrule></CompoundingSubrules></CompoundingRule>");
        }
        let copy = format!("<CopyFromInput index=\"{id}S\" />");
        let output = match affix {
            Affix::Prefix(s) => format!("{}{copy}", insert(s)),
            Affix::Suffix(s) => format!("{copy}{}", insert(s)),
            Affix::Circumfix(p, s) => format!("{}{copy}{}", insert(p), insert(s)),
            Affix::Copy => copy,
            Affix::Compound(_) => unreachable!(),
        };
        format!("<MorphologicalRule id=\"{id}\" requiredPartsOfSpeech=\"v\"><Name>{id}</Name><MorphologicalSubrules><MorphologicalSubrule id=\"{id}0\"><MorphologicalInput><PhoneticSequence id=\"{id}S\">{any}</PhoneticSequence></MorphologicalInput><MorphologicalOutput>{output}</MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>{id}</MorphemeId></MorphologicalRule>")
    }

    struct Stratum {
        prules: Vec<String>,
        affixes: Vec<(String, Affix)>,
        slots: Vec<String>,
        roots: Vec<String>,
        linear: bool,
    }

    fn stratum_xml(i: usize, s: &Stratum, dropped: &[String]) -> String {
        let free: Vec<&str> = s
            .affixes
            .iter()
            .map(|(id, _)| id.as_str())
            .filter(|id| !s.slots.iter().any(|slot| slot == id))
            .collect();
        let prules: Vec<&str> = s
            .prules
            .iter()
            .filter(|id| !dropped.contains(id))
            .map(String::as_str)
            .collect();
        let order = if s.linear { "linear" } else { "unordered" };
        let mut attrs =
            format!("characterDefinitionTable=\"t\" morphologicalRuleOrder=\"{order}\"");
        if !free.is_empty() {
            write!(attrs, " morphologicalRules=\"{}\"", free.join(" ")).unwrap();
        }
        if !prules.is_empty() {
            write!(attrs, " phonologicalRules=\"{}\"", prules.join(" ")).unwrap();
        }
        let mut body = format!("<Name>S{i}</Name>");
        if !s.affixes.is_empty() {
            body.push_str("<MorphologicalRuleDefinitions>");
            for (id, affix) in &s.affixes {
                body.push_str(&affix_xml(id, affix));
            }
            body.push_str("</MorphologicalRuleDefinitions>");
        }
        if !s.slots.is_empty() {
            body.push_str(
                "<AffixTemplates><AffixTemplate requiredPartsOfSpeech=\"v\"><Name>T</Name>",
            );
            for (j, id) in s.slots.iter().enumerate() {
                write!(
                    body,
                    "<Slot optional=\"true\" morphologicalRules=\"{id}\"><Name>sl{j}</Name></Slot>"
                )
                .unwrap();
            }
            body.push_str("</AffixTemplate></AffixTemplates>");
        }
        if !s.roots.is_empty() {
            body.push_str("<LexicalEntries>");
            for (j, root) in s.roots.iter().enumerate() {
                write!(body, "<LexicalEntry id=\"r{j}\" partOfSpeech=\"v\"><Allomorphs><Allomorph id=\"r{j}a\"><PhoneticShape>{root}</PhoneticShape></Allomorph></Allomorphs><MorphemeId>R{j}</MorphemeId></LexicalEntry>").unwrap();
            }
            body.push_str("</LexicalEntries>");
        }
        format!("<Stratum {attrs}>{body}</Stratum>")
    }

    const HEAD: &str = r#"<?xml version="1.0" encoding="utf-8"?><HermitCrabInput><Language><Name>Fuzz</Name><PartsOfSpeech><PartOfSpeech id="v"><Name>V</Name></PartOfSpeech></PartsOfSpeech><PhonologicalFeatureSystem><SymbolicFeature id="f"><Name>f</Name><Symbols><Symbol id="fP">+</Symbol><Symbol id="fM">-</Symbol></Symbols></SymbolicFeature><SymbolicFeature id="g"><Name>g</Name><Symbols><Symbol id="gP">+</Symbol><Symbol id="gM">-</Symbol></Symbols></SymbolicFeature></PhonologicalFeatureSystem><CharacterDefinitionTable id="t"><Name>Main</Name><SegmentDefinitions><SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations><FeatureValue feature="f" symbolValues="fP" /><FeatureValue feature="g" symbolValues="gP" /></SegmentDefinition><SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations><FeatureValue feature="f" symbolValues="fM" /><FeatureValue feature="g" symbolValues="gM" /></SegmentDefinition><SegmentDefinition id="cT"><Representations><Representation>t</Representation></Representations><FeatureValue feature="f" symbolValues="fM" /><FeatureValue feature="g" symbolValues="gP" /></SegmentDefinition><SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations><FeatureValue feature="f" symbolValues="fP" /><FeatureValue feature="g" symbolValues="gM" /></SegmentDefinition></SegmentDefinitions><BoundaryDefinitions><BoundaryDefinition id="b"><Representations><Representation>+</Representation></Representations></BoundaryDefinition></BoundaryDefinitions></CharacterDefinitionTable><NaturalClasses><FeatureNaturalClass id="any"><Name>Any</Name></FeatureNaturalClass><FeatureNaturalClass id="T"><Name>T</Name><FeatureValue feature="f" symbolValues="fM" /><FeatureValue feature="g" symbolValues="gP" /></FeatureNaturalClass><FeatureNaturalClass id="P"><Name>P</Name><FeatureValue feature="f" symbolValues="fM" /><FeatureValue feature="g" symbolValues="gM" /></FeatureNaturalClass><FeatureNaturalClass id="Sx"><Name>Sx</Name><FeatureValue feature="f" symbolValues="fP" /><FeatureValue feature="g" symbolValues="gM" /></FeatureNaturalClass><FeatureNaturalClass id="A"><Name>A</Name><FeatureValue feature="f" symbolValues="fP" /><FeatureValue feature="g" symbolValues="gP" /></FeatureNaturalClass>"#;

    fn segment_classes() -> String {
        let mut out = String::new();
        for (id, members) in &CLASSES[..6] {
            write!(out, "<SegmentNaturalClass id=\"{id}\"><Name>{id}</Name>").unwrap();
            for c in members.chars() {
                write!(out, "<Segment segment=\"c{}\" />", c.to_ascii_uppercase()).unwrap();
            }
            out.push_str("</SegmentNaturalClass>");
        }
        out
    }

    /// Which grammar to write from one seeded draw: the grammar itself, or a control that keeps every draw but runs no metathesis rule. A word that misses the same way in a control misses for a reason other than metathesis.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Variant {
        Full,
        /// The metathesis rules are defined but listed in no stratum.
        NoMetathesis,
        /// Each metathesis rule is replaced by an epenthesis whose environment no word holds, so the grammar keeps the FST route a rewrite sends it down.
        InertEpenthesis,
        /// As `NoMetathesis`, plus a rule changing one segment into another and a later rule, in the same or a later stratum, deleting the second, with the first written into affixes and roots, often as a whole prefix.
        ChangeFeedsDeletion,
    }

    /// The draws of `Variant::ChangeFeedsDeletion`, from a stream of their own so every other variant makes the same draws.
    struct Feed {
        rng: Rng,
        changed: char,
        deleted: char,
        two_strata: bool,
    }

    impl Feed {
        fn new(seed: u64, k: usize) -> Self {
            let mut rng = Rng(seed ^ 0xFEED ^ (k as u64).wrapping_mul(0x9FB2_1C65_1E98_DF25));
            let changed = rng.seg();
            let deleted = loop {
                let c = rng.seg();
                if c != changed {
                    break c;
                }
            };
            let two_strata = rng.chance(0.6);
            Feed {
                rng,
                changed,
                deleted,
                two_strata,
            }
        }

        /// Writes the changed segment into affixes and roots: a prefix or circumfix half often becomes only that segment.
        fn seed_literals(&mut self, affixes: &mut [(String, Affix)], roots: &mut [String]) {
            let x = self.changed.to_string();
            for (_, affix) in affixes.iter_mut() {
                let r = &mut self.rng;
                match affix {
                    Affix::Prefix(p) if r.chance(0.5) => *p = x.repeat(1 + r.below(2)),
                    Affix::Prefix(p) => p.push(self.changed),
                    Affix::Suffix(s) if r.chance(0.3) => *s = x.clone(),
                    Affix::Suffix(s) => s.insert(0, self.changed),
                    Affix::Circumfix(p, _) if r.chance(0.5) => *p = x.clone(),
                    Affix::Circumfix(_, s) => *s = x.clone(),
                    Affix::Copy | Affix::Compound(_) => {}
                }
            }
            for root in roots.iter_mut() {
                if self.rng.chance(0.15) {
                    *root = x.clone();
                } else if self.rng.chance(0.3) {
                    root.insert(0, self.changed);
                }
            }
        }

        /// The change and the deletion, each with an optional context.
        fn rules(&mut self) -> [(String, String, bool); 2] {
            let mut rule = |rid: &str, input: char, output: Option<char>| {
                let r = &mut self.rng;
                let seg = |id: String, c: char| Node::Seg(c).xml(&id);
                let output = output.map_or("<PhoneticSequence />".to_string(), |c| {
                    format!(
                        "<PhoneticSequence>{}</PhoneticSequence>",
                        seg(format!("{rid}o"), c)
                    )
                });
                let env = if r.chance(0.25) {
                    let n = node(r);
                    let name = if r.chance(0.5) { "Left" } else { "Right" };
                    format!("<Environment><{name}Environment><PhoneticTemplate><PhoneticSequence>{}</PhoneticSequence></PhoneticTemplate></{name}Environment></Environment>", n.xml(&format!("{rid}e")))
                } else {
                    String::new()
                };
                let xml = format!("<PhonologicalRule id=\"{rid}\"><Name>{rid}</Name><PhoneticInput><PhoneticSequence>{}</PhoneticSequence></PhoneticInput><PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput>{output}</PhoneticOutput>{env}</PhonologicalSubrule></PhonologicalSubrules></PhonologicalRule>", seg(format!("{rid}n"), input));
                (rid.to_string(), xml, false)
            };
            let (x, y) = (self.changed, self.deleted);
            [rule("fc", x, Some(y)), rule("fd", y, None)]
        }

        /// Lists the change before the deletion: in the first and second strata, or both in one stratum.
        fn place(&mut self, strata: &mut [Stratum]) {
            let r = &mut self.rng;
            if strata.len() == 2 && r.chance(0.5) {
                let at = r.below(strata[0].prules.len() + 1);
                strata[0].prules.insert(at, "fc".to_string());
                let at = r.below(strata[1].prules.len() + 1);
                strata[1].prules.insert(at, "fd".to_string());
            } else {
                let s = &mut strata[r.below(strata.len())];
                let at = r.below(s.prules.len() + 1);
                s.prules.insert(at, "fc".to_string());
                let at = at + 1 + r.below(s.prules.len() - at);
                s.prules.insert(at, "fd".to_string());
            }
        }

        /// Each word with the changed segment, and with both segments, removed.
        fn surfaces(&self, words: &mut Vec<String>) {
            let (x, y) = (self.changed, self.deleted);
            let extra: Vec<String> = words
                .iter()
                .flat_map(|w| [w.replace(x, ""), w.replace([x, y], "")])
                .filter(|w| !w.is_empty())
                .collect();
            words.extend(extra);
            words.sort();
            words.dedup();
        }
    }

    /// An epenthesis of `s` after sixteen `p`s, which no test word holds: each is a root and up to two affixes on each side, at most three segments apiece, fifteen in all.
    fn inert_epenthesis(rid: &str) -> String {
        let env: String = (0..16)
            .map(|i| format!("<Segment id=\"{rid}q{i}\" segment=\"cP\" />"))
            .collect();
        format!("<PhonologicalRule id=\"{rid}\"><Name>{rid}</Name><PhoneticInput><PhoneticSequence /></PhoneticInput><PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput><PhoneticSequence><Segment id=\"{rid}o\" segment=\"cS\" /></PhoneticSequence></PhoneticOutput><Environment><LeftEnvironment><PhoneticTemplate><PhoneticSequence>{env}</PhoneticSequence></PhoneticTemplate></LeftEnvironment></Environment></PhonologicalSubrule></PhonologicalSubrules></PhonologicalRule>")
    }

    /// A rewrite rule other than metathesis: a deletion of a segment or class, a change of one segment to another, or an epenthesis, each with an optional context on either side.
    fn rewrite(r: &mut Rng, rid: &str) -> (String, Vec<char>) {
        let t = r.below(100);
        let (input, output, deleted) = if t < 60 {
            let n = if r.chance(0.75) {
                Node::Seg(r.seg())
            } else {
                let (c, m) = CLASSES[r.below(CLASSES.len())];
                Node::Class(c, m)
            };
            (n.xml(&format!("{rid}n")), String::new(), n.members())
        } else if t < 85 {
            let (a, b) = (r.seg(), r.seg());
            (
                Node::Seg(a).xml(&format!("{rid}n")),
                Node::Seg(b).xml(&format!("{rid}o")),
                Vec::new(),
            )
        } else {
            (
                String::new(),
                Node::Seg(r.seg()).xml(&format!("{rid}o")),
                Vec::new(),
            )
        };
        let epenthesis = input.is_empty();
        let left = (epenthesis || r.chance(0.25)).then(|| node(r));
        let right = r.chance(0.25).then(|| node(r));
        let side = |name: &str, n: &Option<Node>| {
            n.as_ref().map_or(String::new(), |n| {
                format!("<{name}Environment><PhoneticTemplate><PhoneticSequence>{}</PhoneticSequence></PhoneticTemplate></{name}Environment>", n.xml(&format!("{rid}{name}")))
            })
        };
        let env = if left.is_some() || right.is_some() {
            format!(
                "<Environment>{}{}</Environment>",
                side("Left", &left),
                side("Right", &right)
            )
        } else {
            String::new()
        };
        let input = if input.is_empty() {
            "<PhoneticSequence />".to_string()
        } else {
            format!("<PhoneticSequence>{input}</PhoneticSequence>")
        };
        let output = if output.is_empty() {
            "<PhoneticSequence />".to_string()
        } else {
            format!("<PhoneticSequence>{output}</PhoneticSequence>")
        };
        (format!("<PhonologicalRule id=\"{rid}\"><Name>{rid}</Name><PhoneticInput>{input}</PhoneticInput><PhonologicalSubrules><PhonologicalSubrule><PhoneticOutput>{output}</PhoneticOutput>{env}</PhonologicalSubrule></PhonologicalSubrules></PhonologicalRule>"), deleted)
    }

    /// Grammar `k` of the seeded stream, as HC XML, and the words to compare on it.
    pub fn grammar(seed: u64, k: usize) -> (String, Vec<String>) {
        grammar_variant(seed, k, Variant::Full)
    }

    /// Grammar `k` of the seeded stream written as `variant`; every variant makes the same draws, so the words are the same.
    pub fn grammar_variant(seed: u64, k: usize, variant: Variant) -> (String, Vec<String>) {
        let mut r = Rng(seed ^ (k as u64).wrapping_mul(0xA24B_AED4_963E_E407));
        // (id, xml, is metathesis)
        let mut rules: Vec<(String, String, bool)> = Vec::new();
        let mut first_pair = (Vec::new(), Vec::new());
        let count = if r.chance(0.55) { 1 } else { 2 };
        for j in 0..count {
            let rid = format!("pr{j}");
            let (a, b) = (node(&mut r), node(&mut r));
            if j == 0 {
                first_pair = (a.members(), b.members());
            }
            let mut nodes = vec![a];
            if r.chance(0.1) {
                nodes.push(Node::Boundary);
            }
            nodes.push(b);
            let (mut fi, mut si) = (0, nodes.len() - 1);
            if r.chance(0.15) {
                nodes.insert(0, node(&mut r));
                fi += 1;
                si += 1;
            }
            if r.chance(0.15) {
                nodes.push(node(&mut r));
            }
            // Morph edges are drawn to match a neighbouring pair of pattern nodes, often one that holds a context node.
            if j == 0 && nodes.len() > 2 && r.chance(0.5) {
                let segs: Vec<&Node> = nodes
                    .iter()
                    .filter(|n| !matches!(n, Node::Boundary))
                    .collect();
                let at = r.below(segs.len() - 1);
                first_pair = (segs[at].members(), segs[at + 1].members());
            }
            // FieldWorks names the later node as the left switch; hand-written grammars may name the earlier one.
            let (left, right) = if r.chance(0.15) { (fi, si) } else { (si, fi) };
            let rtl = if r.chance(0.4) {
                " multipleApplicationOrder=\"rightToLeftIterative\""
            } else {
                ""
            };
            let seq: String = nodes
                .iter()
                .enumerate()
                .map(|(i, n)| n.xml(&format!("{rid}n{i}")))
                .collect();
            let xml = format!("<MetathesisRule id=\"{rid}\" leftSwitch=\"{rid}n{left}\" rightSwitch=\"{rid}n{right}\"{rtl}><Name>{rid}</Name><StructuralDescription><PhoneticTemplate><PhoneticSequence>{seq}</PhoneticSequence></PhoneticTemplate></StructuralDescription></MetathesisRule>");
            rules.push((rid, xml, true));
        }
        // Up to two other rewrites, each placed anywhere in the rule order.
        let mut deletable = Vec::new();
        for (j, p) in [0.35, 0.15].into_iter().enumerate() {
            if r.chance(p) {
                let rid = format!("pd{j}");
                let (xml, deleted) = rewrite(&mut r, &rid);
                deletable.extend(deleted);
                let at = r.below(rules.len() + 1);
                rules.insert(at, (rid, xml, false));
            }
        }
        let (x0, y0) = first_pair;
        let mut affixes = Vec::new();
        let mut compound = false;
        for i in 0..1 + r.below(3) {
            let t = r.below(100);
            let affix = if t < 4 {
                Affix::Suffix("+".to_string())
            } else if t < 7 {
                Affix::Prefix("+".to_string())
            } else if t < 15 {
                Affix::Copy
            } else if t < 22 && !compound {
                compound = true;
                Affix::Compound(if r.chance(0.5) { "+" } else { "" }.to_string())
            } else if t < 55 {
                let s = string(&mut r, 1, 3, &x0, &[]);
                Affix::Prefix(bounded(&mut r, s, 0.12, 0.6, 0.15))
            } else if t < 90 {
                let s = string(&mut r, 1, 3, &[], &y0);
                Affix::Suffix(bounded(&mut r, s, 0.6, 0.12, 0.15))
            } else {
                let p = string(&mut r, 1, 2, &x0, &[]);
                let s = string(&mut r, 1, 2, &[], &y0);
                Affix::Circumfix(
                    bounded(&mut r, p, 0.1, 0.6, 0.0),
                    bounded(&mut r, s, 0.6, 0.1, 0.0),
                )
            };
            let kind = match affix {
                Affix::Prefix(_) => "PR",
                Affix::Suffix(_) => "SU",
                Affix::Circumfix(..) => "CI",
                Affix::Copy => "ZE",
                Affix::Compound(_) => "CO",
            };
            affixes.push((format!("{kind}{i}"), affix));
        }
        let mut roots: Vec<String> = (0..3)
            .map(|_| {
                let s = string(&mut r, 1, 3, &x0, &y0);
                bounded(&mut r, s, 0.0, 0.0, 0.1)
            })
            .collect();
        // A deletable segment at a morph's edge, so a deletion can expose the segment behind it.
        if !deletable.is_empty() {
            let pad = |r: &mut Rng, s: &mut String, front: bool| {
                if !s.is_empty() && r.chance(0.3) {
                    let c = deletable[r.below(deletable.len())];
                    if front {
                        s.insert(0, c);
                    } else {
                        s.push(c);
                    }
                }
            };
            for (_, affix) in &mut affixes {
                match affix {
                    Affix::Prefix(p) => pad(&mut r, p, false),
                    Affix::Suffix(s) => pad(&mut r, s, true),
                    Affix::Circumfix(p, s) => {
                        pad(&mut r, p, false);
                        pad(&mut r, s, true);
                    }
                    Affix::Copy | Affix::Compound(_) => {}
                }
            }
            for root in &mut roots {
                let front = r.chance(0.5);
                pad(&mut r, root, front);
            }
        }
        let mut feed = (variant == Variant::ChangeFeedsDeletion).then(|| Feed::new(seed, k));
        if let Some(feed) = &mut feed {
            feed.seed_literals(&mut affixes, &mut roots);
        }
        let mut words = words(&mut r, &affixes, &roots);
        if let Some(feed) = &feed {
            feed.surfaces(&mut words);
        }
        let rids: Vec<String> = rules.iter().map(|(id, _, _)| id.clone()).collect();
        let linear = r.chance(0.5);
        let two_strata = r.chance(0.25);
        let strata = if two_strata || feed.as_ref().is_some_and(|f| f.two_strata) {
            let mut inner_rules: Vec<String> =
                rids.iter().filter(|_| r.chance(0.5)).cloned().collect();
            let mut outer_rules: Vec<String> = rids
                .iter()
                .filter(|id| !inner_rules.contains(id))
                .cloned()
                .collect();
            // One metathesis rule listed in both strata.
            if r.chance(0.3) {
                if inner_rules.contains(&"pr0".to_string()) {
                    outer_rules.insert(0, "pr0".to_string());
                } else {
                    inner_rules.push("pr0".to_string());
                }
            }
            let (mut inner, mut outer) = (Vec::new(), Vec::new());
            for a in affixes {
                if r.chance(0.5) {
                    inner.push(a);
                } else {
                    outer.push(a);
                }
            }
            vec![
                Stratum {
                    prules: inner_rules,
                    affixes: inner,
                    slots: Vec::new(),
                    roots,
                    linear,
                },
                Stratum {
                    prules: outer_rules,
                    affixes: outer,
                    slots: Vec::new(),
                    roots: Vec::new(),
                    linear,
                },
            ]
        } else {
            let slots = if r.chance(0.2) {
                let mut slots: Vec<String> = affixes
                    .iter()
                    .filter(|(_, a)| !matches!(a, Affix::Compound(_)))
                    .map(|(id, _)| id.clone())
                    .filter(|_| r.chance(0.7))
                    .collect();
                if slots.is_empty() {
                    if let Some((id, _)) = affixes
                        .iter()
                        .find(|(_, a)| !matches!(a, Affix::Compound(_)))
                    {
                        slots.push(id.clone());
                    }
                }
                slots
            } else {
                Vec::new()
            };
            vec![Stratum {
                prules: rids,
                affixes,
                slots,
                roots,
                linear,
            }]
        };
        let mut strata = strata;
        if let Some(feed) = &mut feed {
            rules.extend(feed.rules());
            feed.place(&mut strata);
        }
        // Some grammars list no affix, so every word is one root and the one-morph shortcut admits them.
        if r.chance(0.1) {
            for s in &mut strata {
                s.affixes.clear();
                s.slots.clear();
            }
        }
        let metathesis: Vec<String> = rules
            .iter()
            .filter(|(_, _, meta)| *meta)
            .map(|(id, _, _)| id.clone())
            .collect();
        let dropped: &[String] = if matches!(
            variant,
            Variant::NoMetathesis | Variant::ChangeFeedsDeletion
        ) {
            &metathesis
        } else {
            &[]
        };
        let mut xml = HEAD.to_string();
        xml.push_str(&segment_classes());
        xml.push_str("</NaturalClasses><PhonologicalRuleDefinitions>");
        for (id, rule, meta) in &rules {
            if *meta && variant == Variant::InertEpenthesis {
                xml.push_str(&inert_epenthesis(id));
            } else {
                xml.push_str(rule);
            }
        }
        xml.push_str("</PhonologicalRuleDefinitions><Strata>");
        for (i, s) in strata.iter().enumerate() {
            xml.push_str(&stratum_xml(i, s, dropped));
        }
        xml.push_str("</Strata></Language></HermitCrabInput>");
        (xml, words)
    }

    /// Every short string, plus each root with up to two prefixes and two suffixes, with up to two swaps of neighbouring segments that carry or leave the boundaries between them, a deleted segment, and each without boundaries; sampled down to `WORDS`.
    fn words(r: &mut Rng, affixes: &[(String, Affix)], roots: &[String]) -> Vec<String> {
        let mut prefixes = Vec::new();
        let mut suffixes = Vec::new();
        let mut stems: Vec<String> = roots.to_vec();
        for (_, affix) in affixes {
            match affix {
                Affix::Prefix(p) => prefixes.push(p.clone()),
                Affix::Suffix(s) => suffixes.push(s.clone()),
                Affix::Circumfix(p, s) => {
                    prefixes.push(p.clone());
                    suffixes.push(s.clone());
                }
                Affix::Copy => {}
                Affix::Compound(link) => {
                    for a in roots {
                        for b in roots {
                            stems.push(format!("{a}{link}{b}"));
                        }
                    }
                }
            }
        }
        let sequences = |pieces: &[String]| {
            let mut out = vec![String::new()];
            for (i, a) in pieces.iter().enumerate() {
                out.push(a.clone());
                for (j, b) in pieces.iter().enumerate() {
                    if i != j {
                        out.push(format!("{a}{b}"));
                    }
                }
            }
            out
        };
        let mut bases = BTreeSet::new();
        for root in &stems {
            for p in sequences(&prefixes) {
                for s in sequences(&suffixes) {
                    bases.insert(format!("{p}{root}{s}"));
                }
            }
        }
        let mut swapped = bases.clone();
        let mut level = bases;
        for _ in 0..2 {
            let next: BTreeSet<String> = level.iter().flat_map(|w| swaps(w)).collect();
            swapped.extend(next.iter().cloned());
            level = next;
        }
        let mut found = BTreeSet::new();
        for w in &swapped {
            found.insert(w.clone());
            found.insert(w.replace('+', ""));
            for (i, c) in w.char_indices() {
                if c != '+' {
                    let mut d = w.clone();
                    d.remove(i);
                    found.insert(d.replace('+', ""));
                    found.insert(d);
                }
            }
        }
        found.retain(|w| !w.is_empty() && w.len() <= 12);
        let mut found: Vec<String> = found.into_iter().collect();
        for i in (1..found.len()).rev() {
            found.swap(i, r.below(i + 1));
        }
        found.truncate(WORDS);
        let mut short = vec![String::new()];
        for _ in 0..3 {
            short = short
                .iter()
                .flat_map(|w| SEGS.iter().map(move |c| format!("{w}{c}")))
                .collect();
            found.extend(short.iter().cloned());
        }
        found.sort();
        found.dedup();
        found
    }

    fn swaps(w: &str) -> Vec<String> {
        let chars: Vec<char> = w.chars().collect();
        let segs: Vec<usize> = (0..chars.len()).filter(|&i| chars[i] != '+').collect();
        let mut out = Vec::new();
        for pair in segs.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let mid: String = chars[a + 1..b].iter().collect();
            let head: String = chars[..a].iter().collect();
            let tail: String = chars[b + 1..].iter().collect();
            let one: String = mid.chars().take(1).collect();
            for m in [mid.as_str(), "", one.as_str()] {
                for (l, rr) in [("", ""), (mid.as_str(), ""), ("", mid.as_str())] {
                    out.push(format!("{head}{l}{}{m}{}{rr}{tail}", chars[b], chars[a]));
                }
            }
        }
        out
    }
}
