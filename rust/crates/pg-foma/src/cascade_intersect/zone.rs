//! Zone layers: the edge-only affix rules of one stratum (plan 2.4.2, 2.4.5).
//!
//! An allomorph is **edge-only** when its right-hand side is `inserts · Copy(0) … Copy(n-1) ·
//! inserts`. With the left-hand side's conditions dropped (M2) one application is `x -> p x' q`,
//! `x'` = `x` with any boundaries dropped. A zone layer reads its input as the identity with
//! boundary keep-or-drop and adds prefix blocks on the left and suffix blocks on the right:
//!
//! - **Ordered chain** (a Linear stratum): reading outward from the stem, rule positions are
//!   nondecreasing and each rule applies at most `max_apps` times (an unrolled counted repeat up
//!   to [`MAX_UNROLL`]; above that a plain loop, which is a superset). The prefix chain is read
//!   outermost first, so its positions descend; the suffix chain ascends. The positions are the
//!   stratum's rule list in declared order, then the slots of its templates (one optional pass,
//!   each slot once, any of its rules).
//! - **Unordered pool**: `Pre* · id · Suf*` over every edge allomorph of every rule of the stratum.
//!
//! Circumfix halves are not paired (a superset): each half is a prefix or suffix block with its
//! own tag. A null allomorph is a tagged epsilon self-loop arc (no state, plan 2.0). Start-state
//! epsilon moves are removed at build by copying arcs (no subset construction), so the start state
//! contains the identity and is final.

use std::collections::BTreeSet;

use pg_foma_runtime::cascade::analyzer::rule_tag;
use pg_foma_runtime::cascade::{
    BuildMeter, BuildRefused, Input, NfaBuilder, NfaLayer, Output, Tag, TokenSet,
};
use pg_grammar::model::{AffixAllomorphDef, MRuleId, OutputAction, PartRef, SegmentedText};

pub const MAX_UNROLL: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteriorKind {
    Modify,
    InsertContext,
    /// Some part of the left-hand side is not copied (and the copies are otherwise in order).
    Truncation,
    /// Infix, reduplication, reordering or no copy at all.
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlloShape {
    Edge { pre: Vec<u32>, suf: Vec<u32> },
    Interior(InteriorKind),
}

fn insert_tokens(shape: &SegmentedText) -> Vec<u32> {
    shape.shape.interior().map(|(_, _, cd, _)| cd).collect()
}

pub fn classify(a: &AffixAllomorphDef) -> AlloShape {
    if a.rhs.iter().any(|x| matches!(x, OutputAction::Modify(..))) {
        return AlloShape::Interior(InteriorKind::Modify);
    }
    if a.rhs
        .iter()
        .any(|x| matches!(x, OutputAction::InsertContext(_)))
    {
        return AlloShape::Interior(InteriorKind::InsertContext);
    }
    let n = a.lhs.len();
    let (mut pre, mut suf) = (Vec::new(), Vec::new());
    let mut copies: Vec<u16> = Vec::new();
    // 0 = leading inserts, 1 = copy run, 2 = trailing inserts
    let mut phase = 0;
    for action in &a.rhs {
        match action {
            OutputAction::InsertSegments { shape, .. } => {
                let t = insert_tokens(shape);
                match phase {
                    0 => pre.extend(t),
                    _ => {
                        phase = 2;
                        suf.extend(t);
                    }
                }
            }
            OutputAction::Copy(PartRef::Input(k)) => {
                if phase == 2 {
                    return AlloShape::Interior(InteriorKind::Other);
                }
                phase = 1;
                copies.push(*k);
            }
            OutputAction::Copy(_) => return AlloShape::Interior(InteriorKind::Other),
            OutputAction::Modify(..) | OutputAction::InsertContext(_) => unreachable!(),
        }
    }
    let in_order = copies.windows(2).all(|w| w[0] < w[1]);
    if !in_order || copies.iter().any(|&k| k as usize >= n) {
        return AlloShape::Interior(InteriorKind::Other);
    }
    if copies.len() == n && n > 0 {
        return AlloShape::Edge { pre, suf };
    }
    if !copies.is_empty() {
        return AlloShape::Interior(InteriorKind::Truncation);
    }
    AlloShape::Interior(InteriorKind::Other)
}

/// One way a rule can act at a position.
#[derive(Clone, Debug)]
pub struct Alt {
    pub rule: MRuleId,
    pub allomorph: usize,
    pub pre: Vec<u32>,
    pub suf: Vec<u32>,
}

/// A rule (or a template slot) in a chain: applied `copies` times at most; `looping` replaces
/// the unrolled copies by a plain loop.
#[derive(Clone, Debug)]
pub struct Position {
    pub copies: u32,
    pub looping: bool,
    pub alts: Vec<Alt>,
}

impl Position {
    pub fn new(max_apps: u32, alts: Vec<Alt>) -> Self {
        Position {
            copies: max_apps.clamp(1, MAX_UNROLL),
            looping: max_apps > MAX_UNROLL,
            alts,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneKind {
    Ordered,
    Pool,
}

pub struct ZoneBuilt {
    pub layer: NfaLayer,
    /// `(rule, allomorph index)` of every alternative placed in this zone.
    pub placed: BTreeSet<(MRuleId, usize)>,
}

struct SArc {
    from: usize,
    input: Input,
    output: Output,
    to: usize,
    tags: Vec<Tag>,
}

#[derive(Default)]
struct Skel {
    finals: Vec<bool>,
    arcs: Vec<SArc>,
    eps: Vec<(usize, usize)>,
}

impl Skel {
    fn state(&mut self, is_final: bool) -> usize {
        self.finals.push(is_final);
        self.finals.len() - 1
    }

    fn arc(&mut self, from: usize, input: Input, output: Output, to: usize, tags: &[Tag]) {
        self.arcs.push(SArc {
            from,
            input,
            output,
            to,
            tags: tags.to_vec(),
        });
    }

    /// Emits `tokens` from `from` to `to`, the tag on the first arc.
    fn path(&mut self, from: usize, to: usize, tokens: &[u32], tag: Tag) {
        let mut cur = from;
        for (k, &t) in tokens.iter().enumerate() {
            let last = k + 1 == tokens.len();
            let next = if last { to } else { self.state(false) };
            let tags: &[Tag] = if k == 0 { &[tag] } else { &[] };
            self.arc(cur, Input::Eps, Output::Tok(t), next, tags);
            cur = next;
        }
    }

    fn closure(&self, s: usize) -> Vec<usize> {
        let mut seen = vec![false; self.finals.len()];
        let mut stack = vec![s];
        seen[s] = true;
        let mut out = Vec::new();
        while let Some(x) = stack.pop() {
            out.push(x);
            for &(a, b) in &self.eps {
                if a == x && !seen[b] {
                    seen[b] = true;
                    stack.push(b);
                }
            }
        }
        out
    }
}

fn is_null(a: &Alt) -> bool {
    a.pre.is_empty() && a.suf.is_empty()
}

pub fn build_zone(
    name: &str,
    universe: u32,
    boundaries: &[u32],
    kind: ZoneKind,
    positions: &[Position],
    meter: &mut BuildMeter,
) -> Result<ZoneBuilt, BuildRefused> {
    let mut b = NfaBuilder::new(name, universe, meter);
    let all = b.add_set(TokenSet::full(universe))?;
    let bset = if boundaries.is_empty() {
        None
    } else {
        Some(b.add_set(TokenSet::from_tokens(universe, boundaries))?)
    };
    let mut sk = Skel::default();
    let mut placed = BTreeSet::new();
    let start = sk.state(false);
    let ident = sk.state(true);
    sk.arc(ident, Input::Class(all), Output::Same, ident, &[]);
    if let Some(bs) = bset {
        sk.arc(ident, Input::Class(bs), Output::Eps, ident, &[]);
    }
    for p in positions {
        for a in &p.alts {
            placed.insert((a.rule, a.allomorph));
        }
    }
    match kind {
        ZoneKind::Pool => {
            let suffix = sk.state(true);
            sk.eps.push((start, ident));
            sk.eps.push((ident, suffix));
            for p in positions {
                for a in &p.alts {
                    let tag = rule_tag(a.rule);
                    if is_null(a) {
                        sk.arc(start, Input::Eps, Output::Eps, start, &[tag]);
                    }
                    if !a.pre.is_empty() {
                        sk.path(start, start, &a.pre, tag);
                    }
                    if !a.suf.is_empty() {
                        sk.path(suffix, suffix, &a.suf, tag);
                    }
                }
            }
        }
        ZoneKind::Ordered => {
            let mut cur = start;
            for p in positions.iter().rev() {
                let reps = if p.looping { 1 } else { p.copies };
                for _ in 0..reps {
                    let next = sk.state(false);
                    sk.eps.push((cur, next));
                    for a in &p.alts {
                        let tag = rule_tag(a.rule);
                        if is_null(a) {
                            sk.arc(cur, Input::Eps, Output::Eps, cur, &[tag]);
                        } else if !a.pre.is_empty() {
                            let to = if p.looping { cur } else { next };
                            sk.path(cur, to, &a.pre, tag);
                        }
                    }
                    cur = next;
                }
            }
            sk.eps.push((cur, ident));
            let mut r = sk.state(true);
            sk.eps.push((ident, r));
            for p in positions {
                let reps = if p.looping { 1 } else { p.copies };
                for _ in 0..reps {
                    let next = sk.state(true);
                    sk.eps.push((r, next));
                    for a in &p.alts {
                        if !a.suf.is_empty() {
                            let to = if p.looping { r } else { next };
                            sk.path(r, to, &a.suf, rule_tag(a.rule));
                        }
                    }
                    r = next;
                }
            }
        }
    }
    emit(sk, start, &mut b)?;
    Ok(ZoneBuilt {
        layer: b.finish(),
        placed,
    })
}

/// Epsilon-removes the skeleton by copying closure arcs (no subset construction).
fn emit(sk: Skel, start: usize, b: &mut NfaBuilder) -> Result<(), BuildRefused> {
    let mut by_from: Vec<Vec<usize>> = vec![Vec::new(); sk.finals.len()];
    for (i, a) in sk.arcs.iter().enumerate() {
        by_from[a.from].push(i);
    }
    let closures: Vec<Vec<usize>> = (0..sk.finals.len()).map(|s| sk.closure(s)).collect();
    let mut ids: Vec<Option<u32>> = vec![None; sk.finals.len()];
    let mut queue = Vec::new();
    let intern = |s: usize,
                  ids: &mut Vec<Option<u32>>,
                  b: &mut NfaBuilder,
                  queue: &mut Vec<usize>|
     -> Result<u32, BuildRefused> {
        if let Some(id) = ids[s] {
            return Ok(id);
        }
        let fin = closures[s].iter().any(|&c| sk.finals[c]);
        let id = b.add_state(fin)?;
        ids[s] = Some(id);
        queue.push(s);
        Ok(id)
    };
    let start_id = intern(start, &mut ids, b, &mut queue)?;
    b.set_start(start_id);
    while let Some(s) = queue.pop() {
        let from = ids[s].expect("queued states have ids");
        for &c in &closures[s] {
            for &ai in &by_from[c] {
                let a = &sk.arcs[ai];
                let to = intern(a.to, &mut ids, b, &mut queue)?;
                b.add_arc(from, a.input, a.output, to, &a.tags, 0, 0)?;
            }
        }
    }
    Ok(())
}
