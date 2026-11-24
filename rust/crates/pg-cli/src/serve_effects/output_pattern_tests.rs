//! Soundness of `outputMatchPattern`: where a rule alone rewrote a word, `match` of the output pattern on the output word reports a span there.

use std::collections::BTreeMap;

use pg_grammar::chardef::CharDefKind;
use pg_grammar::model::{PRuleId, PhonRuleDef};
use pg_parse::Morpher;
use pg_rules::rule_text::{rule_text, RuleText};
use pg_rules::shape_feat::segment_with_features_load as segment_with_features;
use pg_rules::surface_probe::render_shape_marked;
use serde_json::json;

/// What one run over the fixtures found.
#[derive(Default)]
struct Sweep {
    tally: BTreeMap<String, usize>,
    misses: Vec<String>,
}

fn is_boundary(kind: impl std::fmt::Debug) -> bool {
    format!("{kind:?}") == "Boundary"
}

/// Applies each printable, local rule alone to generated words and checks `pattern_of(text)` against the output words; every rewritten position (a segment whose features changed, or the edit point of a length change) must lie in a reported span.
fn sweep(pattern_of: &dyn Fn(&RuleText) -> Option<String>) -> Sweep {
    let mut out = Sweep::default();
    let mut tally = |key: &str, n: usize| *out.tally.entry(key.to_string()).or_default() += n;
    let mut misses = Vec::new();
    for fixture in
        pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
    {
        let Ok(g) = std::panic::catch_unwind(|| fixture.load_grammar()) else {
            continue;
        };
        let Some(last) = g.strata.last().map(|s| s.table) else {
            continue;
        };
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
        for stratum in &g.strata {
            for PRuleId(index) in &stratum.prules {
                let Some(def @ PhonRuleDef::Rewrite(rule)) = g.prules.get(*index as usize) else {
                    continue;
                };
                if stratum.table != last {
                    tally("rules of another stratum table", 1);
                    continue;
                }
                let texts: Vec<RuleText> = (0..rule.subrules.len())
                    .map(|i| rule_text(&g, def, i, stratum.table))
                    .collect();
                if texts.is_empty() || texts.iter().any(|t| pattern_of(t).is_none()) {
                    tally("rules with no output pattern", 1);
                    continue;
                }
                if texts.iter().any(|t| !t.non_local.is_empty()) {
                    tally("rules flagged non-local", 1);
                    continue;
                }
                let markers: Vec<&str> = texts
                    .iter()
                    .filter_map(|t| t.boundary_marker.as_deref())
                    .collect();
                if markers.windows(2).any(|w| w[0] != w[1]) {
                    tally("rules with two boundary markers", 1);
                    continue;
                }
                tally("rules checked", 1);
                // Per word: the output text and, for each edit, the segment ranges it may be placed at.
                type Edit = (String, Vec<Vec<(usize, usize)>>);
                let mut edits: Vec<Edit> = Vec::new();
                for separator in &separators {
                    if markers
                        .first()
                        .is_some_and(|m| !separator.is_empty() && separator != m)
                    {
                        continue;
                    }
                    for left in &sides {
                        for focus in &foci {
                            for right in &sides {
                                let word = format!("{left}{separator}{focus}{separator}{right}");
                                let Ok(input) = segment_with_features(&g, table, &word) else {
                                    continue;
                                };
                                let Ok(outputs) =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        pg_rules::rewrite::synthesize(&g, rule, &input)
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
                                let Some(text) =
                                    render_shape_marked(table, output, separator.as_str())
                                else {
                                    tally("outputs with no spelling", 1);
                                    continue;
                                };
                                // Segment index of node position `p`.
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
                                    let prefix =
                                        inn.iter().zip(&outn).take_while(|(a, b)| a == b).count();
                                    let suffix = inn[prefix..]
                                        .iter()
                                        .rev()
                                        .zip(outn[prefix..].iter().rev())
                                        .take_while(|(a, b)| a == b)
                                        .count();
                                    let (ri, ro) =
                                        (inn.len() - prefix - suffix, outn.len() - prefix - suffix);
                                    let widest = rule
                                        .subrules
                                        .iter()
                                        .map(|s| s.rhs.nodes.len())
                                        .max()
                                        .unwrap_or(0);
                                    if ri > rule.lhs.nodes.len() || ro > widest {
                                        tally("length changes of more than one site left out", 1);
                                        continue;
                                    }
                                    // A pure insertion or deletion next to an equal node can sit one place either way; every such place gives the same word.
                                    let (block, width) =
                                        if ro > 0 { (&outn, ro) } else { (&inn, ri) };
                                    let (mut lo, mut hi) = (prefix, prefix);
                                    if ri == 0 || ro == 0 {
                                        while lo > 0 && block[lo - 1] == block[lo + width - 1] {
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
                                            let a = seg_at(block, q);
                                            if ro > 0 {
                                                (a, seg_at(block, q + width))
                                            } else {
                                                (a, a)
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
                                edits.push((text, checks));
                            }
                        }
                    }
                }
                tally("words the rule rewrites", edits.len());
                let mut request = json!({
                    "pattern": null,
                    "words": edits.iter().map(|e| e.0.as_str()).collect::<Vec<_>>(),
                });
                if let Some(m) = markers.first().copied().or(marker.as_deref()) {
                    request["boundaryMarker"] = json!(m);
                }
                let mut spans_found: Vec<Vec<(usize, usize)>> = vec![Vec::new(); edits.len()];
                let mut skipped = vec![false; edits.len()];
                for text in &texts {
                    request["pattern"] = json!(pattern_of(text));
                    let r = crate::serve_match::handle(
                        &g,
                        &morpher,
                        &json!("m"),
                        request.as_object().unwrap(),
                    );
                    if r["type"] != "result" {
                        misses.push(format!(
                            "{} {}: match refused `{:?}`: {r}",
                            fixture.label(),
                            rule.xml_id,
                            pattern_of(text)
                        ));
                        continue;
                    }
                    for (i, word) in r["words"].as_array().unwrap().iter().enumerate() {
                        if word["status"] != "ok" {
                            skipped[i] = true;
                        }
                        for m in word["matches"].as_array().unwrap() {
                            let seg = &m["segments"];
                            spans_found[i].push((
                                seg[0].as_u64().unwrap() as usize,
                                seg[1].as_u64().unwrap() as usize,
                            ));
                        }
                    }
                }
                for ((edit, found), skipped) in edits.iter().zip(&spans_found).zip(&skipped) {
                    if *skipped {
                        tally("words match cannot read", 1);
                        continue;
                    }
                    for places in &edit.1 {
                        tally("rewritten places checked", 1);
                        let hit = places.iter().any(|(a, b)| {
                            if a == b {
                                found.iter().any(|(x, y)| x == a && y == a)
                            } else {
                                (*a..*b).all(|p| found.iter().any(|(x, y)| *x <= p && p < *y))
                            }
                        });
                        if !hit {
                            misses.push(format!(
                                "{} {} word `{}` places {places:?}: patterns {:?}, spans {found:?}",
                                fixture.label(),
                                rule.xml_id,
                                edit.0,
                                texts.iter().map(pattern_of).collect::<Vec<_>>()
                            ));
                        }
                    }
                }
            }
        }
    }
    out.misses = misses;
    out
}

#[test]
fn match_on_the_output_pattern_finds_every_position_a_rule_alone_rewrote() {
    let real = sweep(&|t| t.output_match_pattern.clone());
    eprintln!(
        "output pattern vs rule application: {:?}, {} misses",
        real.tally,
        real.misses.len()
    );
    let checked = real
        .tally
        .get("rewritten places checked")
        .copied()
        .unwrap_or(0);
    assert!(
        checked > 0,
        "no rewritten place was checked: the test is vacuous"
    );
    let mut misses = real.misses;
    misses.sort();
    misses.dedup();
    assert!(
        misses.is_empty(),
        "{} misses, first: {:#?}",
        misses.len(),
        &misses[..misses.len().min(8)]
    );
}

/// The mutation: handing `match` the input pattern instead (the target before the rewrite) must be caught, or the check above proves nothing.
#[test]
fn the_sweep_catches_the_input_pattern_as_the_output_pattern() {
    let mutated = sweep(&|t| t.output_match_pattern.as_ref().and(t.match_pattern.clone()));
    eprintln!(
        "mutated: {:?}, {} misses",
        mutated.tally,
        mutated.misses.len()
    );
    assert!(
        mutated.misses.len() > 100,
        "the mutated pattern gave only {} misses: the sweep cannot see a wrong pattern",
        mutated.misses.len()
    );
}
