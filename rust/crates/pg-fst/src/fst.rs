//! Frozen CSR finite-state automaton, built by `crate::optimize` and consumed by `crate::traverse`.

use crate::lanes::{flat_subsumes, UNCONSTRAINED};
use crate::{AcceptInfo, Cmd, ConstraintId, Direction};
use pg_featstruct::flat_unifiable;

/// A determinized/epsilon-removed arc input: a positive constraint plus a set of negated constraints (C# `Input`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchInput {
    /// Canonical positive constraint lanes (an empty slice matches any segment).
    pub pos: Vec<u64>,
    pub negated: Vec<Vec<u64>>,
}

impl MatchInput {
    /// C# `Input.Matches`: unifiable with the positive constraint and not unifiable with any negated one.
    #[inline]
    pub fn matches(&self, seg: &[u64]) -> bool {
        flat_unifiable(seg, &self.pos) && self.negated.iter().all(|n| !flat_unifiable(seg, n))
    }

    /// C# `Input.IsSatisfiable` (Input.cs:60-63): no negated constraint subsumes the positive one.
    #[inline]
    pub fn is_satisfiable(&self) -> bool {
        self.negated.iter().all(|n| !flat_subsumes(n, &self.pos))
    }
}

/// A CSR arc: target state, interned constraint, and a range of register commands to run when taken.
#[derive(Copy, Clone, Debug)]
pub struct Arc {
    pub target: u32,
    pub constraint: ConstraintId,
    pub cmd_lo: u32,
    pub cmd_hi: u32,
    /// Arc priority from `MarkArcPriorities`, consumed by the nondeterministic traversal's tiebreak.
    pub priority: i32,
}

/// A CSR state: accept flag plus ranges into the shared `accept_infos`, `commands`, and `arcs` pools.
#[derive(Copy, Clone, Debug)]
pub struct StateMeta {
    pub accepting: bool,
    pub accept_lo: u32,
    pub accept_hi: u32,
    pub fin_lo: u32,
    pub fin_hi: u32,
    pub arc_lo: u32,
    pub arc_hi: u32,
}

/// The frozen automaton.
#[derive(Debug)]
pub struct Fst {
    pub(crate) states: Vec<StateMeta>,
    pub(crate) arcs: Vec<Arc>,
    pub(crate) commands: Vec<Cmd>,
    pub(crate) accept_infos: Vec<AcceptInfo>,
    pub(crate) constraints: Vec<MatchInput>,
    pub(crate) start: u32,
    pub(crate) deterministic: bool,
    pub(crate) direction: Direction,
    pub(crate) register_count: u32,
    /// Group name → start tag (end tag is `+1`), C# `_groups`.
    pub(crate) groups: Vec<(String, i32)>,
    /// Commands run once at the start of each traversal attempt (C# `_initializers`).
    pub(crate) initializers: Vec<Cmd>,
    /// Per-state admissible lower bound on arcs-to-accept, ignoring constraints (`u32::MAX` sentinel = no accepting state reachable).
    /// See `docs/research/pg-fst-optimize-design-notes.md`.
    pub(crate) min_hops_to_accept: Vec<u32>,
}

impl Fst {
    pub fn is_deterministic(&self) -> bool {
        self.deterministic
    }
    pub fn state_count(&self) -> usize {
        self.states.len()
    }
    pub fn arc_count(&self) -> usize {
        self.arcs.len()
    }
    pub fn register_count(&self) -> u32 {
        self.register_count
    }
    pub fn start(&self) -> u32 {
        self.start
    }
    pub fn direction(&self) -> Direction {
        self.direction
    }
    pub fn states(&self) -> &[StateMeta] {
        &self.states
    }
    pub fn arcs(&self) -> &[Arc] {
        &self.arcs
    }
    pub(crate) fn constraint(&self, id: ConstraintId) -> &MatchInput {
        &self.constraints[id.0 as usize]
    }
    /// See the field doc: per-state admissible lower bound on arcs-to-accept, indexed by state id.
    #[inline]
    pub(crate) fn min_hops_to_accept(&self) -> &[u32] {
        &self.min_hops_to_accept
    }

    /// Resolve a group's tag (C# `_groups[name]`).
    pub fn group_tag(&self, name: &str) -> Option<i32> {
        self.groups.iter().find(|(n, _)| n == name).map(|&(_, t)| t)
    }

    /// C# `Fst.GetOffsets`: the captured `(start, end)` of a group from a result's registers, or `None` if unset/empty.
    pub fn get_offsets(&self, name: &str, registers: &[crate::Register]) -> Option<(i32, i32)> {
        let tag = self.group_tag(name)? as usize;
        let start_val = registers[tag * 2]; // [tag, 0]
        let end_val = registers[(tag + 1) * 2 + 1]; // [tag + 1, 1]
        if start_val.has && end_val.has && !start_val.value_eq(&end_val) {
            // LeftToRight: (start, end); RightToLeft would swap (Fst.cs:128-137).
            match self.direction {
                Direction::LeftToRight => Some((start_val.offset, end_val.offset)),
                Direction::RightToLeft => Some((end_val.offset, start_val.offset)),
            }
        } else {
            None
        }
    }
}

/// Intern a constraint pool while freezing; canonical lanes make structurally-equal inputs share an id.
#[derive(Default)]
pub(crate) struct ConstraintPool {
    pub items: Vec<MatchInput>,
    index: std::collections::HashMap<(Vec<u64>, Vec<Vec<u64>>), u32>,
}

impl ConstraintPool {
    pub fn intern(&mut self, input: MatchInput) -> ConstraintId {
        // canonical key: negated set order-normalized so equal inputs collapse.
        let mut neg = input.negated.clone();
        neg.sort();
        neg.dedup();
        let key = (input.pos.clone(), neg.clone());
        if let Some(&id) = self.index.get(&key) {
            return ConstraintId(id);
        }
        let id = self.items.len() as u32;
        self.items.push(MatchInput {
            pos: input.pos,
            negated: neg,
        });
        self.index.insert(key, id);
        ConstraintId(id)
    }
}

/// Keeps `UNCONSTRAINED` referenced so the canonicalization contract lives next to the pool.
const _: u64 = UNCONSTRAINED;
