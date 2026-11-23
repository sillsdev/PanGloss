//! Recipe T: one application of one affix allomorph as a one-way transducer (plan 2.4.1).
//!
//! The left-hand side `P_0 .. P_{n-1}` is read left to right with boundaries skippable
//! everywhere. A subsequence of the right-hand side's `Copy` actions with strictly increasing part
//! indices is *faithful*: its part is copied token by token. Every other part is read and
//! discarded (refused when unbounded: its discard is unconstrained by the surface), and every
//! other `Copy` emits any string of the part's token language. `InsertSegments` emits its tokens.
//! Every boundary token read is copied or dropped, independently. Conditions on the allomorph
//! (environments, alpha variables, anchors) are dropped (M2): the machine only ever accepts more.
//!
//! `Modify` and `InsertContext` need rows the closure does not intern yet and stay refused by name
//! (see [`super::refusal`]); so does a part copied twice (reduplication, later work).

use std::collections::BTreeSet;

use pg_grammar::chardef::{CharDefId, CharDefKind};
use pg_grammar::model::{AffixAllomorphDef, Grammar, OutputAction, PartRef, PatternNode, TableId};

use super::refusal::{construct, unbounded, Refusal};
use crate::lower::JointClosure;

/// Unrolled repeat bound; a larger authored maximum becomes a loop (a superset).
const MAX_UNROLL: u32 = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum In {
    Eps,
    /// Any token of the sorted set.
    Set(Vec<u32>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Out {
    Eps,
    Same,
    Tok(u32),
}

#[derive(Clone, Debug)]
pub struct RArc {
    pub from: usize,
    pub input: In,
    pub output: Out,
    pub to: usize,
}

/// An epsilon-free machine: arcs may still read nothing (they write a token), but no state-level
/// epsilon move remains, so the closure of the start state is the start state.
#[derive(Clone, Debug)]
pub struct Machine {
    pub start: usize,
    pub finals: Vec<bool>,
    pub arcs: Vec<RArc>,
}

impl Machine {
    /// Whether some accepting path copies every token it reads and writes nothing else: an
    /// application that leaves no non-faithful arc and so no instance (plan S.4 e).
    pub fn wholly_faithful(&self) -> bool {
        let mut seen = vec![false; self.finals.len()];
        let mut stack = vec![self.start];
        seen[self.start] = true;
        while let Some(s) = stack.pop() {
            if self.finals[s] {
                return true;
            }
            for a in self.arcs.iter().filter(|a| a.from == s) {
                if matches!(a.input, In::Set(_)) && a.output == Out::Same && !seen[a.to] {
                    seen[a.to] = true;
                    stack.push(a.to);
                }
            }
        }
        false
    }
}

/// What the recipe needs from the grammar: the tokens a node stands for, and the boundaries.
pub struct RecipeCtx<'g> {
    pub g: &'g Grammar,
    closure: JointClosure,
    pub boundaries: Vec<u32>,
}

impl<'g> RecipeCtx<'g> {
    pub fn new(g: &'g Grammar) -> Result<Self, Refusal> {
        let closure = JointClosure::build(g, TableId(0), &[]).map_err(|e| {
            Refusal::new(
                construct::INTERIOR,
                format!("the row closure is unavailable: {e:?}"),
            )
        })?;
        let boundaries = closure
            .boundary_ids(g)
            .into_iter()
            .map(|c| c.0)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok(RecipeCtx {
            g,
            closure,
            boundaries,
        })
    }

    fn is_boundary(&self, id: CharDefId) -> bool {
        self.g.char_tables[0].get(id).kind() == CharDefKind::Boundary
    }

    /// The tokens of a `Context` or `CharDef` node; `None` for a boundary node.
    fn members(&self, node: &PatternNode) -> Option<Vec<u32>> {
        if let PatternNode::CharDef(id) = node {
            if self.is_boundary(*id) {
                return None;
            }
        }
        let mut v: Vec<u32> = self
            .closure
            .node_members(self.g, node)?
            .into_iter()
            .map(|c| c.0)
            .collect();
        v.sort_unstable();
        v.dedup();
        Some(v)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Faithful,
    Discard,
    Emit,
}

#[derive(Default)]
struct Sk {
    finals: Vec<bool>,
    arcs: Vec<RArc>,
    eps: Vec<(usize, usize)>,
}

impl Sk {
    fn state(&mut self) -> usize {
        self.finals.push(false);
        self.finals.len() - 1
    }

    fn arc(&mut self, from: usize, input: In, output: Out, to: usize) {
        self.arcs.push(RArc {
            from,
            input,
            output,
            to,
        });
    }

    /// One arc over `set` in `mode`.
    fn set_arc(&mut self, from: usize, to: usize, set: &[u32], mode: Mode) {
        if set.is_empty() {
            return;
        }
        match mode {
            Mode::Faithful => self.arc(from, In::Set(set.to_vec()), Out::Same, to),
            Mode::Discard => self.arc(from, In::Set(set.to_vec()), Out::Eps, to),
            Mode::Emit => {
                for &t in set {
                    self.arc(from, In::Eps, Out::Tok(t), to);
                }
            }
        }
    }

    fn nodes(
        &mut self,
        ctx: &RecipeCtx,
        nodes: &[PatternNode],
        mut cur: usize,
        mode: Mode,
        touched: &mut Vec<usize>,
    ) -> usize {
        touched.push(cur);
        for node in nodes {
            cur = self.node(ctx, node, cur, mode, touched);
            touched.push(cur);
        }
        cur
    }

    fn node(
        &mut self,
        ctx: &RecipeCtx,
        node: &PatternNode,
        cur: usize,
        mode: Mode,
        touched: &mut Vec<usize>,
    ) -> usize {
        match node {
            PatternNode::Context(_) | PatternNode::CharDef(_) => match ctx.members(node) {
                Some(set) => {
                    let to = self.state();
                    self.set_arc(cur, to, &set, mode);
                    to
                }
                // A boundary node: boundaries are skippable anyway.
                None => cur,
            },
            PatternNode::Anchor(_) => cur,
            PatternNode::Segments { shape, .. } => {
                let mut at = cur;
                for (_, _, cd, _) in shape.shape.interior() {
                    let n = PatternNode::CharDef(CharDefId(cd));
                    at = self.node(ctx, &n, at, mode, touched);
                    touched.push(at);
                }
                at
            }
            PatternNode::Quantifier { min, max, children } => {
                if children.is_empty() {
                    return cur;
                }
                let single = match children.as_slice() {
                    [n @ (PatternNode::Context(_) | PatternNode::CharDef(_))] => ctx.members(n),
                    _ => None,
                };
                let min = (*min).min(MAX_UNROLL);
                let max = max.filter(|m| *m <= MAX_UNROLL);
                let mut at = cur;
                for _ in 0..min {
                    at = self.nodes(ctx, children, at, mode, touched);
                }
                match max {
                    None => {
                        match &single {
                            Some(set) => self.set_arc(at, at, set, mode),
                            None => {
                                let e = self.nodes(ctx, children, at, mode, touched);
                                self.eps.push((e, at));
                            }
                        }
                        at
                    }
                    Some(m) => {
                        let end = self.state();
                        touched.push(end);
                        for _ in min..m.max(min) {
                            self.eps.push((at, end));
                            at = self.nodes(ctx, children, at, mode, touched);
                        }
                        self.eps.push((at, end));
                        end
                    }
                }
            }
        }
    }

    /// Boundary keep-or-drop (or emission) loops on `states`.
    fn boundary_loops(&mut self, ctx: &RecipeCtx, states: &[usize], mode: Mode) {
        if ctx.boundaries.is_empty() {
            return;
        }
        let mut seen = BTreeSet::new();
        for &s in states {
            if !seen.insert(s) {
                continue;
            }
            match mode {
                Mode::Faithful => {
                    self.arc(s, In::Set(ctx.boundaries.clone()), Out::Same, s);
                    self.arc(s, In::Set(ctx.boundaries.clone()), Out::Eps, s);
                }
                Mode::Discard => self.arc(s, In::Set(ctx.boundaries.clone()), Out::Eps, s),
                Mode::Emit => {
                    for &b in &ctx.boundaries {
                        self.arc(s, In::Eps, Out::Tok(b), s);
                    }
                }
            }
        }
    }

    fn part(&mut self, ctx: &RecipeCtx, nodes: &[PatternNode], from: usize, mode: Mode) -> usize {
        let mut touched = Vec::new();
        let end = self.nodes(ctx, nodes, from, mode, &mut touched);
        touched.push(end);
        self.boundary_loops(ctx, &touched, mode);
        end
    }

    fn insert(&mut self, from: usize, tokens: &[u32]) -> usize {
        let mut cur = from;
        for &t in tokens {
            let next = self.state();
            self.arc(cur, In::Eps, Out::Tok(t), next);
            cur = next;
        }
        cur
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

    /// Removes the state-level epsilon moves by copying arcs, and keeps the reachable states.
    fn finish(self, start: usize) -> Machine {
        let n = self.finals.len();
        let closures: Vec<Vec<usize>> = (0..n).map(|s| self.closure(s)).collect();
        let mut by_from: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, a) in self.arcs.iter().enumerate() {
            by_from[a.from].push(i);
        }
        let mut id: Vec<Option<usize>> = vec![None; n];
        let mut order = vec![start];
        id[start] = Some(0);
        let mut arcs = Vec::new();
        let mut k = 0;
        while k < order.len() {
            let s = order[k];
            let from = k;
            k += 1;
            for &c in &closures[s] {
                for &ai in &by_from[c] {
                    let a = &self.arcs[ai];
                    let to = match id[a.to] {
                        Some(x) => x,
                        None => {
                            id[a.to] = Some(order.len());
                            order.push(a.to);
                            order.len() - 1
                        }
                    };
                    arcs.push(RArc {
                        from,
                        input: a.input.clone(),
                        output: a.output,
                        to,
                    });
                }
            }
        }
        let finals = order
            .iter()
            .map(|&s| closures[s].iter().any(|&c| self.finals[c]))
            .collect();
        Machine {
            start: 0,
            finals,
            arcs,
        }
    }
}

fn insert_tokens(a: &pg_grammar::model::SegmentedText) -> Vec<u32> {
    a.shape.interior().map(|(_, _, cd, _)| cd).collect()
}

/// Per `Copy` in right-hand-side order, whether its part is copied; `Err` when an unbounded part cannot be.
fn faithful_choice(a: &AffixAllomorphDef) -> Result<Vec<bool>, Refusal> {
    let n = a.lhs.len();
    // (action position, part)
    let mut copies: Vec<(usize, usize)> = Vec::new();
    for (p, act) in a.rhs.iter().enumerate() {
        match act {
            OutputAction::Copy(PartRef::Input(k)) if (*k as usize) < n => {
                copies.push((p, *k as usize))
            }
            OutputAction::Copy(_) => {
                return Err(Refusal::new(
                    construct::INTERIOR,
                    "a copy of a part the left-hand side does not have",
                ))
            }
            OutputAction::Modify(..) => {
                return Err(Refusal::new(construct::MODIFY, "Modify action"))
            }
            OutputAction::InsertContext(_) => {
                return Err(Refusal::new(construct::MODIFY, "InsertContext action"))
            }
            OutputAction::InsertSegments { .. } => {}
        }
    }
    let mut parts: Vec<usize> = copies.iter().map(|c| c.1).collect();
    parts.sort_unstable();
    if parts.windows(2).any(|w| w[0] == w[1]) {
        return Err(Refusal::new(
            construct::REDUPLICATION,
            "a part is copied more than once",
        ));
    }
    let ub: Vec<bool> = a.lhs.iter().map(|p| unbounded(&p.nodes)).collect();
    let required: Vec<bool> = copies.iter().map(|&(_, part)| ub[part]).collect();
    // Every unbounded part must be copied, in increasing order.
    for (part, &u) in ub.iter().enumerate() {
        if u && !parts.contains(&part) {
            return Err(Refusal::new(
                construct::UNBOUNDED_TRUNCATION,
                format!("part {part} is unbounded and not copied"),
            ));
        }
    }
    let req_parts: Vec<usize> = copies
        .iter()
        .zip(&required)
        .filter(|(_, r)| **r)
        .map(|(c, _)| c.1)
        .collect();
    if req_parts.windows(2).any(|w| w[0] >= w[1]) {
        return Err(Refusal::new(
            construct::UNBOUNDED_TRUNCATION,
            "unbounded parts are reordered",
        ));
    }
    let mut chosen = vec![false; copies.len()];
    let mut last: Option<usize> = None;
    for i in 0..copies.len() {
        let part = copies[i].1;
        let next_required = (i + 1..copies.len())
            .filter(|&j| required[j])
            .map(|j| copies[j].1)
            .min()
            .unwrap_or(n);
        let fits = last.is_none_or(|l| part > l) && part < next_required;
        if required[i] || fits {
            chosen[i] = true;
            last = Some(part);
        }
    }
    Ok(chosen)
}

/// The refusal for an interior allomorph this recipe does not model, if any (needs no grammar
/// context beyond the allomorph).
pub fn recipe_refusal(a: &AffixAllomorphDef) -> Option<Refusal> {
    faithful_choice(a).err()
}

/// Builds the machine of one allomorph.
pub fn recipe_t(ctx: &RecipeCtx, a: &AffixAllomorphDef) -> Result<Machine, Refusal> {
    let chosen = faithful_choice(a)?;
    let n = a.lhs.len();
    // Action position of the faithful copy of each part.
    let mut faithful_at: Vec<Option<usize>> = vec![None; n];
    let mut ci = 0;
    for (p, act) in a.rhs.iter().enumerate() {
        if let OutputAction::Copy(PartRef::Input(k)) = act {
            if chosen[ci] {
                faithful_at[*k as usize] = Some(p);
            }
            ci += 1;
        }
    }
    let mut sk = Sk::default();
    let start = sk.state();
    let mut cur = start;
    let mut next_action = 0;
    let emit = |sk: &mut Sk, from: usize, upto: usize, next_action: &mut usize| -> usize {
        let mut at = from;
        for p in *next_action..upto {
            match &a.rhs[p] {
                OutputAction::InsertSegments { shape, .. } => {
                    at = sk.insert(at, &insert_tokens(shape));
                }
                OutputAction::Copy(PartRef::Input(k)) => {
                    // A non-faithful copy: any string of the part's token language.
                    at = sk.part(ctx, &a.lhs[*k as usize].nodes, at, Mode::Emit);
                }
                _ => {}
            }
        }
        *next_action = (*next_action).max(upto);
        at
    };
    for (i, pattern) in a.lhs.iter().enumerate() {
        match faithful_at[i] {
            Some(q) => {
                cur = emit(&mut sk, cur, q, &mut next_action);
                next_action = q + 1;
                cur = sk.part(ctx, &pattern.nodes, cur, Mode::Faithful);
            }
            None => cur = sk.part(ctx, &pattern.nodes, cur, Mode::Discard),
        }
    }
    cur = emit(&mut sk, cur, a.rhs.len(), &mut next_action);
    sk.finals[cur] = true;
    Ok(sk.finish(start))
}
