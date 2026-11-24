//! `parse_word_morph_spans`: each parse reading's per-morph surface spans, on conformance fixtures and against `generate`'s.

use std::collections::HashSet;

use pg_conformance_fixtures::require_fixture;
use pg_featstruct::FeatureStruct;
use pg_grammar_model::model::{Grammar, LexEntryId, MRuleId};
use pg_parse::morpher::{ParseSpans, TracedGenerateLimits};
use pg_parse::{project_parse_analysis_with_morphemes, GenMorpheme, Morpher, ParseOptions};

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
            other.spans, found[0].spans,
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

fn seed_of(g: &Grammar, signature: &str) -> Option<(LexEntryId, Vec<GenMorpheme>)> {
    let tokens: Vec<&str> = signature.split('|').next()?.split('+').collect();
    let mut root = None;
    let mut others = Vec::new();
    for token in tokens {
        let entry = g
            .entries
            .iter()
            .position(|e| g.morphemes[e.morpheme.0 as usize].morph_id.as_deref() == Some(token));
        let rule = g.mrules.iter().position(|r| {
            r.morpheme()
                .is_some_and(|m| g.morphemes[m.0 as usize].morph_id.as_deref() == Some(token))
        });
        match (entry, rule) {
            (Some(e), None) if root.is_none() => root = Some(LexEntryId(e as u32)),
            (None, Some(r)) => others.push(GenMorpheme::Rule(MRuleId(r as u32))),
            _ => return None,
        }
    }
    Some((root?, others))
}

/// Every answer `generate` gives for `word` from `signature`'s seed, as per-morph-id results.
fn generate_answers(g: &Grammar, signature: &str, word: &str) -> Vec<Vec<(String, Got)>> {
    let Some((root, others)) = seed_of(g, signature) else {
        return Vec::new();
    };
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
    let mut answers = Vec::new();
    for seed in &outcome.seeds {
        for surface in seed.surfaces.iter().filter(|s| s.surface == word) {
            for analysis in &surface.morph_spans {
                answers.push(
                    seed.items
                        .iter()
                        .zip(&analysis.spans.morphemes)
                        .map(|(item, m)| {
                            let id = g.morphemes[item.morpheme.unwrap().0 as usize]
                                .morph_id
                                .clone()
                                .unwrap_or_default();
                            let got = match (&m.spans, &m.reason) {
                                (Some(list), _) => {
                                    Got::Runs(list.iter().map(|s| s.segments).collect())
                                }
                                (None, Some(r)) => Got::Null(r.code),
                                _ => unreachable!(),
                            };
                            (id, got)
                        })
                        .collect(),
                );
            }
        }
    }
    answers
}

/// For every fixture word with a parse, each reading's spans equal an answer `generate` gives for the same morphs.
#[test]
fn batch_spans_equal_generate_spans_on_every_fixture_parse() {
    let (mut compared, mut nulls) = (0, 0);
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
            if !entry.parses.iter().any(|p| !p.guess) {
                continue;
            }
            for r in readings(&g, &entry.word) {
                r.assert_invariants(&format!("{}: {}", fixture.label(), entry.word));
                let signature = r.unique_ids().join("+");
                let answers = generate_answers(&g, &signature, &entry.word);
                if answers.is_empty() {
                    let rotated = generate_rotations(&g, &r, &entry.word);
                    if rotated.is_empty() {
                        continue;
                    }
                    check(
                        &fixture.label(),
                        &entry.word,
                        &r,
                        &rotated,
                        &mut compared,
                        &mut nulls,
                    );
                } else {
                    check(
                        &fixture.label(),
                        &entry.word,
                        &r,
                        &answers,
                        &mut compared,
                        &mut nulls,
                    );
                }
            }
        }
    }
    assert!(
        compared > 100,
        "expected a large sweep, compared {compared}"
    );
    eprintln!("compared {compared} readings, {nulls} with a typed null");
}

/// The seed names its root first; try each morph as the root.
fn generate_rotations(g: &Grammar, r: &Reading, word: &str) -> Vec<Vec<(String, Got)>> {
    let ids = r.unique_ids();
    for root in 0..ids.len() {
        let mut order = vec![ids[root].clone()];
        order.extend(
            ids.iter()
                .enumerate()
                .filter(|(i, _)| *i != root)
                .map(|(_, id)| id.clone()),
        );
        let answers = generate_answers(g, &order.join("+"), word);
        if !answers.is_empty() {
            return answers;
        }
    }
    Vec::new()
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
                .is_some_and(|(_, theirs)| theirs == got)
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
