//! Decoupled pattern compiler: a minimal local pattern description → Thompson NFA → optimized
//! CSR FST. This crate deliberately does **not** depend on `pg-grammar`; the M3 loader will
//! translate `model::Pattern`/`PatternNode` into `CompileInput`.
//!
//! The node set is exactly what `PatternNode` needs (plan §5.4): single-segment constraints,
//! concatenation (a node sequence), alternation, capture groups, and min/max quantifiers
//! including the unbounded (Kleene) case. Anchoring is expressed by the `start_anchor`/`end_anchor`
//! flags on `crate::traverse::Transduce` exactly as C# `Matcher` passes
//! `AnchoredToStart`/`AnchoredToEnd` to `Fst.Transduce` — that is how `PatternNode::Anchor`
//! surfaces, so no anchor node kind is needed here.

use crate::nfa::Nfa;
use crate::AcceptInfo;

pub const ENTIRE_MATCH: &str = "*entire*";

/// One node of a pattern to compile. `Constraint` lanes are canonical `u64` symbolic-feature
/// lanes (§5.3); an empty lane slice is the "matches any segment" constraint.
#[derive(Clone, Debug)]
pub enum CompileNode {
    /// A single-segment arc constraint (match = `pg_featstruct::flat_unifiable`).
    Constraint(Vec<u64>),
    /// A named capture group around a child sequence (C# `CreateTag` start/end).
    Group {
        name: String,
        children: Vec<CompileNode>,
    },
    /// `(alt0 | alt1 | ...)` — each alternative is its own node sequence.
    Alternation(Vec<Vec<CompileNode>>),
    /// `{min,max}` repetition of a child sequence; `max == None` is unbounded (Kleene), `min == 0` makes it optional.
    Quantifier {
        min: u32,
        max: Option<u32>,
        children: Vec<CompileNode>,
    },
}

/// A whole pattern to compile into one FST. A single accepting alternative (id + priority),
/// mirroring one `Matcher` pattern; `deterministic` selects `Determinize()` vs `EpsilonRemoval()`
/// exactly as `Matcher.Compile` does (`!AllSubmatches && !hasVariables && !Nondeterministic`).
#[derive(Clone, Debug)]
pub struct CompileInput {
    pub nodes: Vec<CompileNode>,
    pub accept_id: Option<String>,
    pub accept_priority: i32,
    pub deterministic: bool,
}

impl CompileInput {
    pub fn new(nodes: Vec<CompileNode>) -> Self {
        CompileInput {
            nodes,
            accept_id: None,
            accept_priority: 0,
            deterministic: true,
        }
    }

    pub fn deterministic(mut self, det: bool) -> Self {
        self.deterministic = det;
        self
    }

    pub fn accept(mut self, id: Option<String>, priority: i32) -> Self {
        self.accept_id = id;
        self.accept_priority = priority;
        self
    }

    /// Build the NFA (with the implicit `*entire*` capture group around the whole pattern, exactly
    /// as `Matcher.GeneratePatternNfa` does), returning it with arc priorities marked.
    pub fn build_nfa(&self) -> Nfa {
        let mut nfa = Nfa::new();
        let start = nfa.start;
        // Wrap the whole pattern in the *entire* capture group (Matcher.cs:148-150).
        let s1 = nfa.create_tag(start, ENTIRE_MATCH, true);
        let s2 = gen_seq(&mut nfa, s1, &self.nodes);
        let s3 = nfa.create_tag(s2, ENTIRE_MATCH, false);
        let acc = nfa.create_accepting_state(AcceptInfo {
            id: self.accept_id.clone(),
            priority: self.accept_priority,
        });
        nfa.add_epsilon(s3, acc);
        nfa.mark_arc_priorities();
        nfa
    }
}

/// Generate the concatenation of a node sequence, returning the end state.
fn gen_seq(nfa: &mut Nfa, mut cur: usize, nodes: &[CompileNode]) -> usize {
    for node in nodes {
        cur = gen_node(nfa, cur, node);
    }
    cur
}

fn gen_node(nfa: &mut Nfa, source: usize, node: &CompileNode) -> usize {
    match node {
        CompileNode::Constraint(lanes) => {
            let t = nfa.create_state();
            nfa.add_input(source, t, crate::lanes::canonicalize(lanes.clone()));
            t
        }
        CompileNode::Group { name, children } => {
            let s = nfa.create_tag(source, name, true);
            let e = gen_seq(nfa, s, children);
            nfa.create_tag(e, name, false)
        }
        CompileNode::Alternation(alts) => {
            let merge = nfa.create_state();
            for alt in alts {
                let bs = nfa.create_state();
                nfa.add_epsilon(source, bs);
                let be = gen_seq(nfa, bs, alt);
                nfa.add_epsilon(be, merge);
            }
            merge
        }
        CompileNode::Quantifier { min, max, children } => {
            gen_quantifier(nfa, source, *min, *max, children)
        }
    }
}

/// Compile a `{min,max}` quantifier: mandatory copies are concatenated, optional copies each get a skip epsilon to the shared exit, unbounded adds a back-epsilon loop.
fn gen_quantifier(
    nfa: &mut Nfa,
    source: usize,
    min: u32,
    max: Option<u32>,
    children: &[CompileNode],
) -> usize {
    let mut cur = source;
    for _ in 0..min {
        cur = gen_seq(nfa, cur, children);
    }
    match max {
        None => {
            // Kleene tail: `cur` is both a possible exit and the loop entry; continuation arcs added after `cur` exit the loop.
            let body_end = gen_seq(nfa, cur, children);
            nfa.add_epsilon(body_end, cur);
            cur
        }
        Some(max) => {
            let extra = max.saturating_sub(min);
            if extra == 0 {
                return cur;
            }
            let exit = nfa.create_state();
            for _ in 0..extra {
                nfa.add_epsilon(cur, exit); // stop here (skip remaining optional copies)
                cur = gen_seq(nfa, cur, children);
            }
            nfa.add_epsilon(cur, exit);
            exit
        }
    }
}
