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
