//! A phonological rule as the text serve `match` reads: `lhs`, `environment`, the combined `target / left _ right` and `rhs`, each printed exactly or refused with a typed reason, plus the reasons a rule is not local.

use pg_featstruct::SymbolBits;
use pg_grammar_model::chardef::{CharDefKind, CharDefTable};
use pg_grammar_model::featsys::FlatIndex;
use pg_grammar_model::model::{
    AlphaVar, Grammar, MRuleId, MorphRuleDef, NaturalClassKind, OutputAction, Pattern, PatternNode,
    PhonRuleDef, RewriteMode, RewriteRuleDef, RewriteSubruleDef, TableId, VarId,
};

use crate::pattern_match::compile_match_as_engine;
use crate::pattern_text::{PatternPrinter, Place, Unprintable};
use crate::rewrite::node_pins;
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
    /// Where the rule's output sits right after this rule applies: the environment with the target replaced by what the rule writes, as `match` text; `None` when `output_unprintable` says why not.
    pub output_match_pattern: Option<String>,
    /// Why there is no `output_match_pattern`: `(part, reason)`.
    pub output_unprintable: Vec<(&'static str, &'static str)>,
    /// Whether a later rule, or the rule itself, can rewrite what the output pattern covers; `None` until [`RuleText::with_alteration`] runs or when there is no output pattern.
    pub output_may_be_altered: Option<bool>,
    /// Why `output_may_be_altered`: `(reason, rule id)`.
    pub output_altered_reasons: Vec<(&'static str, Option<String>)>,
    /// The match constraint of each node the output pattern covers (full lane vectors): the first `output_target_len` are the target's, the rest the environment's.
    pub output_covered: Vec<Vec<u64>>,
    /// How many of `output_covered` are the output target's nodes.
    pub output_target_len: usize,
    /// Whether the output pattern holds a word edge.
    output_has_edge: bool,
    /// Whether the rule's own written or rewritten segments could satisfy or overlap its own environment, or an insertion or length change has a side of two or more nodes: one site's edit can then change another site's environment.
    pub output_self_interaction: bool,
}

/// The `outputMayBeAltered` reasons, as `(reason)` codes.
pub const OUTPUT_ALTERED_REASONS: [&str; 6] = [
    "nonLocal",
    "laterRewrite",
    "laterInsertion",
    "wordEdgeLaterStratum",
    "selfInteraction",
    "laterMorphology",
];

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
        out.output_unprintable.push(("outputMatchPattern", reason));
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
            out.output_unprintable
                .push(("outputMatchPattern", "metathesis"));
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
    if let PhonRuleDef::Rewrite(r) = rule {
        match (
            &out.environment,
            output_target(g, &mut printer, stratum_table, r, sub),
        ) {
            (Some(env), Ok((target, covered))) => {
                out.output_match_pattern = Some(if target.is_empty() {
                    format!("/ {env}")
                } else {
                    format!("{target} / {env}")
                });
                out.output_target_len = covered.len();
                out.output_covered = covered;
                out.output_covered.extend(env_lanes(g, stratum_table, sub));
                out.output_has_edge = [&r.lhs]
                    .into_iter()
                    .chain(sub.left_env.as_ref())
                    .chain(sub.right_env.as_ref())
                    .any(|p| p.nodes.iter().any(|n| matches!(n, PatternNode::Anchor(_))));
            }
            (None, _) => out
                .output_unprintable
                .push(("outputMatchPattern", "environment_not_printed")),
            (_, Err(e)) => out.output_unprintable.push(("outputMatchPattern", e.0)),
        }
    }
    if let PhonRuleDef::Rewrite(r) = rule {
        out.output_self_interaction = self_interaction(g, stratum_table, r);
    }
    out.boundary_marker = printer.boundary_marker().map(str::to_string);
    out
}

/// Whether what the rule rewrites or writes (any subrule: they share one pass) could satisfy a node of any subrule's environment, or an insertion or length change has an environment side of two or more nodes. Conservative: lane overlap only.
fn self_interaction(g: &Grammar, table: TableId, rule: &RewriteRuleDef) -> bool {
    let chardefs = &g.char_tables[table.0 as usize];
    let mut env = Vec::new();
    let mut written: Vec<Vec<u64>> = Vec::new();
    let mut flat = Vec::new();
    flatten(&rule.lhs.nodes, &mut flat);
    written.extend(
        flat.iter()
            .map(|n| crate::rewrite::node_full_lanes(g, chardefs, n)),
    );
    let mut wide = false;
    for sub in &rule.subrules {
        let mut nodes = Vec::new();
        flatten(&sub.rhs.nodes, &mut nodes);
        written.extend(
            nodes
                .iter()
                .map(|n| crate::rewrite::node_full_lanes(g, chardefs, n)),
        );
        let mut sides = [Vec::new(), Vec::new()];
        for (side, p) in sides
            .iter_mut()
            .zip([sub.left_env.as_ref(), sub.right_env.as_ref()])
        {
            if let Some(p) = p {
                flatten(&p.nodes, side);
            }
        }
        wide |= sides.iter().any(|s| s.len() >= 2);
        env.extend(
            sides
                .iter()
                .flatten()
                .map(|n| crate::rewrite::node_full_lanes(g, chardefs, n)),
        );
        let length_change = rule.lhs.nodes.len() != sub.rhs.nodes.len();
        if length_change && wide {
            return true;
        }
    }
    let meets = |a: &Vec<u64>, b: &Vec<u64>| a.iter().zip(b).all(|(x, y)| x & y != 0);
    // Sites of a target of two or more nodes can overlap one another.
    let lhs: Vec<Vec<u64>> = flat
        .iter()
        .map(|n| crate::rewrite::node_full_lanes(g, chardefs, n))
        .collect();
    let overlapping = lhs
        .iter()
        .enumerate()
        .any(|(i, a)| lhs.iter().skip(i + 1).any(|b| meets(a, b)));
    overlapping || written.iter().any(|w| env.iter().any(|e| meets(w, e)))
}

impl RuleText {
    /// Fills `output_may_be_altered`: the rule is non-local, or a rule that runs later (after it in its stratum, or in a later stratum) can rewrite or split what the output pattern covers. Conservative: a later rule counts when its target can match any covered node.
    pub fn with_alteration(&mut self, g: &Grammar, stratum: usize, ordinal: usize) {
        if self.output_match_pattern.is_none() {
            return;
        }
        let mut reasons: Vec<(&'static str, Option<String>)> = Vec::new();
        if !self.non_local.is_empty() {
            reasons.push((OUTPUT_ALTERED_REASONS[0], None));
        }
        if self.output_has_edge && stratum + 1 < g.strata.len() {
            reasons.push((OUTPUT_ALTERED_REASONS[3], None));
        }
        if self.output_self_interaction {
            let own = g
                .strata
                .get(stratum)
                .and_then(|s| s.prules.get(ordinal))
                .and_then(|p| g.prules.get(p.0 as usize))
                .map(|r| match r {
                    PhonRuleDef::Rewrite(r) => r.xml_id.clone(),
                    PhonRuleDef::Metathesis(r) => r.xml_id.clone(),
                });
            reasons.push((OUTPUT_ALTERED_REASONS[4], own));
        }
        for st in g.strata.iter().skip(stratum + 1) {
            let mut rules: Vec<MRuleId> = st.mrules.clone();
            for t in &st.templates {
                if let Some(t) = g.templates.get(t.0 as usize) {
                    rules.extend(t.slots.iter().flat_map(|slot| slot.rules.iter().copied()));
                }
            }
            for id in rules {
                if let Some(rule) = g.mrules.get(id.0 as usize) {
                    if !plain_affix(rule) {
                        reasons.push((OUTPUT_ALTERED_REASONS[5], None));
                        break;
                    }
                }
            }
        }
        for (si, st) in g.strata.iter().enumerate().skip(stratum) {
            for (oi, pid) in st.prules.iter().enumerate() {
                let index = pid.0;
                if si == stratum && oi <= ordinal {
                    continue;
                }
                let Some(rule) = g.prules.get(index as usize) else {
                    continue;
                };
                let (id, targets): (&str, &Pattern) = match rule {
                    PhonRuleDef::Rewrite(r) => (&r.xml_id, &r.lhs),
                    PhonRuleDef::Metathesis(r) => (&r.xml_id, &r.pattern),
                };
                let mut nodes = Vec::new();
                flatten(&targets.nodes, &mut nodes);
                if nodes.is_empty() {
                    if self.output_covered.len() >= 2 || self.output_has_edge {
                        reasons.push((OUTPUT_ALTERED_REASONS[2], Some(id.to_string())));
                    }
                    continue;
                }
                let hits = nodes.iter().any(|n| {
                    let lanes =
                        crate::rewrite::node_full_lanes(g, &g.char_tables[st.table.0 as usize], n);
                    self.output_covered
                        .iter()
                        .any(|c| c.iter().zip(&lanes).all(|(a, b)| a & b != 0))
                });
                if hits {
                    reasons.push((OUTPUT_ALTERED_REASONS[1], Some(id.to_string())));
                }
            }
        }
        reasons.dedup();
        self.output_may_be_altered = Some(!reasons.is_empty());
        self.output_altered_reasons = reasons;
    }
}

/// Whether a morphological rule only adds a prefix or a suffix to its copied input: one input part, copied whole, only segments inserted around it. Anything else (a modified or split copy, an infix, reduplication, compounding, a realizational rule) can rewrite or split what an earlier stratum wrote.
fn plain_affix(rule: &MorphRuleDef) -> bool {
    let MorphRuleDef::AffixProcess(def) = rule else {
        return false;
    };
    def.allomorphs.iter().all(|a| {
        let copies = a
            .rhs
            .iter()
            .filter(|o| matches!(o, OutputAction::Copy(_)))
            .count();
        let only_inserts = a.rhs.iter().all(|o| {
            matches!(
                o,
                OutputAction::Copy(_) | OutputAction::InsertSegments { .. }
            )
        });
        a.lhs.len() <= 1 && copies == 1 && only_inserts
    })
}

/// Whether a segment the engine could spell a node of `lanes` as holds back a feature the node pins (it leaves the feature unspecified), so the spelled word no longer satisfies the pattern.
fn spelling_underspecified(g: &Grammar, table: &CharDefTable, lanes: &[u64]) -> bool {
    let nfeat = g.phon_features.len();
    let mut node = lanes.to_vec();
    for (f, lane) in node.iter_mut().enumerate().skip(nfeat) {
        *lane = pg_grammar_model::membership::mask(&g.phon_features, f);
    }
    table.iter().any(|(_, def)| {
        let seg = def.matching_lanes();
        def.kind() == CharDefKind::Segment
            && pg_featstruct::flat_unifiable(&node, &seg)
            && seg.iter().zip(&node).take(nfeat).any(|(s, n)| s & !n != 0)
    })
}

/// A lane vector that constrains nothing.
fn full_lanes(g: &Grammar) -> Vec<u64> {
    (0..pg_grammar_model::membership::width(&g.phon_features))
        .map(|f| pg_grammar_model::membership::mask(&g.phon_features, f))
        .collect()
}

fn flatten<'a>(nodes: &'a [PatternNode], out: &mut Vec<&'a PatternNode>) {
    for n in nodes {
        match n {
            PatternNode::Anchor(_) => {}
            PatternNode::Quantifier { children, .. } => flatten(children, out),
            n => out.push(n),
        }
    }
}

/// The match constraint of each environment node of `sub`.
fn env_lanes(g: &Grammar, table: TableId, sub: &RewriteSubruleDef) -> Vec<Vec<u64>> {
    let mut nodes = Vec::new();
    for p in [sub.left_env.as_ref(), sub.right_env.as_ref()]
        .into_iter()
        .flatten()
    {
        flatten(&p.nodes, &mut nodes);
    }
    nodes
        .into_iter()
        .map(|n| crate::rewrite::node_full_lanes(g, &g.char_tables[table.0 as usize], n))
        .collect()
}

/// The alpha variables the environment binds, with their features.
fn env_vars(sub: &RewriteSubruleDef) -> Vec<(VarId, FlatIndex)> {
    let mut nodes = Vec::new();
    for p in [sub.left_env.as_ref(), sub.right_env.as_ref()]
        .into_iter()
        .flatten()
    {
        flatten(&p.nodes, &mut nodes);
    }
    nodes
        .into_iter()
        .flat_map(|n| match n {
            PatternNode::Context(sc) => sc.vars.iter().map(|v| (v.var, v.feature)).collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// What the rule writes in place of its target, as match text, with the constraint of each node written; the text is the engine's own rewrite: a same-length rule overlays each right-hand node's features on the target node (an alpha variable takes the environment's binding), any other rule deletes the target and inserts the right-hand nodes.
fn output_target(
    g: &Grammar,
    printer: &mut PatternPrinter,
    table: TableId,
    rule: &RewriteRuleDef,
    sub: &RewriteSubruleDef,
) -> Result<(String, Vec<Vec<u64>>), Unprintable> {
    let chardefs = &g.char_tables[table.0 as usize];
    let bound = env_vars(sub);
    let bound_ok = |vars: &[AlphaVar]| {
        vars.iter()
            .all(|v| bound.iter().any(|(id, f)| *id == v.var && *f == v.feature))
    };
    let mut rhs_nodes = Vec::new();
    for n in &sub.rhs.nodes {
        match n {
            PatternNode::CharDef(_) | PatternNode::Context(_) => rhs_nodes.push(n),
            _ => return Err(Unprintable("rhs_node_kind")),
        }
    }
    let vars_of = |n: &PatternNode| -> Vec<AlphaVar> {
        match n {
            PatternNode::Context(sc) => sc.vars.clone(),
            _ => Vec::new(),
        }
    };
    let nfeat = g.phon_features.len();
    if rule.lhs.nodes.len() != sub.rhs.nodes.len() || rule.lhs.nodes.is_empty() {
        for n in &rhs_nodes {
            if !bound_ok(&vars_of(n)) {
                return Err(Unprintable("alpha_not_in_environment"));
            }
        }
        let text = printer.print(&sub.rhs, Place::Target)?;
        let covered: Vec<Vec<u64>> = rhs_nodes
            .iter()
            .map(|n| crate::rewrite::node_full_lanes(g, chardefs, n))
            .collect();
        for (n, lanes) in rhs_nodes.iter().zip(&covered) {
            if matches!(n, PatternNode::Context(_)) && spelling_underspecified(g, chardefs, lanes) {
                return Err(Unprintable("output_underspecified_segments"));
            }
        }
        return Ok((text, covered));
    }
    let mut words = Vec::new();
    let mut covered = Vec::new();
    for (l, r) in rule.lhs.nodes.iter().zip(rhs_nodes) {
        if !matches!(l, PatternNode::CharDef(_) | PatternNode::Context(_)) {
            return Err(Unprintable("lhs_node_kind"));
        }
        if is_boundary(g, l, table) || is_boundary(g, r, table) {
            return Err(Unprintable("boundary_rewrite"));
        }
        // Both sides are segments, so the `Type` pair every loaded class carries agrees and the bundle text does not name it.
        let type_lane = g.phon_features.type_flat().0 as usize;
        let features = |n: &PatternNode| -> Vec<(usize, u64)> {
            node_pins(g, chardefs, n)
                .into_iter()
                .filter(|(f, _)| *f < nfeat && *f != type_lane)
                .collect()
        };
        let (pins_l, pins_r) = (features(l), features(r));
        let (vars_l, vars_r) = (vars_of(l), vars_of(r));
        let mut result: Vec<(usize, u64)> = pins_l
            .iter()
            .filter(|(f, _)| {
                !pins_r.iter().any(|(g, _)| g == f)
                    && !vars_r.iter().any(|v| v.feature.0 as usize == *f)
            })
            .copied()
            .collect();
        result.extend(pins_r.iter().copied());
        let mut vars: Vec<AlphaVar> = vars_l
            .into_iter()
            .filter(|v| {
                !pins_r.iter().any(|(f, _)| *f == v.feature.0 as usize)
                    && !vars_r.iter().any(|w| w.feature == v.feature)
            })
            .collect();
        vars.extend(vars_r.iter().cloned());
        if !bound_ok(&vars) {
            return Err(Unprintable("alpha_not_in_environment"));
        }
        result.sort_unstable();
        let mut sorted_r = pins_r.clone();
        sorted_r.sort_unstable();
        let same_as_r = result == sorted_r && vars == vars_r;
        let word = if same_as_r {
            printer.print(
                &Pattern {
                    nodes: vec![r.clone()],
                },
                Place::Target,
            )?
        } else {
            let pairs: Vec<(FlatIndex, SymbolBits)> = result
                .iter()
                .map(|(f, b)| (FlatIndex(*f as u32), SymbolBits(*b)))
                .collect();
            printer.print_bundle(&pairs, &vars).map_err(|e| match e.0 {
                "feature_value" | "class_not_writable" => Unprintable("output_not_single_class"),
                _ => e,
            })?
        };
        let mut lanes = full_lanes(g);
        for (f, b) in &result {
            lanes[*f] = *b;
        }
        if spelling_underspecified(g, chardefs, &lanes) {
            return Err(Unprintable("output_underspecified_segments"));
        }
        words.push(word);
        covered.push(lanes);
    }
    Ok((words.join(" "), covered))
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
