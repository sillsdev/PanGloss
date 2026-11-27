//! Soundness of `outputMatchPattern`: where one subrule alone rewrote a word, `match` of the output pattern on the output word reports a span at every edit.

use std::collections::BTreeMap;

use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{Grammar, PRuleId, PhonRuleDef};
use pg_parse::Morpher;
use pg_rules::rule_text::{rule_text, RuleText};
use pg_rules::shape_feat::segment_with_features_load as segment_with_features;
use pg_rules::surface_probe::render_shape_marked;
use serde_json::json;

/// An output word and, per edit, the segment ranges it may sit at.
type Edit = (String, Vec<Vec<(usize, usize)>>);

type Loader = Box<dyn Fn() -> Grammar>;

/// What one run over the grammars found.
#[derive(Default)]
struct Sweep {
    tally: BTreeMap<String, usize>,
    misses: Vec<String>,
}

fn is_boundary(kind: impl std::fmt::Debug) -> bool {
    format!("{kind:?}") == "Boundary"
}

fn conformance() -> Vec<(String, Loader)> {
    pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
        .into_iter()
        .map(|f| {
            let label = f.label().to_string();
            let load: Loader = Box::new(move || f.load_grammar());
            (label, load)
        })
        .collect()
}

fn xml(label: &str, text: &'static str) -> (String, Loader) {
    (
        label.to_string(),
        Box::new(move || pg_grammar::load(text).expect("the grammar loads")),
    )
}

fn attack_grammars() -> Vec<(String, Loader)> {
    vec![
        xml(
            "selfsim",
            include_str!("../../tests/data/effects-output-selfsim.xml"),
        ),
        xml(
            "selfiter",
            include_str!("../../tests/data/effects-output-selfiter.xml"),
        ),
        xml(
            "insertsim",
            include_str!("../../tests/data/effects-output-insertsim.xml"),
        ),
    ]
}

fn r2_grammars() -> Vec<(String, Loader)> {
    macro_rules! r2 {
        ($($n:literal),*) => {
            vec![$(xml(concat!("r2-", $n), include_str!(concat!("../../tests/data/effects-output-r2-", $n, ".xml")))),*]
        };
    }
    r2!(
        "assim", "classSim", "delL2", "expsim", "insBoth", "insL2", "iterBoth", "iterL", "rtlL",
        "rtlR", "simBoth", "simR"
    )
}

/// The aligned edit regions `(i0, i1, o0, o1)` between two node lists, from a longest common subsequence.
fn regions<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize, usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let (mut start, mut out) = (None, Vec::new());
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            if let Some((i0, j0)) = start.take() {
                out.push((i0, i, j0, j));
            }
            i += 1;
            j += 1;
        } else {
            start.get_or_insert((i, j));
            if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
    if let Some((i0, j0)) = start {
        out.push((i0, n, j0, m));
    }
    out
}

/// Applies each printable, local subrule alone to generated words and checks `pattern_of(text)` against the output words; every edit (a segment whose features changed, an inserted run, a deletion point) must lie in a reported span. Rows `skip` names are counted and left out.
fn sweep(
    grammars: Vec<(String, Loader)>,
    pattern_of: &dyn Fn(&RuleText) -> Option<String>,
    skip: &dyn Fn(&RuleText) -> bool,
) -> Sweep {
    let mut out = Sweep::default();
    let mut tally = |key: &str, n: usize| *out.tally.entry(key.to_string()).or_default() += n;
    let mut misses = Vec::new();
    for (label, load) in grammars {
        let Ok(g) = std::panic::catch_unwind(AssertUnwindSafe(&load)) else {
            continue;
        };
        let Some(last) = g.strata.last().map(|s| s.table) else {
            continue;
        };
        let mut ug = load();
        let morpher = Morpher::new(&g, 50_000_000);
        let table = &g.char_tables[last.0 as usize];
        let mut roots: Vec<String> = g
            .entries
            .iter()
            .flat_map(|e| e.allomorphs.iter().map(|a| a.shape.text.clone()))
            .filter(|t| !t.is_empty())
            .collect();
        roots.sort();
        roots.dedup();
        let mut segments = Vec::new();
        let mut boundaries = Vec::new();
        for (_, def) in table.iter() {
            let Some(first) = def.representations_nfd().first().filter(|r| !r.is_empty()) else {
                continue;
            };
            match def.kind() {
                CharDefKind::Boundary => boundaries.push(first.clone()),
                CharDefKind::Segment => segments.push(first.clone()),
            }
        }
        let marker = boundaries.first().cloned();
        let mut separators = vec![String::new()];
        separators.extend(marker.clone());
        let mut sides = vec![String::new()];
        sides.extend(segments.iter().take(6).cloned());
        sides.extend(roots.iter().take(4).cloned());
        let foci: Vec<&String> = roots
            .iter()
            .take(6)
            .chain(segments.iter().take(8))
            .collect();
        for (stratum_index, stratum) in g.strata.iter().enumerate() {
            for (ordinal, PRuleId(index)) in stratum.prules.iter().enumerate() {
                let Some(def @ PhonRuleDef::Rewrite(rule)) = g.prules.get(*index as usize) else {
                    continue;
                };
                if stratum.table != last {
                    tally("rules of another stratum table", 1);
                    continue;
                }
                let subrules = rule.subrules.len();
                for sub in 0..subrules {
                    let mut text = rule_text(&g, def, sub, stratum.table);
                    text.with_alteration(&g, stratum_index, ordinal);
                    if pattern_of(&text).is_none() {
                        tally("rows with no output pattern", 1);
                        continue;
                    }
                    if !text.non_local.is_empty() {
                        tally("rows flagged non-local", 1);
                        continue;
                    }
                    if skip(&text) {
                        tally("rows flagged for self interaction", 1);
                        continue;
                    }
                    tally("rows checked", 1);
                    // Run this subrule alone: the others' edits would be unexplained.
                    let held = {
                        let Some(PhonRuleDef::Rewrite(r)) = ug.prules.get_mut(*index as usize)
                        else {
                            continue;
                        };
                        let mut all = std::mem::take(&mut r.subrules);
                        r.subrules = vec![all.remove(sub)];
                        all
                    };
                    let Some(PhonRuleDef::Rewrite(alone)) = ug.prules.get(*index as usize) else {
                        continue;
                    };
                    let mut edits: Vec<Edit> = Vec::new();
                    for separator in &separators {
                        if text
                            .boundary_marker
                            .as_ref()
                            .is_some_and(|m| !separator.is_empty() && separator != m)
                        {
                            continue;
                        }
                        for left in &sides {
                            for focus in &foci {
                                for right in &sides {
                                    let word =
                                        format!("{left}{separator}{focus}{separator}{right}");
                                    let Ok(input) = segment_with_features(&ug, table, &word) else {
                                        continue;
                                    };
                                    let Ok(outputs) =
                                        std::panic::catch_unwind(AssertUnwindSafe(|| {
                                            pg_rules::rewrite::synthesize(&ug, alone, &input)
                                        }))
                                    else {
                                        continue;
                                    };
                                    let Some(output) = outputs.last() else {
                                        continue;
                                    };
                                    let inn: Vec<_> = input
                                        .interior()
                                        .map(|(n, k, _, _)| {
                                            (
                                                is_boundary(k),
                                                (input.char_def(n), input.node_lanes(n).to_vec()),
                                            )
                                        })
                                        .collect();
                                    let outn: Vec<_> = output
                                        .interior()
                                        .map(|(n, k, _, _)| {
                                            (
                                                is_boundary(k),
                                                (output.char_def(n), output.node_lanes(n).to_vec()),
                                            )
                                        })
                                        .collect();
                                    if inn == outn {
                                        continue;
                                    }
                                    let Some(spelled) =
                                        render_shape_marked(table, output, separator.as_str())
                                    else {
                                        tally("outputs with no spelling", 1);
                                        continue;
                                    };
                                    let seg_at = |nodes: &[(bool, _)], p: usize| {
                                        nodes[..p].iter().filter(|(b, _)| !b).count()
                                    };
                                    let mut checks: Vec<Vec<(usize, usize)>> = Vec::new();
                                    if inn.len() == outn.len() {
                                        for (p, (a, b)) in inn.iter().zip(&outn).enumerate() {
                                            if a != b && !b.0 {
                                                let at = seg_at(&outn, p);
                                                checks.push(vec![(at, at + 1)]);
                                                tally("equal-length places", 1);
                                            }
                                        }
                                    } else {
                                        for (i0, i1, o0, o1) in regions(&inn, &outn) {
                                            let (ri, ro) = (i1 - i0, o1 - o0);
                                            // A pure insertion or deletion next to an equal node can sit one place either way; every such place gives the same word.
                                            let (block, start, width) = if ro > 0 {
                                                (&outn, o0, ro)
                                            } else {
                                                (&inn, i0, ri)
                                            };
                                            let (mut lo, mut hi) = (start, start);
                                            if ri == 0 || ro == 0 {
                                                while lo > 0
                                                    && block[lo - 1] == block[lo + width - 1]
                                                {
                                                    lo -= 1;
                                                }
                                                while hi + width < block.len()
                                                    && block[hi + width] == block[hi]
                                                {
                                                    hi += 1;
                                                }
                                            }
                                            let places = (lo..=hi)
                                                .map(|q| {
                                                    if ro > 0 {
                                                        (seg_at(&outn, q), seg_at(&outn, q + width))
                                                    } else {
                                                        let at = seg_at(
                                                            &outn,
                                                            (o0 as isize + q as isize - i0 as isize)
                                                                as usize,
                                                        );
                                                        (at, at)
                                                    }
                                                })
                                                .collect();
                                            tally(
                                                match (ri, ro) {
                                                    (_, 0) => "deletion places",
                                                    (0, _) => "insertion places",
                                                    _ => "expansion places",
                                                },
                                                1,
                                            );
                                            checks.push(places);
                                        }
                                    }
                                    edits.push((spelled, checks));
                                }
                            }
                        }
                    }
                    {
                        let Some(PhonRuleDef::Rewrite(r)) = ug.prules.get_mut(*index as usize)
                        else {
                            continue;
                        };
                        let mut all = held;
                        all.insert(sub, r.subrules.pop().expect("the subrule kept"));
                        r.subrules = all;
                    }
                    tally("words the subrule rewrites", edits.len());
                    let mut request = json!({
                        "pattern": pattern_of(&text),
                        "words": edits.iter().map(|e| e.0.as_str()).collect::<Vec<_>>(),
                    });
                    if let Some(m) = text.boundary_marker.as_deref().or(marker.as_deref()) {
                        request["boundaryMarker"] = json!(m);
                    }
                    let r = crate::serve_match::handle(
                        &g,
                        &morpher,
                        &json!("m"),
                        request.as_object().unwrap(),
                    );
                    if r["type"] != "result" {
                        misses.push(format!(
                            "{label} {}: match refused `{:?}`: {r}",
                            rule.xml_id,
                            pattern_of(&text)
                        ));
                        continue;
                    }
                    for (edit, word) in edits.iter().zip(r["words"].as_array().unwrap()) {
                        if word["status"] != "ok" {
                            tally("words match cannot read", 1);
                            continue;
                        }
                        let found: Vec<(usize, usize)> = word["matches"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|m| {
                                (
                                    m["segments"][0].as_u64().unwrap() as usize,
                                    m["segments"][1].as_u64().unwrap() as usize,
                                )
                            })
                            .collect();
                        for places in &edit.1 {
                            tally("edits checked", 1);
                            let hit = places.iter().any(|(a, b)| {
                                if a == b {
                                    found.iter().any(|(x, y)| x == a && y == a)
                                } else {
                                    (*a..*b).all(|p| found.iter().any(|(x, y)| *x <= p && p < *y))
                                }
                            });
                            if !hit {
                                misses.push(format!(
                                    "{label} {} sub {sub} word `{}` places {places:?}: pattern {:?}, spans {found:?}",
                                    rule.xml_id,
                                    edit.0,
                                    pattern_of(&text)
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    out.misses = misses;
    out
}

use std::panic::AssertUnwindSafe;

fn self_flagged(t: &RuleText) -> bool {
    t.output_altered_reasons
        .iter()
        .any(|r| r.0 == "selfInteraction")
}

fn show(name: &str, s: &Sweep) {
    eprintln!("{name}: {:?}, {} misses", s.tally, s.misses.len());
}

#[test]
fn match_on_the_output_pattern_finds_every_edit_of_every_unflagged_subrule() {
    let mut grammars = conformance();
    grammars.extend(attack_grammars());
    grammars.push(xml(
        "merge",
        include_str!("../../tests/data/effects-output-merge.xml"),
    ));
    grammars.push(xml(
        "litbundle",
        include_str!("../../tests/data/effects-output-litbundle.xml"),
    ));
    let real = sweep(grammars, &|t| t.output_match_pattern.clone(), &self_flagged);
    show("output pattern vs rule application", &real);
    assert!(
        real.tally.get("edits checked").copied().unwrap_or(0) > 0,
        "vacuous"
    );
    let mut misses = real.misses;
    misses.sort();
    misses.dedup();
    let mut groups: BTreeMap<String, usize> = BTreeMap::new();
    for m in &misses {
        let key = m.split(" word ").next().unwrap_or("").to_string();
        *groups.entry(key).or_default() += 1;
    }
    eprintln!("miss groups: {groups:#?}");
    assert!(
        misses.is_empty(),
        "{} misses, first: {:#?}",
        misses.len(),
        &misses[..misses.len().min(8)]
    );
}

/// The three reviewer repros: a rule whose written segments feed or break its own environment at another site misses with the plain overlay, and is flagged `selfInteraction`.
#[test]
fn a_rule_that_rewrites_its_own_environment_misses_and_is_flagged() {
    let loose = sweep(
        attack_grammars(),
        &|t| t.output_match_pattern.clone(),
        &|_| false,
    );
    show("self-interacting, unflagged rows included", &loose);
    for name in ["selfsim", "selfiter", "insertsim"] {
        assert!(
            loose.misses.iter().any(|m| m.starts_with(name)),
            "{name}: no miss shown"
        );
    }
    for (_, load) in attack_grammars() {
        let g = load();
        let table = g.strata.last().unwrap().table;
        let def = g
            .prules
            .iter()
            .find(|r| matches!(r, PhonRuleDef::Rewrite(r) if r.xml_id == "prX"))
            .expect("rule prX");
        let mut text = rule_text(&g, def, 0, table);
        text.with_alteration(&g, 0, 0);
        assert!(
            text.output_match_pattern.is_some(),
            "{:?}",
            text.output_unprintable
        );
        assert_eq!(text.output_may_be_altered, Some(true));
        assert!(self_flagged(&text), "{:?}", text.output_altered_reasons);
    }
}

/// The mutation: handing `match` the input pattern instead must be caught, or the check proves nothing.
#[test]
fn the_sweep_catches_the_input_pattern_as_the_output_pattern() {
    let mutated = sweep(
        conformance(),
        &|t| t.output_match_pattern.as_ref().and(t.match_pattern.clone()),
        &self_flagged,
    );
    show("mutated", &mutated);
    assert!(
        mutated.misses.len() > 100,
        "the mutated pattern gave only {} misses",
        mutated.misses.len()
    );
}

/// A rule of the first stratum whose output a later stratum's morphology modifies: the engine's surface no longer holds the pattern, and the row says so.
#[test]
fn a_later_stratum_that_modifies_the_input_is_flagged_and_the_engine_changes_the_output() {
    let g = pg_grammar::load(include_str!(
        "../../tests/data/effects-output-latermorph.xml"
    ))
    .expect("loads");
    let def = &g.prules[0];
    let table = g.strata[0].table;
    let mut text = rule_text(&g, def, 0, table);
    text.with_alteration(&g, 0, 0);
    let pattern = text.output_match_pattern.clone().expect("printable");
    let morpher = Morpher::new(&g, 1_000_000);
    let plain = morpher.generate_words(
        pg_grammar::model::LexEntryId(0),
        &[],
        pg_featstruct::FeatureStruct::EMPTY,
    );
    let modified = morpher.generate_words(
        pg_grammar::model::LexEntryId(0),
        &[pg_parse::GenMorpheme::Rule(pg_grammar::model::MRuleId(0))],
        pg_featstruct::FeatureStruct::EMPTY,
    );
    eprintln!("pattern {pattern}: root alone {plain:?}, with the later rule {modified:?}");
    let found = |word: &str| {
        let r = crate::serve_match::handle(
            &g,
            &morpher,
            &json!("m"),
            json!({"pattern": pattern, "words": [word]})
                .as_object()
                .unwrap(),
        );
        !r["words"][0]["matches"].as_array().unwrap().is_empty()
    };
    assert!(found(&plain[0]), "the root alone shows the output pattern");
    assert!(
        !found(&modified[0]),
        "the later stratum's modification removes it"
    );
    assert_eq!(text.output_may_be_altered, Some(true));
    assert!(
        text.output_altered_reasons
            .iter()
            .any(|r| r.0 == "laterMorphology"),
        "{:?}",
        text.output_altered_reasons
    );
}

/// On an XML-loaded grammar every class carries the `Type` pair; merged bundles and a literal overlaid by a bundle still print.
#[test]
fn merged_bundles_print_on_a_loaded_grammar() {
    for (xml_text, want) in [
        (
            include_str!("../../tests/data/effects-output-merge.xml"),
            "[+back +round] / /i/ _",
        ),
        (
            include_str!("../../tests/data/effects-output-litbundle.xml"),
            "[-back +round] / /i/ _",
        ),
    ] {
        let g = pg_grammar::load(xml_text).unwrap();
        let table = g.strata.last().unwrap().table;
        let text = rule_text(&g, &g.prules[0], 0, table);
        eprintln!(
            "{:?} {:?}",
            text.output_match_pattern, text.output_unprintable
        );
        assert_eq!(
            text.output_match_pattern.as_deref(),
            Some(want),
            "{:?}",
            text.output_unprintable
        );
    }
}

/// The second-round attack grammars: every one where `match` of the plain overlay misses a rewritten site is flagged, and the flagged rows are the only ones with misses.
#[test]
fn every_second_round_grammar_that_misses_is_flagged() {
    let mut missing = 0;
    for (name, _) in r2_grammars() {
        let one = |_: ()| {
            r2_grammars()
                .into_iter()
                .filter(|(n, _)| *n == name)
                .collect::<Vec<_>>()
        };
        let loose = sweep(one(()), &|t| t.output_match_pattern.clone(), &|_| false);
        let strict = sweep(one(()), &|t| t.output_match_pattern.clone(), &self_flagged);
        eprintln!(
            "{name}: {} misses unflagged-included, {} after skipping flagged",
            loose.misses.len(),
            strict.misses.len()
        );
        assert!(
            strict.misses.is_empty(),
            "{name}: {:?}",
            &strict.misses[..strict.misses.len().min(3)]
        );
        if !loose.misses.is_empty() {
            missing += 1;
        }
    }
    assert!(
        missing >= 6,
        "only {missing} grammars miss: the attack no longer bites"
    );
}

/// A grammar with no features at all (segments matched by identity): `t -> d / t _`, simultaneous, is flagged, and its plain overlay misses.
#[test]
fn a_featureless_self_interacting_rule_is_flagged() {
    let one = || {
        vec![xml(
            "featureless",
            include_str!("../../tests/data/effects-output-featureless.xml"),
        )]
    };
    let g = pg_grammar::load(include_str!(
        "../../tests/data/effects-output-featureless.xml"
    ))
    .expect("loads");
    assert!(g.phon_features.len() <= 1, "no authored features");
    let table = g.strata[0].table;
    let mut text = rule_text(&g, &g.prules[0], 0, table);
    text.with_alteration(&g, 0, 0);
    assert_eq!(
        text.output_match_pattern.as_deref(),
        Some("/d/ / /t/ _"),
        "{:?}",
        text.output_unprintable
    );
    assert!(self_flagged(&text), "{:?}", text.output_altered_reasons);
    let loose = sweep(one(), &|t| t.output_match_pattern.clone(), &|_| false);
    let strict = sweep(one(), &|t| t.output_match_pattern.clone(), &self_flagged);
    eprintln!(
        "featureless: {} misses unflagged-included, {} after skipping",
        loose.misses.len(),
        strict.misses.len()
    );
    assert!(
        !loose.misses.is_empty(),
        "the rule does not miss: the case proves nothing"
    );
    assert!(strict.misses.is_empty());
}

/// A later rule is a `laterRewrite` only when its target can accept a segment the pattern's nodes accept: one on unrelated segments leaves the row unaltered, one that rewrites the written segment flags it.
#[test]
fn a_later_rule_alters_the_output_only_when_it_touches_its_segments() {
    for (text, altered) in [
        (
            include_str!("../../tests/data/effects-output-later-unrelated.xml"),
            false,
        ),
        (
            include_str!("../../tests/data/effects-output-later-touch.xml"),
            true,
        ),
    ] {
        let g = pg_grammar::load(text).expect("loads");
        let table = g.strata[0].table;
        let mut row = rule_text(&g, &g.prules[0], 0, table);
        row.with_alteration(&g, 0, 0);
        assert_eq!(
            row.output_match_pattern.as_deref(),
            Some("/d/ / /a/ _"),
            "{:?}",
            row.output_unprintable
        );
        assert_eq!(
            row.output_may_be_altered,
            Some(altered),
            "{:?}",
            row.output_altered_reasons
        );
        if altered {
            assert_eq!(
                row.output_altered_reasons,
                vec![("laterRewrite", Some("prY".to_string()))]
            );
        }
    }
}

/// Counts, over every rewrite subrule of the project in `PG_COUNT_FWDATA`, the rows that are altered and self-interacting (run by hand: `-- --ignored --nocapture`).
#[test]
#[ignore = "needs a project path in PG_COUNT_FWDATA"]
fn count_flags_on_a_project() {
    let path = std::env::var("PG_COUNT_FWDATA").expect("PG_COUNT_FWDATA");
    let (g, _) = crate::load_grammar(&path).expect("loads");
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    for (si, stratum) in g.strata.iter().enumerate() {
        for (ordinal, PRuleId(index)) in stratum.prules.iter().enumerate() {
            let Some(def @ PhonRuleDef::Rewrite(rule)) = g.prules.get(*index as usize) else {
                continue;
            };
            for sub in 0..rule.subrules.len() {
                let mut t = rule_text(&g, def, sub, stratum.table);
                t.with_alteration(&g, si, ordinal);
                *tally.entry("rows".into()).or_default() += 1;
                if t.output_match_pattern.is_none() {
                    *tally.entry("no output pattern".into()).or_default() += 1;
                    for (part, reason) in &t.output_unprintable {
                        *tally
                            .entry(format!("unprintable {part} {reason}"))
                            .or_default() += 1;
                    }
                    continue;
                }
                if t.output_may_be_altered == Some(true) {
                    *tally.entry("outputMayBeAltered".into()).or_default() += 1;
                }
                for r in &t.output_altered_reasons {
                    *tally.entry(format!("reason {}", r.0)).or_default() += 1;
                }
                if self_flagged(&t) {
                    *tally.entry("selfInteraction".into()).or_default() += 1;
                }
            }
        }
    }
    eprintln!("COUNTS {tally:?}");
}
