//! Every rewrite rule of every conformance grammar prints as match text that parses back to the same pattern, or is refused with a typed reason.

use std::collections::BTreeMap;

use pg_grammar_model::model::{
    Grammar, NaturalClass, NaturalClassKind, Pattern, PatternNode, PhonRuleDef,
};
use pg_rules::pattern_match::{compile_match, compile_match_engine};
use pg_rules::pattern_text::{boundary_char_def, class_token, parse_pattern, UNPRINTABLE_REASONS};
use pg_rules::rule_text::rule_text;

/// Patterns as strings that two patterns share exactly when they match alike: a class by id or by feature pairs, variables renumbered by first appearance across the patterns canonised in turn.
struct Canon<'a> {
    g: &'a Grammar,
    /// The classes the text defined, when canonising what was read back.
    synthetic: Option<&'a [NaturalClass]>,
    vars: Vec<u16>,
}

impl Canon<'_> {
    fn var(&mut self, id: u16) -> usize {
        self.vars.iter().position(|v| *v == id).unwrap_or_else(|| {
            self.vars.push(id);
            self.vars.len() - 1
        })
    }

    fn class(&self, id: u32, by_token: bool) -> String {
        let n = self.g.natural_classes.len() as u32;
        let (kind, by_token) = match (id < n, self.synthetic) {
            (true, _) => (&self.g.natural_classes[id as usize].kind, by_token),
            (false, Some(extra)) => (&extra[(id - n) as usize].kind, false),
            (false, None) => panic!("class {id} is no grammar class"),
        };
        match kind {
            NaturalClassKind::Feature(pairs) if !by_token => format!("F{pairs:?}"),
            _ => format!("C{id}"),
        }
    }

    fn nodes(&mut self, nodes: &[PatternNode]) -> String {
        let mut out = Vec::new();
        for node in nodes {
            out.push(match node {
                PatternNode::Anchor(s) => format!("{s:?}"),
                PatternNode::CharDef(cd) => format!("cd{}", cd.0),
                PatternNode::Segments { .. } => "segments".to_string(),
                PatternNode::Quantifier { min, max, children } => {
                    format!("q{min}-{max:?}({})", self.nodes(children))
                }
                PatternNode::Context(sc) => {
                    // The text names a grammar class by its token; any other class is read back as a bundle.
                    let by_token = self.synthetic.is_some()
                        || (sc.vars.is_empty() && class_token(self.g, sc.nat_class).is_some());
                    let class = self.class(sc.nat_class.0, by_token);
                    let vars: Vec<String> = sc
                        .vars
                        .iter()
                        .map(|v| format!("{}:{}:{}", v.feature.0, self.var(v.var.0), v.plus))
                        .collect();
                    format!("{class}{vars:?}")
                }
            });
        }
        out.join(" ")
    }
}

fn parts_canon(c: &mut Canon, parts: [&Pattern; 3]) -> String {
    parts
        .iter()
        .map(|p| c.nodes(&p.nodes))
        .collect::<Vec<_>>()
        .join(" | ")
}

#[test]
fn every_rewrite_rule_prints_as_text_that_reads_back_to_the_same_pattern() {
    let mut printed: BTreeMap<&str, usize> = BTreeMap::new();
    let mut refused: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    let mut examples: BTreeMap<String, String> = BTreeMap::new();
    let mut grammars = 0usize;
    let empty = Pattern::default();
    for fixture in
        pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
    {
        let Ok(g) = std::panic::catch_unwind(|| fixture.load_grammar()) else {
            continue;
        };
        grammars += 1;
        let Some(surface) = g.strata.last().map(|s| s.table) else {
            continue;
        };
        let table = &g.char_tables[surface.0 as usize];
        for stratum in &g.strata {
            for rule_id in &stratum.prules {
                let Some(def @ PhonRuleDef::Rewrite(rule)) = g.prules.get(rule_id.0 as usize)
                else {
                    continue;
                };
                for (index, sub) in rule.subrules.iter().enumerate() {
                    let at = format!("{} {} subrule {index}", fixture.label(), rule.xml_id);
                    let text = rule_text(&g, def, index, stratum.table);
                    for (part, reason) in &text.unprintable {
                        assert!(
                            UNPRINTABLE_REASONS.contains(reason),
                            "{at}: undeclared reason {reason}"
                        );
                        *refused.entry(format!("{part}:{reason}")).or_default() += 1;
                        examples
                            .entry(format!("{part}:{reason}"))
                            .or_insert_with(|| at.clone());
                    }
                    let boundary = text
                        .boundary_marker
                        .as_deref()
                        .and_then(|m| boundary_char_def(table, m));
                    if text.boundary_marker.is_some() && boundary.is_none() {
                        failures.push(format!("{at}: boundary marker does not resolve"));
                    }
                    if let Some(pattern) = &text.match_pattern {
                        *printed.entry("matchPattern").or_default() += 1;
                        for (kind, mark) in [
                            ("anchor", "#"),
                            ("boundary", "+"),
                            ("optional", "("),
                            ("star", ")*"),
                            ("alpha", "α"),
                            ("bundle", "[+"),
                            ("emptyTarget", "/ "),
                        ] {
                            if pattern.contains(mark) {
                                *printed.entry(kind).or_default() += 1;
                            }
                        }
                        match parse_pattern(&g, table, pattern, boundary) {
                            Err(e) => {
                                failures.push(format!("{at}: `{pattern}` does not parse: {e:?}"))
                            }
                            Ok(parsed) => {
                                let sides = parsed.environment.clone().expect("an environment");
                                let want = parts_canon(
                                    &mut Canon {
                                        g: &g,
                                        synthetic: None,
                                        vars: Vec::new(),
                                    },
                                    [
                                        &rule.lhs,
                                        sub.left_env.as_ref().unwrap_or(&empty),
                                        sub.right_env.as_ref().unwrap_or(&empty),
                                    ],
                                );
                                let got = parts_canon(
                                    &mut Canon {
                                        g: &g,
                                        synthetic: Some(&parsed.synthetic_classes),
                                        vars: Vec::new(),
                                    },
                                    [
                                        &parsed.target,
                                        sides.left.as_ref().unwrap_or(&empty),
                                        sides.right.as_ref().unwrap_or(&empty),
                                    ],
                                );
                                if want != got {
                                    failures.push(format!(
                                        "{at}: `{pattern}` reads back as {got}, not {want}"
                                    ));
                                }
                                for (name, compile) in [
                                    ("exact", compile_match as fn(_, _, _) -> _),
                                    ("engine", compile_match_engine),
                                ] {
                                    if let Err(e) = compile(&g, surface, &parsed) {
                                        failures.push(format!(
                                            "{at}: `{pattern}` does not compile ({name}): {e:?}"
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    if let Some(rhs) = text.rhs.as_ref().filter(|r| !r.is_empty()) {
                        *printed.entry("rhs").or_default() += 1;
                        match parse_pattern(&g, table, rhs, boundary) {
                            Err(e) => {
                                failures.push(format!("{at}: rhs `{rhs}` does not parse: {e:?}"))
                            }
                            Ok(parsed) => {
                                let want = Canon {
                                    g: &g,
                                    synthetic: None,
                                    vars: Vec::new(),
                                }
                                .nodes(&sub.rhs.nodes);
                                let got = Canon {
                                    g: &g,
                                    synthetic: Some(&parsed.synthetic_classes),
                                    vars: Vec::new(),
                                }
                                .nodes(&parsed.target.nodes);
                                if want != got {
                                    failures.push(format!(
                                        "{at}: rhs `{rhs}` reads back as {got}, not {want}"
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!("rule text: {grammars} grammars, printed {printed:?}, refused {refused:?}, e.g. {examples:#?}");
    assert!(
        failures.is_empty(),
        "{} failures, first: {:#?}",
        failures.len(),
        &failures[..failures.len().min(10)]
    );
    assert!(
        printed.get("matchPattern").copied().unwrap_or(0) > 100,
        "the gate is vacuous"
    );
    for kind in ["anchor", "boundary", "optional"] {
        assert!(
            printed.get(kind).copied().unwrap_or(0) > 0,
            "no {kind} pattern was printed"
        );
    }
}

/// Bundles, alpha letters and repeated groups, which no printable conformance rule uses, built by hand over the first grammar with two features that have a `+` symbol.
#[test]
fn hand_built_bundles_alpha_variables_and_groups_read_back_to_the_same_pattern() {
    use pg_featstruct::SymbolBits;
    use pg_grammar_model::chardef::CharDefKind;
    use pg_grammar_model::featsys::FlatIndex;
    use pg_grammar_model::model::{AlphaVar, AnchorSide, NatClassId, SimpleContext, VarId};
    use pg_rules::pattern_text::{PatternPrinter, Place};
    for fixture in
        pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
    {
        let Ok(mut g) = std::panic::catch_unwind(|| fixture.load_grammar()) else {
            continue;
        };
        let Some(surface) = g.strata.last().map(|s| s.table) else {
            continue;
        };
        let signed: Vec<(FlatIndex, u32)> = g
            .phon_features
            .iter()
            .filter(|(flat, name, _)| *flat != g.phon_features.type_flat() && !name.is_empty())
            .filter_map(|(flat, _, _)| {
                (0..g.phon_features.symbol_count(flat) as u32)
                    .find(|i| g.phon_features.symbol_name(flat, *i) == "+")
                    .map(|i| (flat, i))
            })
            .collect();
        let named = (0..g.natural_classes.len() as u32)
            .map(NatClassId)
            .find(|id| class_token(&g, *id).is_some());
        let segment = g.char_tables[surface.0 as usize]
            .iter()
            .find(|(_, d)| d.kind() == CharDefKind::Segment && d.representations_nfd().len() == 1)
            .map(|(cd, _)| cd);
        let (Some(&(f1, p1)), Some(&(f2, _)), Some(named), Some(segment)) =
            (signed.first(), signed.get(1), named, segment)
        else {
            continue;
        };
        // A class no token reaches (its id holds a `]`), so the text writes its feature pairs as a bundle.
        g.natural_classes
            .push(pg_grammar_model::model::NaturalClass {
                xml_id: "__hand]".into(),
                name: None,
                full_name: None,
                kind: NaturalClassKind::Feature(vec![(f1, SymbolBits(1u64 << p1))]),
            });
        let bundle = NatClassId(g.natural_classes.len() as u32 - 1);
        let ctx = |nat_class, vars| PatternNode::Context(SimpleContext { nat_class, vars });
        let alpha = |var: u16, plus| AlphaVar {
            feature: f2,
            var: VarId(var),
            plus,
        };
        let patterns = [
            vec![ctx(named, vec![])],
            vec![ctx(bundle, vec![])],
            vec![
                ctx(bundle, vec![alpha(7, true)]),
                ctx(bundle, vec![alpha(7, false)]),
            ],
            vec![PatternNode::Quantifier {
                min: 0,
                max: None,
                children: vec![ctx(named, vec![]), PatternNode::CharDef(segment)],
            }],
            vec![PatternNode::Quantifier {
                min: 0,
                max: Some(1),
                children: vec![PatternNode::CharDef(segment)],
            }],
            vec![
                PatternNode::Anchor(AnchorSide::Left),
                PatternNode::CharDef(segment),
                PatternNode::Anchor(AnchorSide::Right),
            ],
        ];
        let table = &g.char_tables[surface.0 as usize];
        for nodes in patterns {
            let pattern = Pattern { nodes };
            let mut printer = PatternPrinter::new(&g, surface, surface, &[&pattern]).unwrap();
            let text = printer.print(&pattern, Place::Plain).unwrap_or_else(|e| {
                panic!("{}: {pattern:?} is not printable: {e:?}", fixture.label())
            });
            let parsed = parse_pattern(&g, table, &text, None)
                .unwrap_or_else(|e| panic!("{}: `{text}` does not parse: {e:?}", fixture.label()));
            let want = Canon {
                g: &g,
                synthetic: None,
                vars: Vec::new(),
            }
            .nodes(&pattern.nodes);
            let got = Canon {
                g: &g,
                synthetic: Some(&parsed.synthetic_classes),
                vars: Vec::new(),
            }
            .nodes(&parsed.target.nodes);
            assert_eq!(got, want, "`{text}` in {}", fixture.label());
            eprintln!("hand-built: `{text}`");
        }
        return;
    }
    panic!("no grammar offers two signed features, a named class and a segment");
}

/// Every rewrite rule's output pattern parses, compiles, keeps the rule's environment and reads back, node by node, as the features the engine's rewrite leaves (lhs overlaid by rhs), or is refused with a typed reason.
#[test]
fn every_output_pattern_reads_back_as_the_features_the_rewrite_leaves() {
    use pg_rules::rewrite::node_full_lanes;
    let mut tally: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    for fixture in
        pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
    {
        let Ok(g) = std::panic::catch_unwind(|| fixture.load_grammar()) else {
            continue;
        };
        let Some(surface) = g.strata.last().map(|s| s.table) else {
            continue;
        };
        let table = &g.char_tables[surface.0 as usize];
        let nfeat = g.phon_features.len();
        for (stratum_index, stratum) in g.strata.iter().enumerate() {
            for (ordinal, rule_id) in stratum.prules.iter().enumerate() {
                let Some(def @ PhonRuleDef::Rewrite(rule)) = g.prules.get(rule_id.0 as usize)
                else {
                    continue;
                };
                for index in 0..rule.subrules.len() {
                    let at = format!("{} {} subrule {index}", fixture.label(), rule.xml_id);
                    let mut text = rule_text(&g, def, index, stratum.table);
                    text.with_alteration(&g, stratum_index, ordinal);
                    for (_, reason) in &text.output_unprintable {
                        assert!(
                            UNPRINTABLE_REASONS.contains(reason),
                            "{at}: undeclared reason {reason}"
                        );
                        *tally.entry(format!("refused:{reason}")).or_default() += 1;
                    }
                    let Some(pattern) = &text.output_match_pattern else {
                        assert!(
                            !text.output_unprintable.is_empty(),
                            "{at}: no output pattern and no reason"
                        );
                        continue;
                    };
                    *tally.entry("printed".into()).or_default() += 1;
                    let altered = text
                        .output_may_be_altered
                        .expect("a printed pattern has an alteration verdict");
                    assert_eq!(altered, !text.output_altered_reasons.is_empty(), "{at}");
                    *tally.entry(format!("mayBeAltered:{altered}")).or_default() += 1;
                    for (kind, mark) in [
                        ("alpha", "α"),
                        ("bundle", "[+"),
                        ("deletion", "/ /"),
                        ("insertion", "/ "),
                    ] {
                        if pattern.contains(mark)
                            || (kind == "deletion" && pattern.starts_with("/ "))
                        {
                            *tally.entry(format!("printed:{kind}")).or_default() += 1;
                        }
                    }
                    let boundary = text
                        .boundary_marker
                        .as_deref()
                        .and_then(|m| boundary_char_def(table, m));
                    let parsed = match parse_pattern(&g, table, pattern, boundary) {
                        Ok(p) => p,
                        Err(e) => {
                            failures.push(format!("{at}: `{pattern}` does not parse: {e:?}"));
                            continue;
                        }
                    };
                    for (name, compile) in [
                        ("exact", compile_match as fn(_, _, _) -> _),
                        ("engine", compile_match_engine),
                    ] {
                        if let Err(e) = compile(&g, surface, &parsed) {
                            failures.push(format!(
                                "{at}: `{pattern}` does not compile ({name}): {e:?}"
                            ));
                        }
                    }
                    if !text
                        .environment
                        .as_deref()
                        .is_some_and(|env| pattern.ends_with(env))
                    {
                        failures.push(format!(
                            "{at}: `{pattern}` does not end in the rule's environment"
                        ));
                    }
                    if parsed.target.nodes.len() != text.output_target_len {
                        failures.push(format!(
                            "{at}: `{pattern}` has {} target nodes, the rewrite leaves {}",
                            parsed.target.nodes.len(),
                            text.output_target_len
                        ));
                        continue;
                    }
                    for (i, node) in parsed.target.nodes.iter().enumerate() {
                        let lanes = match node {
                            PatternNode::Context(sc)
                                if sc.nat_class.0 as usize >= g.natural_classes.len() =>
                            {
                                let extra = &parsed.synthetic_classes
                                    [sc.nat_class.0 as usize - g.natural_classes.len()];
                                let mut lanes = text.output_covered[i].clone();
                                for (f, lane) in lanes.iter_mut().enumerate().take(nfeat) {
                                    *lane = pg_grammar_model::membership::mask(&g.phon_features, f);
                                }
                                if let NaturalClassKind::Feature(pairs) = &extra.kind {
                                    for (f, bits) in pairs {
                                        lanes[f.0 as usize] = bits.0;
                                    }
                                }
                                lanes
                            }
                            node => node_full_lanes(&g, table, node),
                        };
                        // Alpha features are not pinned by either side.
                        let vars: Vec<usize> = match node {
                            PatternNode::Context(sc) => {
                                sc.vars.iter().map(|v| v.feature.0 as usize).collect()
                            }
                            _ => Vec::new(),
                        };
                        for f in (0..nfeat).filter(|f| {
                            !vars.contains(f) && *f != g.phon_features.type_flat().0 as usize
                        }) {
                            if lanes[f] != text.output_covered[i][f] {
                                failures.push(format!(
                                    "{at}: `{pattern}` node {i} feature {f} reads back as {:#x}, the rewrite leaves {:#x}",
                                    lanes[f], text.output_covered[i][f]
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!("output patterns: {tally:?}");
    assert!(
        failures.is_empty(),
        "{} failures, first: {:#?}",
        failures.len(),
        &failures[..failures.len().min(10)]
    );
    assert!(
        tally.get("printed").copied().unwrap_or(0) > 50,
        "the gate is vacuous"
    );
}

/// Merged bundles and alpha variables, which no printable conformance rule produces, built by hand on a printable rule of the first grammar that has two signed features.
#[test]
fn hand_built_rewrites_print_their_output_and_refuse_what_they_cannot_say() {
    use pg_featstruct::SymbolBits;
    use pg_grammar_model::featsys::FlatIndex;
    use pg_grammar_model::model::{AlphaVar, NatClassId, SimpleContext, VarId};
    for fixture in
        pg_conformance_fixtures::discover_scoped(pg_conformance_fixtures::ConformanceScope::All)
    {
        let Ok(mut g) = std::panic::catch_unwind(|| fixture.load_grammar()) else {
            continue;
        };
        let Some(surface) = g.strata.last().map(|s| s.table) else {
            continue;
        };
        let signed: Vec<(FlatIndex, u32)> = g
            .phon_features
            .iter()
            .filter(|(flat, name, _)| *flat != g.phon_features.type_flat() && !name.is_empty())
            .filter_map(|(flat, _, _)| {
                (0..g.phon_features.symbol_count(flat) as u32)
                    .find(|i| g.phon_features.symbol_name(flat, *i) == "+")
                    .map(|i| (flat, i))
            })
            .collect();
        let Some(slot) = g.strata.last().and_then(|s| {
            s.prules
                .iter()
                .find_map(|id| match g.prules.get(id.0 as usize) {
                    Some(PhonRuleDef::Rewrite(r)) if !r.subrules.is_empty() => Some(id.0 as usize),
                    _ => None,
                })
        }) else {
            continue;
        };
        let (Some(&(f1, p1)), Some(&(f2, p2))) = (signed.first(), signed.get(1)) else {
            continue;
        };
        let mut class = |pairs: Vec<(FlatIndex, SymbolBits)>| {
            g.natural_classes
                .push(pg_grammar_model::model::NaturalClass {
                    xml_id: "__hand]".into(),
                    name: None,
                    full_name: None,
                    kind: NaturalClassKind::Feature(pairs),
                });
            NatClassId(g.natural_classes.len() as u32 - 1)
        };
        let (c1, c2, empty_class) = (
            class(vec![(f1, SymbolBits(1u64 << p1))]),
            class(vec![(f2, SymbolBits(1u64 << p2))]),
            class(vec![]),
        );
        let ctx = |nat_class, vars| PatternNode::Context(SimpleContext { nat_class, vars });
        let alpha = |plus| AlphaVar {
            feature: f2,
            var: VarId(7),
            plus,
        };
        let (n1, n2) = (
            g.phon_features.feature_name(f1),
            g.phon_features.feature_name(f2),
        );
        type Case = (
            &'static str,
            Vec<PatternNode>,
            Vec<PatternNode>,
            Vec<PatternNode>,
            Result<String, &'static str>,
        );
        let cases: [Case; 4] = [
            // The target keeps its first feature and gains the second.
            (
                "merge",
                vec![ctx(c1, vec![])],
                vec![ctx(c2, vec![])],
                vec![],
                Ok(format!("[+{n1} +{n2}] / _")),
            ),
            // The variable the environment binds survives in the output beside the target's own feature.
            (
                "alpha",
                vec![ctx(c1, vec![])],
                vec![ctx(empty_class, vec![alpha(false)])],
                vec![ctx(empty_class, vec![alpha(true)])],
                Ok(format!("[+{n1} -α{n2}] / [α{n2}] _")),
            ),
            // The variable has no binding in the environment, so the output cannot say it.
            (
                "unbound alpha",
                vec![ctx(c1, vec![])],
                vec![ctx(empty_class, vec![alpha(true)])],
                vec![],
                Err("alpha_not_in_environment"),
            ),
            // A deletion leaves only the environment.
            (
                "deletion",
                vec![ctx(c1, vec![])],
                vec![],
                vec![ctx(c2, vec![])],
                Ok(format!("/ [+{n2}] _")),
            ),
        ];
        for (label, lhs, rhs, left, want) in cases {
            let Some(PhonRuleDef::Rewrite(r)) = g.prules.get_mut(slot) else {
                unreachable!()
            };
            r.lhs = Pattern { nodes: lhs };
            r.subrules[0].rhs = Pattern { nodes: rhs };
            r.subrules[0].left_env = (!left.is_empty()).then_some(Pattern { nodes: left });
            r.subrules[0].right_env = None;
            let stratum_table = g.strata.last().unwrap().table;
            assert_eq!(stratum_table, surface);
            let text = rule_text(&g, &g.prules[slot], 0, stratum_table);
            match want {
                Ok(pattern) => {
                    // The deletion's environment is the left side only: `_` follows it.
                    let pattern = if label == "deletion" {
                        format!("/ [+{n2}] _")
                    } else {
                        pattern
                    };
                    assert_eq!(
                        text.output_match_pattern.as_deref(),
                        Some(pattern.as_str()),
                        "{label} in {}: {:?}",
                        fixture.label(),
                        text.output_unprintable
                    );
                }
                Err(reason) => {
                    assert_eq!(text.output_match_pattern, None, "{label}");
                    assert!(
                        text.output_unprintable
                            .contains(&("outputMatchPattern", reason)),
                        "{label}: {:?}",
                        text.output_unprintable
                    );
                }
            }
        }
        return;
    }
    panic!("no grammar offers two signed features and a rewrite rule");
}
