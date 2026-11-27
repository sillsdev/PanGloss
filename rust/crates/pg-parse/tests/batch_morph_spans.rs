//! `parse_word_morph_spans`: each parse reading's per-morph surface spans, on conformance fixtures and against `generate`'s.

use std::collections::HashSet;

use pg_conformance_fixtures::require_fixture;
use pg_featstruct::FeatureStruct;
use pg_grammar_model::model::{AllomorphOwner, Grammar, LexEntryId, MRuleId, MorphRuleDef};
use pg_parse::morpher::{ParseSpans, TracedGenerateLimits};
use pg_parse::{project_parse_analysis_with_morphemes, GenMorpheme, Morpher, ParseOptions};
use unicode_normalization::UnicodeNormalization;

/// The fixture's grammar with synthetic source GUIDs, which an XML grammar lacks and the FieldWorks projection needs; circumfix allomorphs get two forms.
fn load(fixture: (&str, &str)) -> Grammar {
    guid_grammar(&require_fixture(fixture.0, fixture.1), &[])
}

/// `words` find the circumfix allomorphs: one that two records of one reading share.
fn guid_grammar(fixture: &pg_conformance_fixtures::FixtureRef, words: &[String]) -> Grammar {
    let one_form = with_guids(fixture.load_grammar(), &HashSet::new());
    let morpher = Morpher::new(&one_form, usize::MAX);
    let mut two_form = HashSet::new();
    for word in words {
        for analysis in morpher.parse_word(word).structured {
            let mut seen = HashSet::new();
            for o in &analysis.morph_occurrences {
                if !seen.insert((o.allomorph_id, o.morpheme_id)) {
                    two_form.insert(o.allomorph_id as usize);
                }
            }
        }
    }
    with_guids(fixture.load_grammar(), &two_form)
}

fn with_guids(mut g: Grammar, two_form: &HashSet<usize>) -> Grammar {
    let guid = |a: usize, b: usize| format!("{:08x}-0000-4000-8000-{:012x}", a + 1, b + 1);
    for (i, source) in g.allomorph_sources.iter_mut().enumerate() {
        let slots = if two_form.contains(&i) { 2 } else { 1 };
        source.form_guids = (0..slots).map(|k| Some(guid(i, k))).collect();
    }
    for (i, morpheme) in g.morphemes.iter_mut().enumerate() {
        morpheme.source_msa_guid = Some(guid(i, 0));
    }
    g
}

/// A run's segment range and byte offsets.
type Run = ((usize, usize), Option<(usize, usize)>);

/// What one morph got: its runs with their text, or the typed null.
#[derive(Debug, PartialEq, Eq, Clone)]
enum Got {
    Runs(Vec<(usize, usize)>),
    Null(&'static str),
}

fn runs(list: &[(usize, usize)]) -> Got {
    Got::Runs(list.to_vec())
}

/// One reading: its morphs' ids in surface order and their spans.
struct Reading {
    morph_ids: Vec<String>,
    /// The grammar morpheme behind each of `morph_ids`.
    morphemes: Vec<u32>,
    spans: ParseSpans,
}

impl Reading {
    fn got(&self, morph_id: &str) -> Got {
        let item = self.morph_ids.iter().position(|id| id == morph_id).unwrap();
        let m = &self.spans.spans.morphemes[item];
        match (&m.spans, &m.reason) {
            (Some(list), None) => Got::Runs(list.iter().map(|s| s.segments).collect()),
            (None, Some(reason)) => Got::Null(reason.code),
            other => panic!("spans and reason must exclude each other: {other:?}"),
        }
    }

    /// Each morph id once, in surface order.
    fn unique_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for id in &self.morph_ids {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        ids
    }

    /// All items of `morph_id` together: every run, in surface order, or the first typed null.
    fn merged(&self, morph_id: &str) -> Got {
        let mut all = Vec::new();
        for (item, id) in self.morph_ids.iter().enumerate() {
            if id != morph_id {
                continue;
            }
            let m = &self.spans.spans.morphemes[item];
            match (&m.spans, &m.reason) {
                (Some(list), None) => all.extend(list.iter().map(|s| s.segments)),
                (None, Some(reason)) => return Got::Null(reason.code),
                other => panic!("spans and reason must exclude each other: {other:?}"),
            }
        }
        all.sort();
        Got::Runs(all)
    }

    /// Runs, unattributed runs and text must account for the surface exactly once, whoever computed them.
    fn assert_invariants(&self, label: &str) {
        let spans = &self.spans.spans;
        assert_eq!(spans.morphemes.len(), self.morph_ids.len(), "{label}");
        let mut covered: Vec<Run> = Vec::new();
        for m in &spans.morphemes {
            assert_eq!(m.spans.is_none(), m.reason.is_some(), "{label}: {m:?}");
            covered.extend(m.spans.iter().flatten().map(|s| (s.segments, s.offsets)));
        }
        covered.extend(
            spans
                .unattributed
                .iter()
                .map(|u| (u.span.segments, u.span.offsets)),
        );
        covered.sort();
        let mut next = 0;
        let mut text = String::new();
        let mut all_offsets = true;
        for ((start, end), offsets) in &covered {
            assert_eq!(*start, next, "{label}: gap or overlap in {spans:?}");
            assert!(start < end, "{label}: empty run");
            next = *end;
            match offsets {
                Some((a, b)) => text.push_str(&self.spans.surface[*a..*b]),
                None => all_offsets = false,
            }
        }
        assert_eq!(next, spans.segment_count, "{label}: runs stop short");
        if all_offsets {
            assert_eq!(
                text, self.spans.surface,
                "{label}: text does not rebuild the surface"
            );
        }
    }

    fn text(&self, morph_id: &str) -> Vec<Option<String>> {
        let item = self.morph_ids.iter().position(|id| id == morph_id).unwrap();
        let nfd = &self.spans.surface;
        self.spans.spans.morphemes[item]
            .spans
            .iter()
            .flatten()
            .map(|s| s.offsets.map(|(a, b)| nfd[a..b].to_string()))
            .collect()
    }

    fn unattributed(&self) -> Vec<(usize, usize, &'static str)> {
        self.spans
            .spans
            .unattributed
            .iter()
            .map(|u| (u.span.segments.0, u.span.segments.1, u.reason.code))
            .collect()
    }
}

fn readings(g: &Grammar, word: &str) -> Vec<Reading> {
    let morpher = Morpher::new(g, usize::MAX);
    let opts = ParseOptions::default().with_morph_spans(true);
    let (outcome, spans) = morpher.parse_word_morph_spans(word, &opts);
    assert_eq!(
        spans.len(),
        outcome.structured.len(),
        "{word}: one answer per reading"
    );
    outcome
        .structured
        .iter()
        .zip(spans)
        .filter_map(|(analysis, spans)| {
            let ids = project_parse_analysis_with_morphemes(analysis, g)
                .ok()?
                .morphemes;
            Some(Reading {
                morphemes: ids.clone(),
                morph_ids: ids
                    .iter()
                    .map(|&id| {
                        g.morphemes[id as usize]
                            .morph_id
                            .clone()
                            .unwrap_or_default()
                    })
                    .collect(),
                spans: spans.expect("a projected reading has a span answer"),
            })
        })
        .collect()
}

/// The one reading of `word` whose morphs are exactly `morphs`.
fn reading(fixture: (&str, &str), word: &str, morphs: &[&str]) -> Reading {
    let g = load(fixture);
    let mut found: Vec<Reading> = readings(&g, word)
        .into_iter()
        .filter(|r| {
            let mut have = r.morph_ids.clone();
            have.sort();
            let mut want: Vec<String> = morphs.iter().map(|m| m.to_string()).collect();
            want.sort();
            have == want
        })
        .collect();
    assert!(
        !found.is_empty(),
        "{word} must have a reading of {morphs:?}"
    );
    // Homophonous readings of one morph set must agree.
    for other in &found[1..] {
        assert_eq!(
            other.spans.spans, found[0].spans.spans,
            "{word}: readings of {morphs:?} disagree"
        );
    }
    found.swap_remove(0)
}

#[test]
fn prefix_root_and_suffix_each_get_their_own_run_and_text() {
    let r = reading(
        ("edge-cases", "mpr-gated-exception"),
        "mentanukam",
        &["NPFX", "TANUK", "DELEXCLSRC"],
    );
    assert_eq!(r.got("NPFX"), runs(&[(0, 3)]));
    assert_eq!(r.got("TANUK"), runs(&[(3, 8)]));
    assert_eq!(r.got("DELEXCLSRC"), runs(&[(8, 10)]));
    assert_eq!(r.text("NPFX"), [Some("men".to_string())]);
    assert_eq!(r.text("TANUK"), [Some("tanuk".to_string())]);
    assert_eq!(r.text("DELEXCLSRC"), [Some("am".to_string())]);
    assert_eq!(r.unattributed(), []);
}

#[test]
fn an_infix_splits_the_root_into_two_runs() {
    let r = reading(
        ("edge-cases", "infix-interdigitation"),
        "kpfotab",
        &["KTB", "PFV"],
    );
    assert_eq!(r.got("KTB"), runs(&[(0, 1), (3, 7)]));
    assert_eq!(r.got("PFV"), runs(&[(1, 3)]));
    assert_eq!(
        r.text("KTB"),
        [Some("k".to_string()), Some("otab".to_string())]
    );
    assert_eq!(r.text("PFV"), [Some("pf".to_string())]);
}

#[test]
fn a_truncation_is_a_typed_null_for_every_morph() {
    let fixture = ("edge-cases", "truncate-morphotactic-identity");
    let g = load(fixture);
    let all = readings(&g, "a");
    assert!(!all.is_empty(), "the fixture parses a");
    let mut nulls = 0;
    for r in &all {
        for id in &r.morph_ids {
            if r.got(id) == Got::Null("span_unknown_truncation") {
                nulls += 1;
            }
            assert!(
                matches!(r.got(id), Got::Null(_)),
                "{id}: a truncated word reports no span"
            );
        }
    }
    assert!(nulls > 0);
}

#[test]
fn reduplicated_material_belongs_to_the_reduplicating_affix() {
    let r = reading(
        ("edge-cases", "bracket-pattern-reduplication-affix"),
        "kukuma",
        &["PFX", "KUMA"],
    );
    assert_eq!(r.got("PFX"), runs(&[(0, 2)]));
    assert_eq!(r.got("KUMA"), runs(&[(2, 6)]));
    assert_eq!(r.text("PFX"), [Some("ku".to_string())]);
}

#[test]
fn an_epenthetic_segment_is_unattributed_to_its_rule() {
    let r = reading(
        ("edge-cases", "rtl-epenthesis-cursor-after-apply"),
        "uyu",
        &["SUBRULE"],
    );
    assert_eq!(r.got("SUBRULE"), runs(&[(0, 1), (2, 3)]));
    assert_eq!(r.unattributed(), [(1, 2, "inserted_by_rule")]);
}

#[test]
fn an_xml_circumfix_gives_each_half_its_own_run() {
    let fixture = require_fixture("edge-cases", "circumfix-cross-product-and-infix-drop");
    let g = guid_grammar(&fixture, &["pabatidan".to_string()]);
    let all = readings(&g, "pabatidan");
    let r = all
        .iter()
        .find(|r| r.morph_ids.iter().filter(|id| *id == "CROSS").count() == 2)
        .expect("a reading with both halves as separate morphs");
    r.assert_invariants("pabatidan");
    assert_eq!(r.morph_ids, ["CROSS", "ROOT", "CROSS"]);
    let texts: Vec<Vec<Option<String>>> = (0..3)
        .map(|item| {
            r.spans.spans.morphemes[item]
                .spans
                .iter()
                .flatten()
                .map(|s| s.offsets.map(|(a, b)| r.spans.surface[a..b].to_string()))
                .collect()
        })
        .collect();
    let some = |t: &str| vec![Some(t.to_string())];
    assert_eq!(texts, [some("pa"), some("batid"), some("an")]);
    assert_eq!(r.spans.spans.unattributed, []);
}

/// A FieldWorks project's circumfix halves are two morphs of one morpheme with their own forms.
#[test]
fn a_fieldworks_circumfix_gives_each_half_its_own_run() {
    let fixture = require_fixture("edge-cases", "adhoc-prohibition-circumfix-pairings");
    let project = fixture.dir.join("fieldworks").join("project.fwdata");
    let (snapshot, _report) = pg_fwdata::import_file(&project).unwrap();
    let (g, _warnings) = pg_grammar::compile_project(&snapshot).unwrap();
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default().with_morph_spans(true);
    let mut halves = 0;
    for word in ["tagaka", "tagaki", "togaka", "togaki"] {
        let (outcome, spans) = morpher.parse_word_morph_spans(word, &opts);
        assert!(!outcome.structured.is_empty(), "{word} parses");
        for (analysis, spans) in outcome.structured.iter().zip(spans) {
            let projected = project_parse_analysis_with_morphemes(analysis, &g).unwrap();
            let spans = spans.expect("a projected reading has spans");
            assert_eq!(projected.analysis.morphs.len(), 3, "{word}");
            let text = |item: usize| -> String {
                let runs = spans.spans.morphemes[item].spans.as_ref().expect("no null");
                runs.iter()
                    .map(|s| {
                        let (a, b) = s.offsets.unwrap();
                        &spans.surface[a..b]
                    })
                    .collect()
            };
            assert_eq!(
                [text(0), text(1), text(2)],
                [&word[..2], "ga", &word[4..]],
                "{word}"
            );
            halves += 2;
        }
    }
    assert!(
        halves >= 8,
        "all four pairings project, saw {halves} halves"
    );
}

#[test]
fn a_guessed_root_gets_the_surface_it_guessed() {
    let g = load(("edge-cases", "guesser-pattern-root-fallback"));
    let morpher = Morpher::new(&g, usize::MAX);
    let opts = ParseOptions::default()
        .with_guess_root(true)
        .with_morph_spans(true);
    let (outcome, spans) = morpher.parse_word_morph_spans("gag", &opts);
    assert!(outcome.guessed && !outcome.structured.is_empty());
    assert_eq!(spans.len(), outcome.structured.len());
    for spans in spans {
        let spans = spans.expect("a guessed reading has spans");
        let root = &spans.spans.morphemes[0];
        let run = root.spans.as_ref().expect("the guessed root is placed")[0];
        let (a, b) = run.offsets.unwrap();
        assert_eq!(&spans.surface[a..b], "gag");
    }
}

#[test]
fn spans_are_not_tracked_unless_asked() {
    let g = load(("edge-cases", "mpr-gated-exception"));
    let morpher = Morpher::new(&g, usize::MAX);
    let (outcome, spans) = morpher.parse_word_morph_spans("mentanukam", &ParseOptions::default());
    assert!(!outcome.structured.is_empty());
    assert!(spans.is_empty());
}

/// How a reading's seed order was found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SeedOrder {
    /// The reading's own unapplied rules, outermost first.
    Derivation,
    /// No rule order matched the reading; prefixes left to right, then suffixes right to left.
    Position,
}

/// The seed `generate` takes for `r`: the head root, then its morphemes in the reading's own unapplication order, outermost first.
/// docs/generation.md:37 says only that the order is not surface order; the positional fallback is a heuristic and is counted.
fn seed_of(g: &Grammar, r: &Reading) -> Option<(LexEntryId, Vec<GenMorpheme>, SeedOrder)> {
    if let Some((root, seed)) = derivation_seed(g, r) {
        return Some((root, seed, SeedOrder::Derivation));
    }
    let entry_of = |m: u32| g.entries.iter().position(|e| e.morpheme.0 == m);
    let roots: Vec<usize> = (0..r.morphemes.len())
        .filter(|&i| entry_of(r.morphemes[i]).is_some())
        .collect();
    let [root_at] = roots[..] else {
        return None;
    };
    let mut seed = Vec::new();
    for (i, &m) in r.morphemes.iter().enumerate() {
        if i != root_at {
            seed.push((i, GenMorpheme::Rule(rule_of_morpheme(g, m)?)));
        }
    }
    // Prefixes are outer the further left they stand, suffixes the further right.
    let (mut prefixes, suffixes): (Vec<_>, Vec<_>) =
        seed.into_iter().partition(|(i, _)| *i < root_at);
    prefixes.extend(suffixes.into_iter().rev());
    let seed = prefixes.into_iter().map(|(_, m)| m).collect();
    Some((
        LexEntryId(entry_of(r.morphemes[root_at])? as u32),
        seed,
        SeedOrder::Position,
    ))
}

fn rule_of_morpheme(g: &Grammar, morpheme: u32) -> Option<MRuleId> {
    g.mrules
        .iter()
        .position(|r| r.morpheme().is_some_and(|m| m.0 == morpheme))
        .map(|i| MRuleId(i as u32))
}

/// How many compounding rules the reading unapplied; a compounding rule is no morpheme.
fn compounding_steps(g: &Grammar, r: &Reading) -> usize {
    r.spans
        .unapplied
        .iter()
        .filter(|(rule, _)| rule.is_some_and(|rule| g.mrules[rule.0 as usize].morpheme().is_none()))
        .count()
}

/// The `steps` non-head roots, when all one entry so the trail can say which compounding step took which; an infixed head is one root in two runs.
fn non_head_roots(
    g: &Grammar,
    r: &Reading,
    head: LexEntryId,
    steps: usize,
) -> Option<Vec<LexEntryId>> {
    let mut roots: Vec<LexEntryId> = r
        .morphemes
        .iter()
        .filter_map(|&m| g.entries.iter().position(|e| e.morpheme.0 == m))
        .map(|e| LexEntryId(e as u32))
        .collect();
    roots.remove(roots.iter().position(|&e| e == head)?);
    while roots.len() > steps {
        roots.remove(roots.iter().position(|&e| e == head)?);
    }
    (roots.len() == steps && roots.windows(2).all(|w| w[0] == w[1])).then_some(roots)
}

/// The head root and the reading's own unapplied steps, when together they account for exactly the reading's morphemes.
fn derivation_seed(g: &Grammar, r: &Reading) -> Option<(LexEntryId, Vec<GenMorpheme>)> {
    let AllomorphOwner::Root(root, _) = g.allomorph_owners[r.spans.head?.0 as usize] else {
        return None;
    };
    let mut covered = vec![g.entries[root.0 as usize].morpheme.0];
    let compounding = compounding_steps(g, r);
    let mut non_heads = non_head_roots(g, r, root, compounding)?;
    let mut seed = Vec::new();
    for &(rule, _) in &r.spans.unapplied {
        let affix = rule.and_then(|rule| Some((rule, g.mrules[rule.0 as usize].morpheme()?)));
        if let Some((rule, morpheme)) = affix {
            covered.push(morpheme.0);
            seed.push(GenMorpheme::Rule(rule));
            continue;
        }
        // A compounding rule is no morpheme and its trail slot does not name the non-head root; the reading's other roots do.
        let non_head = non_heads.pop()?;
        covered.push(g.entries[non_head.0 as usize].morpheme.0);
        seed.push(GenMorpheme::NonHead(non_head));
    }
    if !non_heads.is_empty() {
        return None;
    }
    // A realizational rule never occupies a trail slot, so the trail cannot place it; it is unapplied first.
    let mut realizational = Vec::new();
    for &m in &r.morphemes {
        if let Some(rule) = rule_of_morpheme(g, m) {
            if matches!(g.mrules[rule.0 as usize], MorphRuleDef::Realizational(_)) {
                covered.push(m);
                realizational.push(GenMorpheme::Rule(rule));
            }
        }
    }
    let mut have = r.morphemes.clone();
    have.sort();
    have.dedup();
    covered.sort();
    covered.dedup();
    realizational.extend(seed);
    (have == covered).then_some((root, realizational))
}

/// Every answer `generate` gives from `r`'s seed. It renders each segment's first spelling, so the word it makes is matched to the surface the reading's own spans index, not to the input spelling.
fn generate_answers(g: &Grammar, r: &Reading) -> Option<GenerateAnswers> {
    let (root, others, order) = seed_of(g, r)?;
    let morpher = Morpher::new(g, usize::MAX);
    let outcome = morpher.generate_words_traced(
        root,
        &others,
        FeatureStruct::EMPTY,
        &TracedGenerateLimits {
            work_limit: 200_000,
            timeout: None,
            max_seeds: 200,
            max_trace_nodes: Some(200_000),
        },
    );
    let mut found = GenerateAnswers {
        answers: Vec::new(),
        surface: false,
        order,
        generated: Vec::new(),
    };
    for seed in &outcome.seeds {
        found
            .generated
            .extend(seed.surfaces.iter().map(|s| s.surface.clone()));
        for surface in seed
            .surfaces
            .iter()
            .filter(|s| s.surface.nfd().collect::<String>() == r.spans.surface)
        {
            found.surface = true;
            for analysis in &surface.morph_spans {
                let mut answer: Vec<(String, Got)> = Vec::new();
                for (item, m) in seed.items.iter().zip(&analysis.spans.morphemes) {
                    let id = g.morphemes[item.morpheme.unwrap().0 as usize]
                        .morph_id
                        .clone()
                        .unwrap_or_default();
                    let got = match (&m.spans, &m.reason) {
                        (Some(list), _) => Got::Runs(list.iter().map(|s| s.segments).collect()),
                        (None, Some(r)) => Got::Null(r.code),
                        _ => unreachable!(),
                    };
                    // One entry per morph id, as `Reading::merged` gives: every run together, or the first typed null.
                    match answer.iter_mut().find(|(other, _)| *other == id) {
                        None => answer.push((id, got)),
                        Some((_, Got::Null(_))) => {}
                        Some((_, mine @ Got::Runs(_))) => match got {
                            Got::Runs(more) => {
                                let Got::Runs(all) = mine else { unreachable!() };
                                all.extend(more);
                                all.sort();
                            }
                            null => *mine = null,
                        },
                    }
                }
                found.answers.push(answer);
            }
        }
    }
    Some(found)
}

/// What `generate` gave for one reading's seed.
struct GenerateAnswers {
    /// One per derivation of the word with spans.
    answers: Vec<Vec<(String, Got)>>,
    /// Whether any seed produced the word at all.
    surface: bool,
    order: SeedOrder,
    /// Every surface the seed generated, for the report.
    generated: Vec<String>,
}

/// Why the gate cannot compare a reading, each with a ratchet pinned to the suite at `conformance.commit`: a new one fails the gate.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Exclusion {
    /// The trail names the non-head roots of two or more compounding steps in no order.
    OrderedCompounds,
    /// A realizational rule fires only for a seed carrying its realizational features, and this gate seeds none.
    Realizational,
}

impl Exclusion {
    const fn bound(self) -> usize {
        match self {
            Exclusion::OrderedCompounds => 75,
            Exclusion::Realizational => 3,
        }
    }
}

/// The named predicate that explains a reading the gate did not compare, or `None` when nothing does.
fn exclusion(g: &Grammar, r: &Reading) -> Option<Exclusion> {
    if compounding_steps(g, r) >= 2 {
        return Some(Exclusion::OrderedCompounds);
    }
    let realizational = r.morphemes.iter().any(|&m| {
        rule_of_morpheme(g, m)
            .is_some_and(|rule| matches!(g.mrules[rule.0 as usize], MorphRuleDef::Realizational(_)))
    });
    realizational.then_some(Exclusion::Realizational)
}

/// What a sweep saw.
#[derive(Default)]
struct Sweep {
    compared: usize,
    nulls: usize,
    /// Compared with the positional seed because no rule order matched the reading.
    position_seeds: Vec<String>,
    /// No seed could be built; `exclusions` names those a predicate explains.
    unseedable: Vec<String>,
    /// A seed was built and `generate` never produced the word.
    no_surface: Vec<String>,
    /// `generate` produced the word but gave no spans for it.
    surface_without_spans: Vec<String>,
    excluded: std::collections::BTreeMap<Exclusion, usize>,
}

/// Compares every reading of `word` with `generate`'s answers; a reading it cannot reproduce is counted, not dropped.
fn compare_word(label: &str, g: &Grammar, word: &str, sweep: &mut Sweep) {
    for r in readings(g, word) {
        r.assert_invariants(&format!("{label}: {word}"));
        let found = generate_answers(g, &r);
        let line = format!(
            "{label}: {word}: {} generated {:?}",
            r.morph_ids.join("+"),
            found.as_ref().map(|f| &f.generated)
        );
        let bucket = match &found {
            None => &mut sweep.unseedable,
            Some(f) if !f.surface => &mut sweep.no_surface,
            Some(f) if f.answers.is_empty() => &mut sweep.surface_without_spans,
            Some(f) => {
                if f.order == SeedOrder::Position {
                    sweep.position_seeds.push(line);
                }
                check(
                    label,
                    word,
                    &r,
                    &f.answers,
                    &mut sweep.compared,
                    &mut sweep.nulls,
                );
                continue;
            }
        };
        match exclusion(g, &r) {
            Some(reason) => *sweep.excluded.entry(reason).or_default() += 1,
            None => bucket.push(line),
        }
    }
}

/// For every fixture word with a parse, each reading's spans equal an answer `generate` gives for the same morphs.
#[test]
fn batch_spans_equal_generate_spans_on_every_fixture_parse() {
    let mut sweep = Sweep::default();
    for fixture in
        pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
    {
        let yaml = fixture.load_words_yaml();
        if yaml.skip_in_generic_replay().is_some() {
            continue;
        }
        let words: Vec<String> = yaml.words.iter().map(|e| e.word.clone()).collect();
        let g = guid_grammar(&fixture, &words);
        for entry in &yaml.words {
            if entry.parses.iter().any(|p| !p.guess) {
                compare_word(&fixture.label(), &g, &entry.word, &mut sweep);
            }
        }
    }
    eprintln!(
        "compared {} ({} with a typed null, {} positional seeds); excluded {:?}; unexplained: {} unseedable, {} no surface, {} surface without spans",
        sweep.compared,
        sweep.nulls,
        sweep.position_seeds.len(),
        sweep.excluded,
        sweep.unseedable.len(),
        sweep.no_surface.len(),
        sweep.surface_without_spans.len()
    );
    let unexplained = |name: &str, list: &[String]| {
        assert!(
            list.is_empty(),
            "{name}: {} readings the gate did not compare:\n{}",
            list.len(),
            list.join("\n")
        );
    };
    unexplained(
        "positional seeds (no rule order matched)",
        &sweep.position_seeds,
    );
    unexplained("unseedable", &sweep.unseedable);
    unexplained("generate made no such surface", &sweep.no_surface);
    unexplained("surface without spans", &sweep.surface_without_spans);
    for (exclusion, count) in &sweep.excluded {
        assert!(
            *count <= exclusion.bound(),
            "{exclusion:?}: {count} readings excluded, bound {}",
            exclusion.bound()
        );
    }
    assert!(
        sweep.compared > 100,
        "expected a large sweep, compared {}",
        sweep.compared
    );
}

/// An invented grammar over a root `bak` (posN); `rules` are suffix or prefix definitions and `order` their ids.
fn synthetic_grammar(rules: &[String], order: &str) -> Grammar {
    let xml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE HermitCrabInput SYSTEM "HermitCrabInput.dtd">
<HermitCrabInput><Language><Name>Synthetic</Name>
<PartsOfSpeech><PartOfSpeech id="posN"><Name>noun</Name></PartOfSpeech><PartOfSpeech id="posV"><Name>verb</Name></PartOfSpeech></PartsOfSpeech>
<CharacterDefinitionTable id="table1"><Name>Main</Name><SegmentDefinitions>
<SegmentDefinition id="cB"><Representations><Representation>b</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cA"><Representations><Representation>a</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cK"><Representations><Representation>k</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cP"><Representations><Representation>p</Representation></Representations></SegmentDefinition>
<SegmentDefinition id="cS"><Representations><Representation>s</Representation></Representations></SegmentDefinition>
</SegmentDefinitions></CharacterDefinitionTable>
<NaturalClasses><FeatureNaturalClass id="ncAny"><Name>Any</Name></FeatureNaturalClass></NaturalClasses>
<Strata><Stratum characterDefinitionTable="table1" morphologicalRuleOrder="unordered" morphologicalRules="{order}"><Name>Main</Name>
<MorphologicalRuleDefinitions>{}</MorphologicalRuleDefinitions>
<LexicalEntries><LexicalEntry id="ROOT" partOfSpeech="posN"><Allomorphs><Allomorph id="ROOT_bak"><PhoneticShape>bak</PhoneticShape></Allomorph></Allomorphs><MorphemeId>ROOT</MorphemeId><Gloss>ROOT</Gloss></LexicalEntry></LexicalEntries>
</Stratum></Strata></Language></HermitCrabInput>"#,
        rules.concat()
    );
    with_guids(
        pg_grammar::load(&xml).unwrap_or_else(|e| panic!("test grammar failed to load: {e}")),
        &HashSet::new(),
    )
}

/// One affix rule that inserts `shape` after (`prefix` false) or before the input, taking `from` to `to`.
fn affix(id: &str, from: &str, to: &str, shape: &str, prefix: bool) -> String {
    let copy = format!(r#"<CopyFromInput index="{id}_s" />"#);
    let insert =
        format!(r#"<InsertSegments><PhoneticShape>{shape}</PhoneticShape></InsertSegments>"#);
    let output = if prefix {
        format!("{insert}{copy}")
    } else {
        format!("{copy}{insert}")
    };
    format!(
        r#"<MorphologicalRule id="{id}" requiredPartsOfSpeech="{from}" outputPartOfSpeech="{to}"><Name>{id}</Name><MorphologicalSubrules><MorphologicalSubrule id="{id}_sub"><MorphologicalInput><PhoneticSequence id="{id}_s"><OptionalSegmentSequence min="1" max="-1"><SimpleContext naturalClass="ncAny" /></OptionalSegmentSequence></PhoneticSequence></MorphologicalInput><MorphologicalOutput>{output}</MorphologicalOutput></MorphologicalSubrule></MorphologicalSubrules><MorphemeId>{id}</MorphemeId></MorphologicalRule>"#
    )
}

/// Two suffixes spelled alike are accepted in either order; each reading's spans must match the derivation of its own order.
#[test]
fn homophonous_affixes_are_compared_against_their_own_order() {
    let g = synthetic_grammar(
        &[
            affix("SFX1", "posN", "posN", "a", false),
            affix("SFX2", "posN", "posN", "a", false),
        ],
        "SFX1 SFX2",
    );
    let mut sweep = Sweep::default();
    compare_word("synthetic", &g, "bakaa", &mut sweep);
    assert!(sweep.unseedable.is_empty() && sweep.no_surface.is_empty());
    assert!(
        sweep.position_seeds.is_empty(),
        "{:?}",
        sweep.position_seeds
    );
    assert_eq!(sweep.compared, 2, "both orders are readings of bakaa");
}

/// A derivational prefix applies before an inflectional suffix that needs its output, so the suffix is the outer morpheme although a prefix stands left of the root.
#[test]
fn a_prefix_applied_before_a_suffix_is_seeded_by_derivation_order() {
    let g = synthetic_grammar(
        &[
            affix("PFX", "posN", "posV", "p", true),
            affix("SFX", "posV", "posV", "s", false),
        ],
        "PFX SFX",
    );
    let mut sweep = Sweep::default();
    compare_word("synthetic", &g, "pbaks", &mut sweep);
    assert!(
        sweep.position_seeds.is_empty(),
        "{:?}",
        sweep.position_seeds
    );
    assert!(sweep.unseedable.is_empty() && sweep.no_surface.is_empty());
    assert_eq!(sweep.compared, 1);
}

fn check(
    label: &str,
    word: &str,
    r: &Reading,
    answers: &[Vec<(String, Got)>],
    compared: &mut usize,
    nulls: &mut usize,
) {
    let mine: Vec<(String, Got)> = r
        .unique_ids()
        .iter()
        .map(|id| (id.clone(), r.merged(id)))
        .collect();
    let matches = answers.iter().any(|answer| {
        mine.iter().all(|(id, got)| {
            answer
                .iter()
                .find(|(other, _)| other == id)
                .is_some_and(|(_, theirs)| {
                    // `generate` does not attribute a morpheme applied twice, and says so; batch gives each application its run.
                    theirs == got || *theirs == Got::Null("repeated_morpheme")
                })
        })
    });
    assert!(
        matches,
        "{label}: {word}: batch {mine:?} is not among generate's {answers:?}"
    );
    *compared += 1;
    if mine.iter().any(|(_, got)| matches!(got, Got::Null(_))) {
        *nulls += 1;
    }
}
