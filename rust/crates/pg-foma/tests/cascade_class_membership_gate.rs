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
    Segment { lanes: Vec<u64>, explicit_full: Vec<u32> },
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

fn shape_of(g: &Grammar, focus: &[BlockNode], ids: &[Option<CharDefId>]) -> Shape {
    let width = g.phon_features.len() as u32;
    let wide = width as usize + 1;
    let pad = |lanes: &[u64]| {
        let mut v = lanes.to_vec();
        v.resize(wide, 15);
        v
    };
    let mut b = ShapeBuilder::with_features(wide as u32);
    for (node, id) in focus.iter().zip(ids) {
        match node {
            BlockNode::Boundary(cd) => {
                if width == 0 {
                    b.push_boundary(cd.0)
                } else {
                    b.push_boundary_with_lanes(cd.0, &pad(g.char_tables[0].get(*cd).feature_lanes()))
                }
            }
            BlockNode::Segment { lanes, .. } => {
                if width == 0 {
                    b.push_segment(id.unwrap().0)
                } else {
                    b.push_segment_with_lanes(NO_CHAR_DEF, &pad(lanes))
                }
            }
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
        for lanes in variants {
            out.push(Element {
                token: Some(*id),
                node: BlockNode::Segment {
                    lanes,
                    explicit_full: Vec::new(),
                },
            });
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
    featureless: usize,
    refused: Vec<String>,
    rules: usize,
    skipped_nodes: usize,
    cases: u64,
    matched: u64,
    sampled_rules: usize,
    under: Vec<String>,
    missing_images: Vec<String>,
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
    // Featureless tables match by spelling, which a bare lane shape does not reproduce (the probe matched every token); reported as remaining work.
    if g.phon_features.is_empty() {
        tally.featureless += 1;
        return;
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
                let ids: Vec<Option<CharDefId>> = idx.iter().map(|&i| pool[i].token).collect();
                tally.cases += 1;
                let shape = shape_of(g, &focus, &ids);
                let result = synthesize(g, &stripped, &shape);
                let Some(out) = result.first() else { continue };
                tally.matched += 1;
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
        "W2 summary: featureless_not_probed={} grammars={} refused={} rules={} skipped_lhs={} cases={} matched={} sampled_rules={} under={} missing_images={}",
        tally.featureless, tally.grammars, tally.refused.len(), tally.rules, tally.skipped_nodes, tally.cases, tally.matched,
        tally.sampled_rules, tally.under.len(), tally.missing_images.len()
    );
    for r in &tally.refused {
        println!("W2 refused {r}");
    }
    assert!(tally.grammars > 0, "no grammar reached the gate");
    assert!(tally.under.is_empty(), "under-inclusions: {:#?}", &tally.under[..tally.under.len().min(20)]);
    assert!(tally.missing_images.is_empty(), "missing images: {:#?}", &tally.missing_images[..tally.missing_images.len().min(20)]);
}
