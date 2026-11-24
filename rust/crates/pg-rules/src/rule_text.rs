//! A phonological rule as the text serve `match` reads: `lhs`, `environment`, the combined `target / left _ right` and `rhs`, each printed exactly or refused with a typed reason, plus the reasons a rule is not local.

use pg_grammar_model::chardef::CharDefKind;
use pg_grammar_model::model::{
    Grammar, NaturalClassKind, Pattern, PatternNode, PhonRuleDef, RewriteMode, TableId,
};

use crate::pattern_match::compile_match_as_engine;
use crate::pattern_text::{PatternPrinter, Place, Unprintable};
use crate::shape_feat::segment_with_features_load as segment_with_features;

/// The `nonLocal` reasons, in the order they are reported.
pub const NON_LOCAL_CRITERIA: [&str; 4] = [
    "unboundedQuantifier",
    "alphaAgreement",
    "iterativeSpreading",
    "iterativeLengthChange",
];

/// One rule row (a rewrite subrule, or a whole metathesis rule) as match text.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RuleText {
    /// The target pattern; empty for an insertion.
    pub lhs: Option<String>,
    /// `left _ right`, the right-hand part of a `match` environment.
    pub environment: Option<String>,
    /// `lhs / left _ right`, the pattern text to hand `match` (the plain `lhs` for a metathesis rule).
    pub match_pattern: Option<String>,
    /// The replacement pattern; empty for a deletion.
    pub rhs: Option<String>,
    /// The text `+` stands for, when a printed part has a boundary; `match` needs it as `boundaryMarker`.
    pub boundary_marker: Option<String>,
    /// Each part with no exact text: `(part, reason)`.
    pub unprintable: Vec<(&'static str, &'static str)>,
    /// The criteria of [`NON_LOCAL_CRITERIA`] the rule meets; any one makes the rule non-local.
    pub non_local: Vec<&'static str>,
}

/// The text of subrule `subrule` of `rule`, which runs in a stratum whose character table is `stratum_table`; the text is read against the last stratum's (surface) table.
pub fn rule_text(
    g: &Grammar,
    rule: &PhonRuleDef,
    subrule: usize,
    stratum_table: TableId,
) -> RuleText {
    let mut out = RuleText::default();
    let empty = Pattern::default();
    let (lhs, sub) = match rule {
        PhonRuleDef::Rewrite(r) => (&r.lhs, r.subrules.get(subrule)),
        PhonRuleDef::Metathesis(r) => (&r.pattern, None),
    };
    out.non_local = non_local(g, rule, subrule, stratum_table);
    let parts: Vec<&Pattern> = std::iter::once(lhs)
        .chain(sub.iter().flat_map(|s| {
            [Some(&s.rhs), s.left_env.as_ref(), s.right_env.as_ref()]
                .into_iter()
                .flatten()
        }))
        .collect();
    let printer = g
        .strata
        .last()
        .filter(|s| s.table == stratum_table)
        .and_then(|s| PatternPrinter::new(g, stratum_table, s.table, &parts));
    let Some(mut printer) = printer else {
        let reason = "stratum_table_differs";
        out.unprintable.push(("lhs", reason));
        if matches!(rule, PhonRuleDef::Rewrite(_)) {
            out.unprintable
                .extend([("environment", reason), ("rhs", reason)]);
        }
        return out;
    };
    let put = |part: &'static str, result: Result<String, Unprintable>, errs: &mut Vec<_>| {
        result.map_err(|e| errs.push((part, e.0))).ok()
    };
    out.lhs = put(
        "lhs",
        printer.print(lhs, Place::Plain),
        &mut out.unprintable,
    );
    let Some(sub) = sub else {
        if matches!(rule, PhonRuleDef::Metathesis(_)) {
            out.match_pattern = out.lhs.clone();
            out.unprintable
                .extend([("environment", "metathesis"), ("rhs", "metathesis")]);
        }
        out.boundary_marker = printer.boundary_marker().map(str::to_string);
        return out;
    };
    out.rhs = put(
        "rhs",
        printer.print(&sub.rhs, Place::Plain),
        &mut out.unprintable,
    );
    let left = printer.print(sub.left_env.as_ref().unwrap_or(&empty), Place::Left);
    let right = printer.print(sub.right_env.as_ref().unwrap_or(&empty), Place::Right);
    let target = printer.print(lhs, Place::Target);
    let env = match (left, right) {
        (Ok(l), Ok(r)) => {
            let words: Vec<&str> = [l.as_str(), "_", r.as_str()]
                .into_iter()
                .filter(|w| !w.is_empty())
                .collect();
            Some(words.join(" "))
        }
        (l, r) => {
            for e in [l.err(), r.err()].into_iter().flatten() {
                out.unprintable.push(("environment", e.0));
            }
            None
        }
    };
    out.environment = env.clone();
    match (env, target) {
        (Some(env), Ok(t)) => {
            out.match_pattern = Some(if t.is_empty() {
                format!("/ {env}")
            } else {
                format!("{t} / {env}")
            });
        }
        (Some(_), Err(e)) => out.unprintable.push(("matchPattern", e.0)),
        (None, _) => {}
    }
    out.boundary_marker = printer.boundary_marker().map(str::to_string);
    out
}

fn any_node(nodes: &[PatternNode], test: &dyn Fn(&PatternNode) -> bool) -> bool {
    nodes.iter().any(|n| {
        test(n) || matches!(n, PatternNode::Quantifier { children, .. } if any_node(children, test))
    })
}

/// The features a node constrains, or `None` when it constrains every one (a segment, a segment list).
fn lanes(g: &Grammar, node: &PatternNode) -> Option<Vec<u32>> {
    match node {
        PatternNode::Context(sc) => {
            let class = g.natural_classes.get(sc.nat_class.0 as usize)?;
            let NaturalClassKind::Feature(pairs) = &class.kind else {
                return None;
            };
            Some(
                pairs
                    .iter()
                    .map(|(f, _)| f.0)
                    .chain(sc.vars.iter().map(|v| v.feature.0))
                    .collect(),
            )
        }
        _ => None,
    }
}

fn is_boundary(g: &Grammar, node: &PatternNode, table: TableId) -> bool {
    matches!(node, PatternNode::CharDef(cd)
        if g.char_tables[table.0 as usize].get(*cd).kind() == CharDefKind::Boundary)
}

/// The criteria of [`NON_LOCAL_CRITERIA`] that rule row `subrule` meets: a group that repeats without bound, an alpha variable, an iterative rewrite whose environment can see the segments the rule rewrites, or an iterative rewrite that changes length.
pub fn non_local(
    g: &Grammar,
    rule: &PhonRuleDef,
    subrule: usize,
    table: TableId,
) -> Vec<&'static str> {
    let mut parts: Vec<&Pattern> = Vec::new();
    match rule {
        PhonRuleDef::Rewrite(r) => {
            parts.push(&r.lhs);
            if let Some(s) = r.subrules.get(subrule) {
                parts.extend(
                    [Some(&s.rhs), s.left_env.as_ref(), s.right_env.as_ref()]
                        .into_iter()
                        .flatten(),
                );
            }
        }
        PhonRuleDef::Metathesis(r) => parts.push(&r.pattern),
    }
    let mut out = Vec::new();
    let unbounded = |n: &PatternNode| matches!(n, PatternNode::Quantifier { max: None, .. });
    let alpha = |n: &PatternNode| matches!(n, PatternNode::Context(sc) if !sc.vars.is_empty());
    if parts.iter().any(|p| any_node(&p.nodes, &unbounded)) {
        out.push(NON_LOCAL_CRITERIA[0]);
    }
    if parts.iter().any(|p| any_node(&p.nodes, &alpha)) {
        out.push(NON_LOCAL_CRITERIA[1]);
    }
    if iterative_spreading(g, rule, table) {
        out.push(NON_LOCAL_CRITERIA[2]);
    }
    if iterative_length_change(rule) {
        out.push(NON_LOCAL_CRITERIA[3]);
    }
    out
}

/// Whether an iterative rewrite has a subrule whose replacement and left side differ in length (deletion, expansion, insertion), so later sites see shifted material.
fn iterative_length_change(rule: &PhonRuleDef) -> bool {
    matches!(rule, PhonRuleDef::Rewrite(r)
        if r.mode == RewriteMode::Iterative
            && r.subrules.iter().any(|s| s.rhs.nodes.len() != r.lhs.nodes.len()))
}

/// Whether an iterative rewrite spreads: the rule is iterative, a subrule's left side and replacement have as many nodes, and an environment node of any subrule (one subrule feeds the next at the same site) can match a segment a replacement writes. A literal segment is tested with the engine's own matcher, a feature change by the features it writes.
fn iterative_spreading(g: &Grammar, rule: &PhonRuleDef, table: TableId) -> bool {
    let PhonRuleDef::Rewrite(r) = rule else {
        return false;
    };
    if r.mode != RewriteMode::Iterative || r.lhs.nodes.is_empty() {
        return false;
    }
    let chardefs = &g.char_tables[table.0 as usize];
    // Whether the env node can match what the replacement node writes.
    let sees = |written: &PatternNode, seen: &PatternNode| -> bool {
        match written {
            PatternNode::CharDef(cd) => {
                let shape = chardefs
                    .get(*cd)
                    .representations_nfd()
                    .iter()
                    .find_map(|rep| segment_with_features(g, chardefs, rep).ok());
                let probe = Pattern {
                    nodes: vec![seen.clone()],
                };
                match (shape, compile_match_as_engine(g, table, &probe)) {
                    (Some(shape), Ok(compiled)) => !compiled.matches(g, &shape).is_empty(),
                    _ => true,
                }
            }
            written => match (lanes(g, written), lanes(g, seen)) {
                (Some(w), Some(l)) => l.iter().any(|f| w.contains(f)),
                _ => true,
            },
        }
    };
    let reaches = |written: &Pattern, n: &PatternNode| match n {
        PatternNode::Anchor(_) | PatternNode::Quantifier { .. } => false,
        n if is_boundary(g, n, table) => false,
        n => written.nodes.iter().any(|w| sees(w, n)),
    };
    let writers = r
        .subrules
        .iter()
        .filter(|s| s.rhs.nodes.len() == r.lhs.nodes.len());
    writers.into_iter().any(|w| {
        r.subrules.iter().any(|s| {
            [s.left_env.as_ref(), s.right_env.as_ref()]
                .into_iter()
                .flatten()
                .any(|p| any_node(&p.nodes, &|n| reaches(&w.rhs, n)))
        })
    })
}
