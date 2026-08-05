//! Non-deterministic finite automaton over segment constraints (the acceptor NFA that
//! `Matcher.Compile` builds before `Determinize()`/`EpsilonRemoval()`; Fst.cs + Matcher.cs).
//!
//! Only the reachable acceptor machinery: states, epsilon arcs (optionally carrying a capture
//! **tag**, C# `CreateTag`), single-constraint arcs, and accepting states with an id + priority.
//! No transducer outputs (proven dead, plan §5.4), so arcs carry no `Output`s and no enqueue
//! count.

use crate::AcceptInfo;

/// An NFA arc's input predicate. Real NFA arcs are either epsilon (possibly a capture tag) or a
/// single positive constraint; the negated-condition form only arises during determinization
/// (see `crate::optimize`).
#[derive(Clone, Debug)]
pub enum ArcInput {
    Epsilon,
    /// A single-segment constraint as canonical `u64` lanes (§5.3).
    Fs(Vec<u64>),
}

/// One NFA arc. `tag == -1` means "no tag"; otherwise this is an epsilon arc that records the
/// current position into capture register `tag` (C# `Arc.Tag`). `priority` is filled in by
/// `Nfa::mark_arc_priorities`.
#[derive(Clone, Debug)]
pub struct NfaArc {
    pub input: ArcInput,
    pub target: usize,
    pub tag: i32,
    pub priority: i32,
}

#[derive(Clone, Debug, Default)]
pub struct NfaStateData {
    pub accepting: bool,
    pub accept_infos: Vec<AcceptInfo>,
    pub arcs: Vec<NfaArc>,
}

/// A mutable Thompson-construction NFA. `next_tag`/`groups` mirror C# `_nextTag`/`_groups`:
/// each capture group name owns an even "start" tag and the following odd "end" tag.
#[derive(Debug, Default)]
pub struct Nfa {
    pub states: Vec<NfaStateData>,
    pub start: usize,
    pub next_tag: i32,
    pub groups: Vec<(String, i32)>,
}

impl Nfa {
    pub fn new() -> Self {
        let mut nfa = Nfa {
            states: Vec::new(),
            start: 0,
            next_tag: 0,
            groups: Vec::new(),
        };
        nfa.start = nfa.create_state();
        nfa
    }

    pub fn create_state(&mut self) -> usize {
        self.states.push(NfaStateData::default());
        self.states.len() - 1
    }

    pub fn create_accepting_state(&mut self, info: AcceptInfo) -> usize {
        self.states.push(NfaStateData {
            accepting: true,
            accept_infos: vec![info],
            arcs: Vec::new(),
        });
        self.states.len() - 1
    }

    /// Resolve (creating if needed) the tag number for a group boundary. C# `GetTag`
    /// (Fst.cs:159-170): a group owns tag `t` (start) and `t+1` (end).
    pub fn get_tag(&mut self, group: &str, is_start: bool) -> i32 {
        if let Some(&(_, t)) = self.groups.iter().find(|(n, _)| n == group) {
            return if is_start { t } else { t + 1 };
        }
        let t = self.next_tag;
        self.next_tag += 2;
        self.groups.push((group.to_string(), t));
        if is_start {
            t
        } else {
            t + 1
        }
    }

    /// C# `CreateTag`: an epsilon arc from `source` to a fresh state that records the current
    /// position into the group's start/end register.
    pub fn create_tag(&mut self, source: usize, group: &str, is_start: bool) -> usize {
        let tag = self.get_tag(group, is_start);
        let target = self.create_state();
        self.states[source].arcs.push(NfaArc {
            input: ArcInput::Epsilon,
            target,
            tag,
            priority: -1,
        });
        target
    }

    pub fn add_epsilon(&mut self, source: usize, target: usize) {
        self.states[source].arcs.push(NfaArc {
            input: ArcInput::Epsilon,
            target,
            tag: -1,
            priority: -1,
        });
    }

    pub fn add_input(&mut self, source: usize, target: usize, lanes: Vec<u64>) {
        self.states[source].arcs.push(NfaArc {
            input: ArcInput::Fs(lanes),
            target,
            tag: -1,
            priority: -1,
        });
    }

    /// Number of capture registers reserved by tags (C# `_nextTag`). This is the base of the
    /// register array before determinization allocates reindex temporaries.
    pub fn tag_register_count(&self) -> i32 {
        self.next_tag
    }

    /// Port of `MarkArcPriorities` (Fst.cs:801-817): a pre-order DFS from the start state that
    /// numbers every arc in visit order. The start state's arcs are pushed **reversed** so the
    /// first arc is popped (and numbered) first; a target's arcs are pushed in forward order the
    /// first time it is reached. Lower priority number = higher priority (`Arc.MinBy`).
    pub fn mark_arc_priorities(&mut self) {
        // Work on (state, arc-index) pairs since arcs live inside states.
        let mut visited = vec![false; self.states.len()];
        let mut todo: Vec<(usize, usize)> = Vec::new();
        // StartState.Arcs.Reverse() -> pushed so index 0 ends on top.
        for ai in (0..self.states[self.start].arcs.len()).rev() {
            todo.push((self.start, ai));
        }
        let mut next_priority = 0;
        while let Some((si, ai)) = todo.pop() {
            self.states[si].arcs[ai].priority = next_priority;
            next_priority += 1;
            let target = self.states[si].arcs[ai].target;
            if !visited[target] {
                visited[target] = true;
                for tai in 0..self.states[target].arcs.len() {
                    todo.push((target, tai));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-reasoned pre-order DFS numbering of `MarkArcPriorities` (Fst.cs:801-817). The start
    /// state's arcs are explored first-to-last (initial `Reverse()`), but a deeper state is fully
    /// descended before the start's next arc — verifying the exact stack discipline.
    #[test]
    fn mark_arc_priorities_preorder() {
        // states: 0(start) -> 1 (arc s0a0), 0 -> 2 (arc s0a1); 1 -> 3, 2 -> 3.
        let mut nfa = Nfa::new(); // creates state 0
        let s1 = nfa.create_state();
        let s2 = nfa.create_state();
        let s3 = nfa.create_state();
        nfa.add_epsilon(nfa.start, s1); // state0 arc0
        nfa.add_epsilon(nfa.start, s2); // state0 arc1
        nfa.add_epsilon(s1, s3); // state1 arc0
        nfa.add_epsilon(s2, s3); // state2 arc0
        nfa.mark_arc_priorities();
        // Expected: 0->1 = 0, 1->3 = 1, 0->2 = 2, 2->3 = 3.
        assert_eq!(nfa.states[0].arcs[0].priority, 0);
        assert_eq!(nfa.states[1].arcs[0].priority, 1);
        assert_eq!(nfa.states[0].arcs[1].priority, 2);
        assert_eq!(nfa.states[2].arcs[0].priority, 3);
    }
}
