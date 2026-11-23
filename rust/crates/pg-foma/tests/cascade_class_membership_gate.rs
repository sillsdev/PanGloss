//! W2 gate (cascade backend plan, section 7 row W2): the joint-fixpoint closure and the cascade's class membership must be a superset of what the synthesis code does.
//!
//! Implemented here: the membership differential for rewrite foci (`focus_outputs`, the matcher rewrite synthesis uses), the image gate for rewrite outputs and for `Modify`/`InsertContext` rows (every image interned or covered by an any-value row), and the KC-0b row census.
//! Not yet: affix lhs through `PatternBridge`, metathesis switches, the rule-application differential against net outputs, the spelling differential, the whole-rule differential (see reports/W2.md).
//!
//! Enumerations are exhaustive up to `EXHAUSTIVE` cases per rule and sampled above (`SAMPLE` cases, seeded); seed and coverage are printed. Any under-inclusion or missing image fails the gate.

use std::collections::HashSet;

use pg_foma::cascade_parts::CascadeParts;
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
const SAMPLE_DEFAULT: u64 = 100_000;

/// Sample size above the exhaustive limit; `PG_W2_SAMPLE` lowers it for development runs (the printed value is what ran).
fn sample() -> u64 {
    std::env::var("PG_W2_SAMPLE").ok().and_then(|v| v.parse().ok()).unwrap_or(SAMPLE_DEFAULT)
}
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
    /// (matcher, strings tried, applied, no-token skips)
    nets: std::collections::BTreeMap<String, (u64, u64, u64)>,
    net_refused: usize,
    net_misses: Vec<String>,
    spelling_cases: u64,
    spelling_capped: u64,
    spelling_twins: u64,
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
    let cases = if exhaustive { total } else { sample() };
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
    for idx in 0..closure.rows().len() {
        let (id, row) = (&closure.rows()[idx].0, &closure.rows()[idx].1);
        let kind = closure.row_kind(*id);
        if kind == pg_foma::lower::RowKind::EmptyTwin {
            tally.spelling_twins += 1;
            continue;
        }
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
            if let pg_foma::lower::RowKind::Private(members) = &kind {
                states.clear();
                states.push(("private".into(), NO_CHAR_DEF, pg_shape::CdSet::Members(pg_shape::CdBits::from_ids(members.iter().map(|m| m.0))), false));
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

struct NetCtx<'a> {
    label: &'a str,
    g: &'a Grammar,
    parts: &'a CascadeParts,
}

impl NetCtx<'_> {
    fn width(&self) -> usize {
        self.g.phon_features.len()
    }

    /// The tokens the net's own closure has for a node: its char-def when it keeps one, else every row whose lanes cover it (a zero lane covers any value).
    fn tokens(&self, node: &BlockNode) -> Vec<char> {
        let table = &self.g.char_tables[0];
        match node {
            BlockNode::Boundary(cd) => vec![self.parts.token(*cd)],
            BlockNode::Segment { cd, .. } if (*cd as usize) < table.len() => vec![self.parts.token(CharDefId(*cd))],
            BlockNode::Segment { lanes, .. } => {
                let w = self.width();
                let covering: Vec<(CharDefId, bool)> = self
                    .parts
                    .rows()
                    .iter()
                    .filter(|(_, row)| row.len() >= w && row.iter().zip(lanes).take(w).all(|(r, l)| *r == 0 || r == l))
                    .map(|(id, row)| (*id, row.iter().zip(lanes).take(w).all(|(r, l)| r == l)))
                    .collect();
                let exact: Vec<char> = covering.iter().filter(|(_, e)| *e).map(|(id, _)| self.parts.token(*id)).collect();
                if exact.is_empty() { covering.iter().map(|(id, _)| self.parts.token(*id)).collect() } else { exact }
            }
        }
    }
}

/// Cases foma 0.4.2's apply-up could not check (it panics with an integer overflow at apply.rs:1135): UNCHECKED, never matched. Per grammar the first 5 are printed on `W2 foma-panic:` lines; the first one's net is dumped as AT&T text to `PG_W2_PANIC_NET` (default the temp dir).
static APPLY_PANICS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PANIC_LOG: std::sync::Mutex<Vec<(String, u64)>> = std::sync::Mutex::new(Vec::new());

fn record_panic(label: &str, rule: &RewriteRuleDef, net: &foma::types::Fsm, up_input: &str, down_input: &str) {
    APPLY_PANICS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut log = PANIC_LOG.lock().unwrap();
    let first = log.is_empty();
    let entry = match log.iter_mut().find(|(l, _)| l == label) {
        Some(e) => e,
        None => {
            log.push((label.to_string(), 0));
            log.last_mut().unwrap()
        }
    };
    entry.1 += 1;
    if entry.1 <= 5 {
        println!("W2 foma-panic: {label} rule {} apply-up of {up_input:?} (case input {down_input:?}) UNCHECKED", rule.xml_id);
    }
    if first {
        let path = std::env::var("PG_W2_PANIC_NET").unwrap_or_else(|_| format!("{}/w2-foma-panic.att", std::env::temp_dir().display()));
        if let Ok(mut f) = std::fs::File::create(&path) {
            let _ = foma::io::net_print_att(&foma::options::FomaOptions::default(), net, &mut f);
            println!("W2 foma-panic-net: first panic net written to {path}");
        }
    }
}

/// Runs HC's single-rule synthesis and the rule's net on `string`; `Ok(applied)` when the net relates the input to HC's output, `Err` describing the miss otherwise. `None` when an input node has no token in the net's closure. The relation is tested by applying the net upward from HC's output (an epenthesis net has unboundedly many outputs for one input, but finitely many inputs for one output).
fn net_check(ctx: &NetCtx, rule: &RewriteRuleDef, net: &foma::types::Fsm, handle: &mut foma::types::ApplyHandle, string: &[BlockNode]) -> Option<Result<bool, String>> {
    let input: Vec<Vec<char>> = string.iter().map(|n| ctx.tokens(n)).collect();
    if input.iter().any(|t| t.is_empty()) {
        return None;
    }
    let texts: Vec<String> = {
        // Every input token choice (cap 8): the net may know the node under any covering row.
        let mut acc: Vec<String> = vec![String::new()];
        for t in &input {
            let mut next = Vec::new();
            for a in &acc {
                for c in t.iter().take(2) {
                    let mut x = a.clone();
                    x.push(*c);
                    next.push(x);
                }
            }
            acc = next;
            if acc.len() > 8 {
                acc.truncate(8);
            }
        }
        acc
    };
    let shape = shape_of(ctx.g, string);
    let result = synthesize(ctx.g, rule, &shape);
    let applied = !result.is_empty();
    let hc: Vec<Vec<char>> = match result.first() {
        Some(out) => out
            .interior()
            .map(|(j, kind, cd, _)| {
                let node = if kind == NodeKind::Boundary {
                    BlockNode::Boundary(CharDefId(cd))
                } else {
                    BlockNode::Segment { lanes: out.node_lanes(j).to_vec(), cd }
                };
                ctx.tokens(&node)
            })
            .collect(),
        None => input.iter().map(|t| t.iter().copied().take(2).collect()).collect(),
    };
    if hc.iter().any(|t| t.is_empty()) {
        return Some(Err(format!("HC image of {:?} has no token in the net's closure", texts[0])));
    }
    // Every output spelling HC may be read as (cap 64).
    let mut outs: Vec<String> = vec![String::new()];
    for set in &hc {
        let mut next = Vec::new();
        for o in &outs {
            for c in set.iter().take(4) {
                let mut x = o.clone();
                x.push(*c);
                next.push(x);
            }
        }
        outs = next;
        if outs.len() > 64 {
            outs.truncate(64);
        }
    }
    for out in &outs {
        let mut n = 0;
        // foma 0.4.2's apply-up can overflow `ipos` on some nets; that is counted, not hidden.
        let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for inp in handle.up(out) {
                n += 1;
                if texts.iter().any(|t| *t == inp) {
                    return true;
                }
                if n > 20000 {
                    break;
                }
            }
            false
        }));
        match found {
            Ok(true) => return Some(Ok(applied)),
            Ok(false) => {}
            Err(_) => {
                record_panic(ctx.label, rule, net, out, &texts[0]);
                return None;
            }
        }
    }
    Some(Err(format!("HC output {:?} for input {:?} (applied={applied}) is not related by the net", outs[0], texts[0])))
}

/// Rule-application differential (strings HC applied the rule to: foci with their environments) and whole-rule differential (strings of rows and boundaries up to length 4): HC's single-rule output must be among the reused net's outputs.
fn net_differentials(whole: bool, g: &Grammar, label: &str, tally: &mut Tally, rules: &[&RewriteRuleDef], elems: &[Element], applying: &std::collections::HashMap<String, Vec<Vec<BlockNode>>>) {
    let prules: Vec<&PhonRuleDef> = g.prules.iter().filter(|r| matches!(r, PhonRuleDef::Rewrite(_))).collect();
    if prules.is_empty() {
        return;
    }
    let opts = foma::options::FomaOptions::default();
    let parts = match CascadeParts::build(&opts, g, &prules) {
        Ok(p) => p,
        Err(e) => {
            tally.net_refused += 1;
            println!("W2 net-unbuildable: {label}: {e:?}");
            return;
        }
    };
    let ctx = NetCtx {
        label, g, parts: &parts };
    for rule in rules {
        let Some(part) = parts.rules.iter().find(|p| p.rule == rule.xml_id) else { continue };
        let mut handle = foma::apply::apply_init(&part.net);
        for (kind, strings) in [("rule-application", if whole { Vec::new() } else { applying.get(&rule.xml_id).cloned().unwrap_or_default() })] {
            let stat = tally.nets.entry(kind.to_owned()).or_default();
            for s in &strings {
                match net_check(&ctx, rule, &part.net, &mut handle, s) {
                    None => stat.2 += 1,
                    Some(Ok(a)) => {
                        stat.0 += 1;
                        stat.1 += u64::from(a);
                    }
                    Some(Err(e)) => {
                        stat.0 += 1;
                        tally.net_misses.push(format!("{label} {kind} rule {}: {e}", rule.xml_id));
                    }
                }
            }
        }
        if !whole {
            continue;
        }
        let is_deletion = !rule.lhs.nodes.is_empty() && rule.subrules.iter().all(|sub| sub.rhs.nodes.is_empty());
        match std::env::var("PG_W2_RULE_KIND").as_deref() {
            Ok("deletion") if !is_deletion => continue,
            Ok("other") if is_deletion => continue,
            _ => {}
        }
        // Whole-rule: strings of length 1..=4.
        let e = elems.len() as u64;
        let total: u64 = (1..=4u32).map(|l| e.saturating_pow(l)).fold(0u64, |a, b| a.saturating_add(b));
        let exhaustive = total <= EXHAUSTIVE;
        let cases = if exhaustive { total } else { sample() };
        if !exhaustive {
            println!("W2 {label} whole-rule {}: sampled {cases} of {total} strings, seed {SEED:#x}", rule.xml_id);
        }
        let mut rng = Rng(SEED ^ (rule.xml_id.len() as u64) ^ 0xabcd);
        let (mut ran, mut hit, mut skipped) = (0u64, 0u64, 0u64);
        let mut misses = Vec::new();
        for case in 0..cases {
            let mut idxs: Vec<usize> = Vec::new();
            if exhaustive {
                let mut c = case;
                let mut len = 1u32;
                while c >= e.pow(len) {
                    c -= e.pow(len);
                    len += 1;
                }
                for _ in 0..len {
                    idxs.push((c % e) as usize);
                    c /= e;
                }
            } else {
                let len = 1 + rng.next() % 4;
                for _ in 0..len {
                    idxs.push((rng.next() % e) as usize);
                }
            }
            let string: Vec<BlockNode> = idxs.iter().map(|&i| elems[i].node.clone()).collect();
            match net_check(&ctx, rule, &part.net, &mut handle, &string) {
                None => skipped += 1,
                Some(Ok(a)) => {
                    ran += 1;
                    hit += u64::from(a);
                }
                Some(Err(m)) => {
                    ran += 1;
                    misses.push(format!("{label} whole-rule rule {}: {m}", rule.xml_id));
                    if misses.len() > 5 {
                        break;
                    }
                }
            }
        }
        let stat2 = tally.nets.entry("whole-rule".to_owned()).or_default();
        stat2.0 += ran;
        stat2.1 += hit;
        stat2.2 += skipped;
        tally.net_misses.extend(misses);
    }
}

fn check_grammar(whole: bool, label: &str, g: &Grammar, tally: &mut Tally) {
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
        "W2 {label}: rows attested={} private={} total={}/{} epsilon={} modify_private={} empty_twins={}",
        report.attested_rows, report.private_rows, report.total_rows, report.token_cap, report.epsilon_rows, report.private_modify_rows, report.empty_twin_rows
    );
    let table = &g.char_tables[0];
    let elems = elements(g, &closure);
    if whole {
        net_differentials(true, g, label, tally, &rules, &elems, &std::collections::HashMap::new());
        return;
    }
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
    let mut applying: std::collections::HashMap<String, Vec<Vec<BlockNode>>> = std::collections::HashMap::new();
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
        let cases = if exhaustive { total } else { sample() };
        if !exhaustive {
            tally.sampled_rules += 1;
            println!("W2 {label} rule {}: sampled {cases} of {total} tuples, seed {SEED:#x}", rule.xml_id);
        }
        let mut rng = Rng(SEED ^ (rule.xml_id.len() as u64));
        let mut foci: Vec<Vec<BlockNode>> = Vec::new();
        let rule_id = rule.xml_id.clone();
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
                let slot = applying.entry(rule_id.clone()).or_default();
                if slot.len() < 300 {
                    slot.push(focus.clone());
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
                    let hit = !synthesize(g, &one, &shape_of(g, &nodes)).is_empty();
                    if hit {
                        let slot = applying.entry(rule.xml_id.clone()).or_default();
                        if slot.len() < 600 {
                            slot.push(nodes);
                        }
                    }
                    hit
                });
            }
        }
    }
    spelling_differential(g, &closure, tally, label);
    net_differentials(false, g, label, tally, &rules, &elems, &applying);
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

fn run(whole: bool) {
    let scope = if std::env::var_os("HC_CONFORMANCE_SUITE").is_some()
        || pg_conformance_fixtures::suite_root().exists()
    {
        pg_conformance_fixtures::ConformanceScope::All
    } else {
        pg_conformance_fixtures::ConformanceScope::Local
    };
    println!("W2 scope {} exhaustive<={EXHAUSTIVE} sample={} seed={SEED:#x}", scope.label(), sample());
    let mut tally = Tally::default();
    for f in pg_conformance_fixtures::discover_scoped(scope) {
        let xml = f.load_grammar_xml();
        let Ok(g) = pg_grammar::load(&xml) else { continue };
        check_grammar(whole, &f.label(), &g, &mut tally);
    }
    println!(
        "W2 summary: grammars={} refused={} rules={} skipped_lhs={} cases={} matched={} sampled_rules={} under={} missing_images={}",
        tally.grammars, tally.refused.len(), tally.rules, tally.skipped_nodes, tally.cases, tally.matched,
        tally.sampled_rules, tally.under.len(), tally.missing_images.len()
    );
    for (k, (cases, hit, probes, sampled)) in &tally.kinds {
        println!("W2 matcher {k}: probes={probes} sampled={sampled} cases={cases} matched={hit}");
    }
    println!("W2 spelling: cases={} capped_rows={} empty_twin_rows_skipped={} gaps={}", tally.spelling_cases, tally.spelling_capped, tally.spelling_twins, tally.spelling_private_count);
    for m in tally.spelling_private_gaps.iter().take(5) {
        println!("W2 private-gap {m}");
    }
    for (k, (ran, hit, skipped)) in &tally.nets {
        println!("W2 net {k}: strings={ran} applied={hit} no_token_skipped={skipped}");
    }
    println!("W2 net: grammars_with_unbuildable_cascade={} misses={} unchecked_foma_apply_up_overflow={}", tally.net_refused, tally.net_misses.len(), APPLY_PANICS.load(std::sync::atomic::Ordering::Relaxed));
    for m in &tally.net_misses {
        println!("W2 net-miss {m}");
    }
    println!("W2 skipped: env={} other={}", tally.skipped_env, tally.skipped_other);
    for r in &tally.refused {
        println!("W2 refused {r}");
    }
    assert!(tally.net_misses.is_empty(), "net outputs miss synthesis: {:#?}", &tally.net_misses[..tally.net_misses.len().min(10)]);
    assert!(tally.grammars > 0, "no grammar reached the gate");
    assert!(tally.under.is_empty(), "under-inclusions: {:#?}", &tally.under[..tally.under.len().min(20)]);
    assert!(tally.missing_images.is_empty(), "missing images: {:#?}", &tally.missing_images[..tally.missing_images.len().min(20)]);
}

/// Membership (rewrite foci, environments, affix lhs, metathesis), images, spelling and rule-application differentials.
#[test]
fn class_membership_and_images_are_a_superset_of_synthesis() {
    run(false);
}

/// Whole-rule differential: HC's single-rule synthesis output is related by the reused net, strings of closure rows and boundaries up to length 4 (exhaustive to 1,000,000 per rule, 100,000 sampled above). Slow (about 90 minutes over every grammar), so it is its own test.
#[test]
#[ignore = "red on known deletion misses owned by W6 (reports/W2.md); run explicitly with --ignored; the named expected-miss list is not yet written"]
fn whole_rule_differential_nets_contain_synthesis() {
    run(true);
}

/// Names every grammar whose cascade cannot be built, with the refusal: those grammars are UNCHECKED by the net differentials (membership, spelling and image checks still cover them).
#[test]
fn unbuildable_cascades_are_named() {
    let scope = if std::env::var_os("HC_CONFORMANCE_SUITE").is_some() || pg_conformance_fixtures::suite_root().exists() {
        pg_conformance_fixtures::ConformanceScope::All
    } else {
        pg_conformance_fixtures::ConformanceScope::Local
    };
    let opts = foma::options::FomaOptions::default();
    let mut n = 0;
    for f in pg_conformance_fixtures::discover_scoped(scope) {
        let xml = f.load_grammar_xml();
        let Ok(g) = pg_grammar::load(&xml) else { continue };
        let prules: Vec<&PhonRuleDef> = g.prules.iter().filter(|r| matches!(r, PhonRuleDef::Rewrite(_))).collect();
        if prules.is_empty() {
            continue;
        }
        if let Err(e) = CascadeParts::build(&opts, &g, &prules) {
            n += 1;
            println!("W2 net-unbuildable: {}: {e:?}", f.label());
        }
    }
    println!("W2 net-unbuildable total={n}");
}
