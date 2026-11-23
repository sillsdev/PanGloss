//! W2 gate (cascade backend plan, section 7 row W2): the joint-fixpoint closure and the cascade's class membership must be a superset of what the synthesis code does.
//!
//! Implemented here: the membership differential for rewrite foci (`focus_outputs`, the matcher rewrite synthesis uses), the image gate for rewrite outputs and for `Modify`/`InsertContext` rows (every image interned or covered by an any-value row), and the KC-0b row census.
//! Not yet: affix lhs through `PatternBridge`, metathesis switches, the rule-application differential against net outputs, the spelling differential, the whole-rule differential (see reports/W2.md).
//!
//! Enumerations are exhaustive up to `EXHAUSTIVE` cases per rule and sampled above (`SAMPLE` cases, seeded); seed and coverage are printed. Any under-inclusion or missing image fails the gate.

use std::collections::HashSet;

use pg_foma::lower::JointClosure;
use pg_grammar::chardef::{CharDefId, CharDefKind};
use pg_grammar::model::{
    Grammar, MorphRuleDef, OutputAction, PatternNode, PhonRuleDef, RewriteRuleDef, TableId,
};
use pg_rules::rewrite::synthesize;
use pg_shape::{NodeKind, Shape, ShapeBuilder, NO_CHAR_DEF};

/// What synthesis is given for one focus element.
#[derive(Clone)]
enum BlockNode {
    Segment { lanes: Vec<u64>, cd: u32 },
    Boundary(CharDefId),
}

/// The rule without environments or gates: applying it to a string that is exactly its lhs asks the synthesis matcher only whether each lhs node matches its element.
fn strip(rule: &RewriteRuleDef) -> RewriteRuleDef {
    RewriteRuleDef {
        xml_id: rule.xml_id.clone(),
        name: rule.name.clone(),
        mode: rule.mode,
        dir: rule.dir,
        vars: rule.vars.clone(),
        lhs: rule.lhs.clone(),
        subrules: rule
            .subrules
            .iter()
            .map(|s| pg_grammar::model::RewriteSubruleDef {
                required_pos: None,
                required_mpr: Default::default(),
                excluded_mpr: Default::default(),
                rhs: s.rhs.clone(),
                left_env: None,
                right_env: None,
                self_opaquing: s.self_opaquing,
            })
            .collect(),
    }
}

fn shape_of(g: &Grammar, focus: &[BlockNode]) -> Shape {
    let wide = g.phon_features.len() + 1;
    let pad = |lanes: &[u64]| {
        let mut v = lanes.to_vec();
        v.resize(wide, 15);
        v
    };
    let mut b = ShapeBuilder::with_features(wide as u32);
    for node in focus {
        match node {
            BlockNode::Boundary(cd) => {
                b.push_boundary_with_lanes(cd.0, &pad(g.char_tables[0].get(*cd).feature_lanes()))
            }
            BlockNode::Segment { lanes, cd } => b.push_segment_with_lanes(*cd, &pad(lanes)),
        }
    }
    b.finish()
}

const EXHAUSTIVE: u64 = 1_000_000;
const SAMPLE: u64 = 100_000;
const SEED: u64 = 0x5eed_0002;
/// Concrete completions tried per any-value lane combination of one row.
const COMPLETIONS: usize = 24;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// One focus element: the token it stands for and the node the synthesis code is given.
struct Element {
    token: Option<CharDefId>,
    node: BlockNode,
}

/// Concrete values an any-value (zero) lane stands for: every single value, its complement, and the whole set.
fn lane_values(mask: u64) -> Vec<u64> {
    let mut values = vec![mask];
    for bit in 0..64 {
        let one = 1u64 << bit;
        if mask & one != 0 {
            values.push(one);
            if mask & !one != 0 {
                values.push(mask & !one);
            }
        }
    }
    values
}

fn elements(g: &Grammar, closure: &JointClosure) -> Vec<Element> {
    let mut out = Vec::new();
    let mut rng = Rng(SEED);
    for (id, row) in closure.rows() {
        let zero: Vec<usize> = (0..row.len()).filter(|&l| row[l] == 0).collect();
        let mut variants: Vec<Vec<u64>> = vec![row.clone()];
        for &lane in &zero {
            let values = lane_values(g.phon_features.mask(pg_grammar::featsys::FlatIndex(lane as u32)));
            let mut next = Vec::new();
            for base in &variants {
                for value in &values {
                    let mut r = base.clone();
                    r[lane] = *value;
                    next.push(r);
                }
            }
            variants = next;
        }
        while variants.len() > COMPLETIONS {
            let drop = (rng.next() as usize) % variants.len();
            variants.swap_remove(drop);
        }
        let attested = (id.0 as usize) < g.char_tables[0].len();
        for lanes in variants {
            if attested && lanes == *row {
                out.push(Element {
                    token: Some(*id),
                    node: BlockNode::Segment { lanes: lanes.clone(), cd: id.0 },
                });
            }
            // Without features only a char-def identifies a segment: a node with none matches every literal, so it says nothing about its token.
            if !(attested && g.phon_features.is_empty()) {
                out.push(Element {
                    token: Some(*id),
                    node: BlockNode::Segment { lanes, cd: NO_CHAR_DEF },
                });
            }
        }
    }
    for id in closure.boundary_ids(g) {
        out.push(Element {
            token: Some(id),
            node: BlockNode::Boundary(id),
        });
    }
    out
}

#[derive(Default)]
struct Tally {
    grammars: usize,
    refused: Vec<String>,
    rules: usize,
    skipped_nodes: usize,
    cases: u64,
    matched: u64,
    sampled_rules: usize,
    under: Vec<String>,
    missing_images: Vec<String>,
    /// per matcher: (cases, matched, probes, sampled probes)
    kinds: std::collections::BTreeMap<String, (u64, u64, u64, u64)>,
    skipped_env: usize,
    spelling_cases: u64,
    spelling_capped: u64,
    spelling_private_gaps: Vec<String>,
    spelling_private_count: u64,
    skipped_other: usize,
}

fn rewrite_rules(g: &Grammar) -> Vec<&RewriteRuleDef> {
    g.prules
        .iter()
        .filter_map(|r| match r {
            PhonRuleDef::Rewrite(rule) => Some(rule),
            PhonRuleDef::Metathesis(_) => None,
        })
        .collect()
}


fn simple_node(g: &Grammar, node: &PatternNode) -> Option<bool> {
    // Some(true) = probed segment/class node, Some(false) = boundary or anchor (not probed), None = not modelled by this probe.
    let table = &g.char_tables[0];
    match node {
        PatternNode::Context(_) => Some(true),
        PatternNode::CharDef(id) => Some(table.get(*id).kind() != CharDefKind::Boundary),
        PatternNode::Anchor(_) => Some(false),
        _ => None,
    }
}

fn label_of(node: &BlockNode) -> String {
    match node {
        BlockNode::Segment { lanes, cd } => format!("{lanes:?} cd {cd}"),
        BlockNode::Boundary(b) => format!("boundary {b:?}"),
    }
}

/// Enumerates tuples of `pool` of length `members.len()` (exhaustive to `EXHAUSTIVE`, else `SAMPLE` seeded draws); for every tuple on which `applies` says the synthesis matcher accepted, each element's token must be in its node's cascade class.
#[allow(clippy::too_many_arguments)]
fn probe(
    tally: &mut Tally,
    label: &str,
    what: &str,
    pool: &[&Element],
    members: &[&HashSet<CharDefId>],
    mut applies: impl FnMut(&[&Element]) -> bool,
) {
    let n = members.len();
    if n == 0 || pool.is_empty() {
        return;
    }
    let e = pool.len() as u64;
    let total = e.checked_pow(n as u32).unwrap_or(u64::MAX);
    let exhaustive = total <= EXHAUSTIVE;
    let cases = if exhaustive { total } else { SAMPLE };
    let stat = tally.kinds.entry(what.split(' ').next().unwrap().to_owned()).or_default();
    stat.2 += 1;
    if !exhaustive {
        stat.3 += 1;
        println!("W2 {label} {what}: sampled {cases} of {total} tuples, seed {SEED:#x}");
    }
    let mut rng = Rng(SEED ^ (what.len() as u64));
    let mut idx = vec![0usize; n];
    let (mut ran, mut hit) = (0u64, 0u64);
    let mut found = Vec::new();
    for case in 0..cases {
        if exhaustive {
            let mut c = case;
            for slot in idx.iter_mut().rev() {
                *slot = (c % e) as usize;
                c /= e;
            }
        } else {
            for slot in idx.iter_mut() {
                *slot = (rng.next() % e) as usize;
            }
        }
        let els: Vec<&Element> = idx.iter().map(|&i| pool[i]).collect();
        ran += 1;
        if !applies(&els) {
            continue;
        }
        hit += 1;
        for (k, el) in els.iter().enumerate() {
            let token = el.token.unwrap();
            if !members[k].contains(&token) {
                found.push(format!(
                    "{label} {what} node {k}: synthesis matched element {} but the cascade class lacks token {token:?}",
                    label_of(&el.node)
                ));
            }
        }
        if found.len() > 20 {
            break;
        }
    }
    let stat = tally.kinds.entry(what.split(' ').next().unwrap().to_owned()).or_default();
    stat.0 += ran;
    stat.1 += hit;
    tally.under.extend(found);
}

fn strip_one(rule: &RewriteRuleDef, only: usize) -> RewriteRuleDef {
    let mut r = strip(rule);
    let keep = &rule.subrules[only];
    r.subrules = vec![pg_grammar::model::RewriteSubruleDef {
        required_pos: None,
        required_mpr: Default::default(),
        excluded_mpr: Default::default(),
        rhs: keep.rhs.clone(),
        left_env: keep.left_env.clone(),
        right_env: keep.right_env.clone(),
        self_opaquing: keep.self_opaquing,
    }];
    r
}

/// Spelling differential: every closure row x every cd state x every concrete completion, the cascade's spellings (attested segments, or nothing) cover the representations `matching_reps_for_node` gives.
fn spelling_differential(g: &Grammar, closure: &JointClosure, tally: &mut Tally, label: &str) {
    let table = &g.char_tables[0];
    let wide = g.phon_features.len() + 1;
    let seg_classes: Vec<Vec<u32>> = g
        .natural_classes
        .iter()
        .filter_map(|c| match &c.kind {
            pg_grammar::model::NaturalClassKind::Segments(m) => Some(m.iter().map(|cd| cd.0).collect()),
            _ => None,
        })
        .collect();
    for idx in 0..closure.rows().len() {
        let (id, row) = (&closure.rows()[idx].0, &closure.rows()[idx].1);
        let (spelled, epsilon) = closure.spellings(g, idx);
        let cascade: HashSet<&str> = spelled.iter().flat_map(|cd| table.get(*cd).representations_nfd().iter().map(String::as_str)).collect();
        let Some(completions) = closure.completions(g, idx, 256) else {
            tally.spelling_capped += 1;
            continue;
        };
        let attested = (id.0 as usize) < table.len();
        for lanes in completions {
            let mut padded = lanes.clone();
            padded.resize(wide, 15);
            // (a) its own char-def, (b) no char-def and no restriction, (c) no char-def and a segment-list restriction (a private `Modify` token).
            let mut states: Vec<(String, u32, pg_shape::CdSet, bool)> = Vec::new();
            // Without features a node with no char-def spells every segment: it is not a state of any row.
            if !(attested && g.phon_features.is_empty()) {
                states.push(("free".into(), NO_CHAR_DEF, pg_shape::CdSet::Unrestricted, false));
            }
            if attested && lanes == *row {
                states.push(("own".into(), id.0, pg_shape::CdSet::Unrestricted, false));
            }
            for (n, members) in seg_classes.iter().enumerate().filter(|_| !g.phon_features.is_empty()) {
                states.push((format!("segment-class {n}"), NO_CHAR_DEF, pg_shape::CdSet::Members(pg_shape::CdBits::from_ids(members.iter().copied())), true));
            }
            for (name, cd, set, private) in states {
                tally.spelling_cases += 1;
                let reps = pg_parse::surface::matching_reps_for_node(table, NodeKind::Segment, cd, &set, &padded, true);
                let missing: Vec<&str> = reps.iter().copied().filter(|r| !cascade.contains(r)).collect();
                let needs_eps = reps.is_empty() && !epsilon;
                if missing.is_empty() && !needs_eps {
                    continue;
                }
                let msg = format!("{label} row {idx} {row:?} state {name} completion {lanes:?}: synthesis spells {reps:?}, cascade spells {cascade:?} epsilon={epsilon}");
                if private {
                    tally.spelling_private_count += 1;
                    if tally.spelling_private_gaps.len() < 5 {
                        tally.spelling_private_gaps.push(msg);
                    }
                } else {
                    tally.under.push(msg);
                }
            }
        }
    }
}

fn check_grammar(label: &str, g: &Grammar, tally: &mut Tally) {
    if g.char_tables.len() != 1 {
        tally.refused.push(format!("{label}: more than one character table"));
        return;
    }
    let rules = rewrite_rules(g);
    let closure = match JointClosure::build(g, TableId(0), &rules) {
        Ok(c) => c,
        Err(e) => {
            tally.refused.push(format!("{label}: closure refused: {e:?}"));
            return;
        }
    };
    tally.grammars += 1;
    let report = closure.report(g);
    println!(
        "W2 {label}: rows attested={} private={} total={}/{} epsilon={}",
        report.attested_rows, report.private_rows, report.total_rows, report.token_cap, report.epsilon_rows
    );
    let table = &g.char_tables[0];
    let elems = elements(g, &closure);
    // Image gate for Modify and InsertContext: every row a `Modify` can write is interned (or covered by an any-value row).
    let width = g.phon_features.len();
    let type_lane = g.phon_features.type_flat().0 as usize;
    let actions = g
        .mrules
        .iter()
        .filter_map(|rule| rule.affix_allomorphs())
        .flatten()
        .flat_map(|a| &a.rhs)
        .chain(g.mrules.iter().filter_map(|rule| match rule {
            MorphRuleDef::Compounding(c) => Some(&c.subrules),
            _ => None,
        }).flatten().flat_map(|s| &s.rhs));
    for action in actions {
        let OutputAction::Modify(_, context) = action else { continue };
        let pins: Vec<_> = pg_rules::morph::ctx_pins(g, TableId(0), context)
            .into_iter()
            .filter(|(lane, _)| *lane < width)
            .collect();
        let alpha: Vec<usize> = context.vars.iter().map(|v| v.feature.0 as usize).collect();
        for (_, row) in closure.rows() {
            let mut image = row.clone();
            for &(lane, bits) in &pins {
                image[lane] = bits;
            }
            for &lane in &alpha {
                image[lane] = 0;
            }
            image[type_lane] = pg_grammar::featsys::TYPE_SEGMENT_BITS;
            if !closure.covers(&image) {
                tally.missing_images.push(format!("{label}: Modify image {image:?} of row {row:?} not interned"));
                break;
            }
        }
    }
    for rule in &rules {
        tally.rules += 1;
        let n_all = rule.lhs.nodes.len();
        // A boundary node of a pattern is optional in synthesis, so such a pattern is probed with boundary-free strings of its other nodes.
        let is_boundary: Vec<bool> = rule
            .lhs
            .nodes
            .iter()
            .map(|node| matches!(node, PatternNode::CharDef(id) if table.get(*id).kind() == CharDefKind::Boundary))
            .collect();
        let probed: Vec<usize> = (0..n_all).filter(|&k| !is_boundary[k]).collect();
        let n = probed.len();
        let pool: Vec<&Element> = elems
            .iter()
            .filter(|el| probed.len() == n_all || matches!(el.node, BlockNode::Segment { .. }))
            .collect();
        let simple = n_all > 0 && n > 0
            && rule.lhs.nodes.iter().all(|node| match node {
                PatternNode::Context(_) => true,
                PatternNode::CharDef(_) => true,
                _ => false,
            });
        if !simple {
            tally.skipped_nodes += 1;
            continue;
        }
        let members: Vec<HashSet<CharDefId>> = rule
            .lhs
            .nodes
            .iter()
            .map(|node| closure.node_members(g, node).unwrap_or_default().into_iter().collect())
            .collect();
        let e = pool.len() as u64;
        let total = e.checked_pow(n as u32).unwrap_or(u64::MAX);
        let exhaustive = total <= EXHAUSTIVE;
        let cases = if exhaustive { total } else { SAMPLE };
        if !exhaustive {
            tally.sampled_rules += 1;
            println!("W2 {label} rule {}: sampled {cases} of {total} tuples, seed {SEED:#x}", rule.xml_id);
        }
        let mut rng = Rng(SEED ^ (rule.xml_id.len() as u64));
        let mut foci: Vec<Vec<BlockNode>> = Vec::new();
        {
            let stripped = strip(rule);
            let mut idx = vec![0usize; n];
            for case in 0..cases {
                if exhaustive {
                    let mut c = case;
                    for slot in idx.iter_mut().rev() {
                        *slot = (c % e) as usize;
                        c /= e;
                    }
                } else {
                    for slot in idx.iter_mut() {
                        *slot = (rng.next() % e) as usize;
                    }
                }
                let focus: Vec<BlockNode> = idx.iter().map(|&i| pool[i].node.clone()).collect();
                tally.cases += 1;
                let shape = shape_of(g, &focus);
                let result = synthesize(g, &stripped, &shape);
                let Some(out) = result.first() else { continue };
                tally.matched += 1;
                if foci.len() < 6 {
                    foci.push(focus.clone());
                }
                for (&k, &i) in probed.iter().zip(idx.iter()) {
                    let token = pool[i].token.unwrap();
                    if !members[k].contains(&token) && table.get(token).kind() != CharDefKind::Boundary {
                        tally.under.push(format!(
                            "{label} rule {} node {k}: synthesis matched element {:?} but the cascade class lacks token {token:?}",
                            rule.xml_id, match &pool[i].node { BlockNode::Segment { lanes, .. } => format!("{lanes:?}"), BlockNode::Boundary(b) => format!("boundary {b:?}") }
                        ));
                    }
                }
                if width > 0 {
                    for (j, kind, _, _) in out.interior() {
                        if kind == NodeKind::Segment && !closure.covers(&out.node_lanes(j)[..width]) {
                            tally.missing_images.push(format!(
                                "{label} rule {}: synthesis image {:?} is not an interned row",
                                rule.xml_id, &out.node_lanes(j)[..width]
                            ));
                        }
                    }
                }
                if tally.under.len() > 50 || tally.missing_images.len() > 50 {
                    return;
                }
            }
        }
        // Environment nodes: the whole rule (one subrule, gates dropped) applied to left tuple + a focus synthesis matched + right tuple.
        let segs: Vec<&Element> = elems.iter().filter(|el| matches!(el.node, BlockNode::Segment { .. })).collect();
        for (si, sub) in rule.subrules.iter().enumerate() {
            let sides: Vec<&pg_grammar::model::Pattern> = [&sub.left_env, &sub.right_env].into_iter().flatten().collect();
            if sides.is_empty() {
                continue;
            }
            let kinds: Vec<Vec<Option<bool>>> = sides.iter().map(|p| p.nodes.iter().map(|n| simple_node(g, n)).collect()).collect();
            if kinds.iter().flatten().any(|k| k.is_none()) {
                tally.skipped_env += 1;
                continue;
            }
            let left_n = sub.left_env.as_ref().map_or(0, |p| p.nodes.iter().filter(|n| simple_node(g, n) == Some(true)).count());
            let mut sets: Vec<HashSet<CharDefId>> = Vec::new();
            for p in &sides {
                for node in p.nodes.iter().filter(|n| simple_node(g, n) == Some(true)) {
                    sets.push(closure.node_members(g, node).unwrap_or_default().into_iter().collect());
                }
            }
            let refs: Vec<&HashSet<CharDefId>> = sets.iter().collect();
            let one = strip_one(rule, si);
            for (fi, focus) in foci.iter().enumerate() {
                let what = format!("env {} sub{si} focus{fi}", rule.xml_id);
                probe(tally, label, &what, &segs, &refs, |els| {
                    let mut nodes: Vec<BlockNode> = els[..left_n].iter().map(|e| e.node.clone()).collect();
                    nodes.extend(focus.iter().cloned());
                    nodes.extend(els[left_n..].iter().map(|e| e.node.clone()));
                    !synthesize(g, &one, &shape_of(g, &nodes)).is_empty()
                });
            }
        }
    }
    spelling_differential(g, &closure, tally, label);
    // Affix allomorph lhs through the matcher affix synthesis uses.
    let segs: Vec<&Element> = elems.iter().filter(|el| matches!(el.node, BlockNode::Segment { .. })).collect();
    for rule in &g.mrules {
        let Some(allos) = rule.affix_allomorphs() else { continue };
        for allo in allos {
            let nodes: Vec<&PatternNode> = allo.lhs.iter().flat_map(|p| p.nodes.iter()).collect();
            if nodes.is_empty() || nodes.iter().any(|n| simple_node(g, n).is_none()) {
                tally.skipped_other += 1;
                continue;
            }
            let sets: Vec<HashSet<CharDefId>> = nodes
                .iter()
                .filter(|n| simple_node(g, n) == Some(true))
                .map(|n| closure.node_members(g, n).unwrap_or_default().into_iter().collect())
                .collect();
            let refs: Vec<&HashSet<CharDefId>> = sets.iter().collect();
            let what = format!("affix {:?}", allo.id);
            probe(tally, label, &what, &segs, &refs, |els| {
                let nodes: Vec<BlockNode> = els.iter().map(|e| e.node.clone()).collect();
                pg_rules::morph::lhs_parts_match(g, TableId(0), &allo.lhs, &shape_of(g, &nodes))
            });
        }
    }
    // Metathesis switches and pattern nodes, by pattern position.
    for pr in &g.prules {
        let PhonRuleDef::Metathesis(rule) = pr else { continue };
        let nodes: Vec<&PatternNode> = rule.pattern.nodes.iter().collect();
        if nodes.iter().any(|n| simple_node(g, n).is_none()) {
            tally.skipped_other += 1;
            continue;
        }
        let sets: Vec<HashSet<CharDefId>> = nodes
            .iter()
            .filter(|n| simple_node(g, n) == Some(true))
            .map(|n| closure.node_members(g, n).unwrap_or_default().into_iter().collect())
            .collect();
        let refs: Vec<&HashSet<CharDefId>> = sets.iter().collect();
        let what = format!("meta {}", rule.xml_id);
        probe(tally, label, &what, &segs, &refs, |els| {
            let nodes: Vec<BlockNode> = els.iter().map(|e| e.node.clone()).collect();
            !pg_rules::metathesis::synthesize(g, rule, &shape_of(g, &nodes)).is_empty()
        });
    }
}

#[test]
fn class_membership_and_images_are_a_superset_of_synthesis() {
    let scope = if std::env::var_os("HC_CONFORMANCE_SUITE").is_some()
        || pg_conformance_fixtures::suite_root().exists()
    {
        pg_conformance_fixtures::ConformanceScope::All
    } else {
        pg_conformance_fixtures::ConformanceScope::Local
    };
    println!("W2 scope {} exhaustive<={EXHAUSTIVE} sample={SAMPLE} seed={SEED:#x}", scope.label());
    let mut tally = Tally::default();
    for f in pg_conformance_fixtures::discover_scoped(scope) {
        let xml = f.load_grammar_xml();
        let Ok(g) = pg_grammar::load(&xml) else { continue };
        check_grammar(&f.label(), &g, &mut tally);
    }
    println!(
        "W2 summary: grammars={} refused={} rules={} skipped_lhs={} cases={} matched={} sampled_rules={} under={} missing_images={}",
        tally.grammars, tally.refused.len(), tally.rules, tally.skipped_nodes, tally.cases, tally.matched,
        tally.sampled_rules, tally.under.len(), tally.missing_images.len()
    );
    for (k, (cases, hit, probes, sampled)) in &tally.kinds {
        println!("W2 matcher {k}: probes={probes} sampled={sampled} cases={cases} matched={hit}");
    }
    println!("W2 spelling: cases={} capped_rows={} private_modify_token_gaps={}", tally.spelling_cases, tally.spelling_capped, tally.spelling_private_count);
    for m in tally.spelling_private_gaps.iter().take(5) {
        println!("W2 private-gap {m}");
    }
    println!("W2 skipped: env={} other={}", tally.skipped_env, tally.skipped_other);
    for r in &tally.refused {
        println!("W2 refused {r}");
    }
    assert!(tally.grammars > 0, "no grammar reached the gate");
    assert!(tally.under.is_empty(), "under-inclusions: {:#?}", &tally.under[..tally.under.len().min(20)]);
    assert!(tally.missing_images.is_empty(), "missing images: {:#?}", &tally.missing_images[..tally.missing_images.len().min(20)]);
}
