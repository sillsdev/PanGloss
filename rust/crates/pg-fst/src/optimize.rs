//! Subset-construction optimizer: the port of `Fst.Optimize` (Fst.cs:819-901), faithful to the acceptor path only -- the transducer-only dequeue/output logic is dead and omitted.

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::fst::{ConstraintPool, Fst, MatchInput, StateMeta};
use crate::lanes::{canonicalize, flat_unify, is_empty};
use crate::nfa::{ArcInput, Nfa};
use crate::{AcceptInfo, Arc, Cmd, Direction, CURRENT_POSITION};

// --- NFA-state-info and subset state (C# NfaStateInfo / SubsetState) --------------------------

/// C# `NfaStateInfo` on the acceptor path (Fst.cs:465-556): equality/hash on `(state, tag key-set)` only.
#[derive(Clone, Debug)]
struct Nsi {
    state: usize,
    max_priority: i32,
    last_priority: i32,
    /// Tag number → its live index (C# `Tags`). `BTreeMap` gives deterministic iteration.
    tags: BTreeMap<i32, i32>,
}

impl PartialEq for Nsi {
    fn eq(&self, other: &Self) -> bool {
        self.state == other.state
            && self.tags.len() == other.tags.len()
            && self.tags.keys().eq(other.tags.keys())
    }
}
impl Eq for Nsi {}

impl std::hash::Hash for Nsi {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.state.hash(h);
        // order-independent over tag keys (C# XOR aggregate)
        let mut x: i32 = 0;
        for &k in self.tags.keys() {
            x ^= k;
        }
        x.hash(h);
    }
}

/// C# `NfaStateInfo.CompareTo`: `(max_priority, last_priority)` ascending.
fn nsi_cmp(a: &Nsi, b: &Nsi) -> std::cmp::Ordering {
    a.max_priority
        .cmp(&b.max_priority)
        .then(a.last_priority.cmp(&b.last_priority))
}

/// C# `SubsetState`: a set of `Nsi`, keyed by set equality / order-independent hash for the dedup map.
#[derive(Clone, Debug)]
struct SubsetState {
    states: Vec<Nsi>,
}

impl SubsetState {
    fn new(states: Vec<Nsi>) -> Self {
        SubsetState { states }
    }
}

impl PartialEq for SubsetState {
    fn eq(&self, other: &Self) -> bool {
        if self.states.len() != other.states.len() {
            return false;
        }
        // set equality (elements are unique by Nsi::eq)
        self.states.iter().all(|s| other.states.contains(s))
    }
}
impl Eq for SubsetState {}

impl std::hash::Hash for SubsetState {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        // XOR of per-element hashes -> order independent (C# Aggregate(^)).
        let mut acc: u64 = 0;
        for s in &self.states {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(s, &mut hasher);
            acc ^= std::hash::Hasher::finish(&hasher);
        }
        acc.hash(h);
    }
}

// --- epsilon closure (Fst.cs:1141-1190) -------------------------------------------------------

/// C# `EpsilonClosure` (Fst.cs:1141-1190), keyed on the NFA state alone (outputs are empty in a matcher): keeps the highest-priority path per NFA state, seeds in order with the last of equal key winning; `index` stamps tag writes.
fn epsilon_closure(nfa: &Nfa, seeds: Vec<Nsi>, index: i32) -> Vec<Nsi> {
    let mut entries: Vec<Nsi> = Vec::new();
    let mut slot: HashMap<usize, usize> = HashMap::new();
    let mut stack: Vec<Nsi> = Vec::new();
    for seed in seeds {
        match slot.entry(seed.state) {
            std::collections::hash_map::Entry::Occupied(e) => entries[*e.get()] = seed.clone(),
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(entries.len());
                entries.push(seed.clone());
            }
        }
        stack.push(seed);
    }

    while let Some(top) = stack.pop() {
        for arc in &nfa.states[top.state].arcs {
            if !matches!(arc.input, ArcInput::Epsilon) {
                continue;
            }
            let new_max = arc.priority.max(top.max_priority);
            let new_last = arc.priority;
            if let Some(&at) = slot.get(&arc.target) {
                let temp = &entries[at];
                if temp.max_priority < new_max {
                    continue;
                }
                if temp.max_priority == new_max && temp.last_priority <= new_last {
                    continue;
                }
            }
            let mut tags = top.tags.clone();
            if arc.tag != -1 {
                tags.insert(arc.tag, index);
            }
            let nsi = Nsi {
                state: arc.target,
                max_priority: new_max,
                last_priority: new_last,
                tags,
            };
            match slot.entry(nsi.state) {
                std::collections::hash_map::Entry::Occupied(e) => entries[*e.get()] = nsi.clone(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(entries.len());
                    entries.push(nsi.clone());
                }
            }
            stack.push(nsi);
        }
    }
    entries
}

/// C# `GetFirstFreeIndex` (Fst.cs:1192-1201): max tag index in the subset, + 1.
fn get_first_free_index(subset: &SubsetState) -> i32 {
    let mut m = -1;
    for s in &subset.states {
        for &v in s.tags.values() {
            m = m.max(v);
        }
    }
    m + 1
}

// --- register index allocation (Fst.cs:1203-1216) ---------------------------------------------

fn get_register_index(
    reg_indices: &mut HashMap<(i32, i32), i32>,
    next_tag: i32,
    tag: i32,
    index: i32,
) -> i32 {
    if index == 0 {
        return tag;
    }
    let key = (tag, index);
    if let Some(&r) = reg_indices.get(&key) {
        return r;
    }
    let r = next_tag + reg_indices.len() as i32;
    reg_indices.insert(key, r);
    r
}

/// A partial determinization condition: one row of `DeterministicGetArcs`'s `preprocessedConditions`.
type PreCond = (Option<Vec<u64>>, Vec<Vec<u64>>, Vec<Nsi>);

// --- mutable build target ---------------------------------------------------------------------

struct BuildArc {
    input: MatchInput,
    target: usize,
    commands: Vec<Cmd>,
    priority: i32,
}

struct BuildState {
    accepting: bool,
    accept_infos: Vec<AcceptInfo>,
    finishers: Vec<Cmd>,
    arcs: Vec<BuildArc>,
}

struct Builder<'n> {
    nfa: &'n Nfa,
    states: Vec<BuildState>,
    /// canonical subset per build state (index-aligned) for `ReorderTagIndices`.
    canonical: Vec<SubsetState>,
    lookup: HashMap<SubsetState, usize>,
    reg_indices: HashMap<(i32, i32), i32>,
    next_tag: i32,
}

impl<'n> Builder<'n> {
    /// C# `CreateOptimizedState` (Fst.cs:989-1054), acceptor path.
    fn create_state(&mut self, subset: &SubsetState) -> usize {
        let mut accepting: Vec<&Nsi> = subset
            .states
            .iter()
            .filter(|s| self.nfa.states[s.state].accepting)
            .collect();
        // `orderby state descending` (by CompareTo); the sort is stable, so ties keep subset order as in C#.
        accepting.sort_by(|a, b| nsi_cmp(b, a));

        if accepting.is_empty() {
            self.states.push(BuildState {
                accepting: false,
                accept_infos: Vec::new(),
                finishers: Vec::new(),
                arcs: Vec::new(),
            });
            return self.states.len() - 1;
        }

        let mut accept_infos: Vec<AcceptInfo> = Vec::new();
        for s in &accepting {
            accept_infos.extend(self.nfa.states[s.state].accept_infos.iter().cloned());
        }
        let mut finishers: Vec<Cmd> = Vec::new();
        let mut finished: std::collections::HashSet<i32> = std::collections::HashSet::new();
        for s in &accepting {
            for (&tag, &val) in &s.tags {
                if val > 0 && !finished.contains(&tag) {
                    finished.insert(tag);
                    let src = get_register_index(&mut self.reg_indices, self.next_tag, tag, val);
                    let dest = get_register_index(&mut self.reg_indices, self.next_tag, tag, 0);
                    finishers.push(Cmd { dest, src });
                }
            }
        }
        // acceptor: outputs are always empty, so the state is directly accepting.
        self.states.push(BuildState {
            accepting: true,
            accept_infos,
            finishers,
            arcs: Vec::new(),
        });
        self.states.len() - 1
    }

    /// C# `CreateOptimizedArc` (Fst.cs:922-987).
    #[allow(clippy::too_many_arguments)]
    fn create_arc(
        &mut self,
        queue: &mut VecDeque<(SubsetState, usize)>,
        cur_subset: &SubsetState,
        cur_idx: usize,
        target: SubsetState,
        input: MatchInput,
        priority: i32,
    ) {
        // cmdTags = target tags not present (key+value) in any current state.
        let mut cmd_tags: BTreeMap<i32, i32> = BTreeMap::new();
        for ts in &target.states {
            for (&k, &v) in &ts.tags {
                let found = cur_subset
                    .states
                    .iter()
                    .any(|cs| cs.tags.get(&k) == Some(&v));
                if !found {
                    cmd_tags.insert(k, v);
                }
            }
        }

        let mut cmds: Vec<Cmd> = Vec::new();
        let target_idx = if let Some(&existing) = self.lookup.get(&target) {
            let stored = self.canonical[existing].clone();
            self.reorder_tag_indices(&target, &stored, &mut cmds);
            existing
        } else {
            let idx = self.create_state(&target);
            self.lookup.insert(target.clone(), idx);
            self.canonical.push(target.clone());
            debug_assert_eq!(self.canonical.len(), self.states.len());
            queue.push_back((target.clone(), idx));
            idx
        };

        for (&tag, &val) in &cmd_tags {
            let reg = get_register_index(&mut self.reg_indices, self.next_tag, tag, val);
            let mut found = false;
            for c in cmds.iter_mut() {
                if c.src == reg {
                    found = true;
                    c.src = CURRENT_POSITION;
                    break;
                }
            }
            if !found {
                cmds.push(Cmd {
                    dest: reg,
                    src: CURRENT_POSITION,
                });
            }
        }

        self.states[cur_idx].arcs.push(BuildArc {
            input,
            target: target_idx,
            commands: cmds,
            priority,
        });
    }

    /// C# `ReorderTagIndices` (Fst.cs:1084-1139): reconcile tag-index assignments when merging `from` into the already-stored equal subset `to`.
    fn reorder_tag_indices(&mut self, from: &SubsetState, to: &SubsetState, cmds: &mut Vec<Cmd>) {
        let mut new_cmds: Vec<Cmd> = Vec::new();
        let mut reordered_indices: HashMap<(i32, i32), i32> = HashMap::new();
        let mut reordered_states: HashMap<(i32, i32), Nsi> = HashMap::new();

        for from_state in &from.states {
            for to_state in &to.states {
                if to_state.state != from_state.state {
                    continue;
                }
                for (&ftag_key, &ftag_val) in &from_state.tags {
                    let to_idx = match to_state.tags.get(&ftag_key) {
                        Some(&i) => i,
                        None => continue, // keysets match for equal states, but stay total
                    };
                    let tag_index = (ftag_key, to_idx);
                    if let Some(&index) = reordered_indices.get(&tag_index) {
                        let state = &reordered_states[&tag_index];
                        if index != ftag_val
                            && state.max_priority <= from_state.max_priority
                            && state.last_priority <= from_state.last_priority
                        {
                            continue;
                        }
                        let src = get_register_index(
                            &mut self.reg_indices,
                            self.next_tag,
                            ftag_key,
                            index,
                        );
                        let dest = get_register_index(
                            &mut self.reg_indices,
                            self.next_tag,
                            ftag_key,
                            tag_index.1,
                        );
                        new_cmds.retain(|c| !(c.src == src && c.dest == dest));
                    }
                    if tag_index.1 != ftag_val {
                        let src = get_register_index(
                            &mut self.reg_indices,
                            self.next_tag,
                            ftag_key,
                            ftag_val,
                        );
                        let dest = get_register_index(
                            &mut self.reg_indices,
                            self.next_tag,
                            ftag_key,
                            tag_index.1,
                        );
                        new_cmds.push(Cmd { dest, src });
                    }
                    reordered_indices.insert(tag_index, ftag_val);
                    reordered_states.insert(tag_index, from_state.clone());
                }
            }
        }
        cmds.extend(new_cmds);
    }
}

// --- arc selectors ----------------------------------------------------------------------------

/// C# `DeterministicGetArcs` (Fst.cs:590-688): partitions the subset's arc conditions into positive/negated combinations, epsilon-closing each combination's target states.
fn deterministic_get_arcs(nfa: &Nfa, from: &SubsetState) -> Vec<(SubsetState, MatchInput, i32)> {
    // Group arcs by input FS (stable, state-index then arc order).
    let mut cond_keys: Vec<Vec<u64>> = Vec::new();
    let mut cond_map: HashMap<Vec<u64>, Vec<Nsi>> = HashMap::new();
    for st in &from.states {
        for arc in &nfa.states[st.state].arcs {
            if let ArcInput::Fs(lanes) = &arc.input {
                let key = canonicalize(lanes.clone());
                let nsi = Nsi {
                    state: arc.target,
                    max_priority: arc.priority.max(st.max_priority),
                    last_priority: arc.priority,
                    tags: st.tags.clone(),
                };
                cond_map
                    .entry(key.clone())
                    .or_insert_with(|| {
                        cond_keys.push(key.clone());
                        Vec::new()
                    })
                    .push(nsi);
            }
        }
    }

    // preprocessedConditions: (positive|None, negated set, included states)
    let mut pre: Vec<PreCond> = vec![(None, Vec::new(), Vec::new())];
    for key in &cond_keys {
        let cond_states = &cond_map[key];
        let mut temp: Vec<PreCond> = Vec::new();
        for (pos, neg, states) in &pre {
            let new_pos = match pos {
                None => Some(key.clone()),
                Some(p) => flat_unify(p, key),
            };
            if let Some(np) = new_pos {
                let mut ns = states.clone();
                ns.extend(cond_states.iter().cloned());
                temp.push((Some(np), neg.clone(), ns));
            }
            if !is_empty(key) {
                let mut nn = neg.clone();
                nn.push(key.clone());
                temp.push((pos.clone(), nn, states.clone()));
            }
        }
        pre = temp;
    }

    let mut out = Vec::new();
    for (pos, neg, states) in pre {
        let pos = match pos {
            Some(p) => p,
            None => continue,
        };
        if states.is_empty() {
            continue;
        }
        let input = MatchInput { pos, negated: neg };
        if !input.is_satisfiable() {
            continue;
        }
        let idx = get_first_free_index(from);
        let closed = epsilon_closure(nfa, states, idx);
        if closed.is_empty() {
            continue;
        }
        out.push((SubsetState::new(closed), input, 0));
    }
    out
}

/// C# `EpsilonRemovalGetArcs` (Fst.cs:775-799): each non-epsilon arc becomes one arc to the epsilon-closure of its target.
fn epsilon_removal_get_arcs(nfa: &Nfa, from: &SubsetState) -> Vec<(SubsetState, MatchInput, i32)> {
    let mut out = Vec::new();
    let idx = get_first_free_index(from);
    for q in &from.states {
        for arc in &nfa.states[q.state].arcs {
            if let ArcInput::Fs(lanes) = &arc.input {
                let nsi = Nsi {
                    state: arc.target,
                    max_priority: arc.priority.max(q.max_priority),
                    last_priority: arc.priority,
                    tags: q.tags.clone(),
                };
                let closed = epsilon_closure(nfa, vec![nsi], idx);
                let priority = closed.iter().map(|s| s.max_priority).min().unwrap_or(0);
                out.push((
                    SubsetState::new(closed),
                    MatchInput {
                        pos: canonicalize(lanes.clone()),
                        negated: Vec::new(),
                    },
                    priority,
                ));
            }
        }
    }
    out
}

// C# Fst.Minimize (Fst.cs:1599-1707): vectors retain HashSet insertion order.
fn minimize(states: &mut Vec<BuildState>, start: usize) {
    #[derive(PartialEq, Eq, Hash)]
    struct Key {
        accepting: bool,
        finishers: Vec<Cmd>,
        accept_ids: Vec<Option<String>>,
        input: Option<(Vec<u64>, Vec<Vec<u64>>)>,
        commands: Vec<Cmd>,
    }
    fn key(state: &BuildState, arc: Option<&BuildArc>) -> Key {
        let input = arc.map(|arc| {
            let mut negated = arc.input.negated.clone();
            negated.sort();
            negated.dedup();
            (arc.input.pos.clone(), negated)
        });
        Key {
            accepting: state.accepting,
            finishers: if state.accepting {
                state.finishers.clone()
            } else {
                Vec::new()
            },
            accept_ids: if state.accepting {
                state
                    .accept_infos
                    .iter()
                    .map(|info| info.id.clone())
                    .collect()
            } else {
                Vec::new()
            },
            input,
            commands: arc.map_or_else(Vec::new, |arc| arc.commands.clone()),
        }
    }
    fn same_set(a: &[usize], b: &[usize]) -> bool {
        a.len() == b.len() && a.iter().all(|item| b.contains(item))
    }

    let mut partitions = vec![Vec::new()];
    let mut accepting_groups: HashMap<Key, usize> = HashMap::new();
    for (i, state) in states.iter().enumerate() {
        if !state.accepting {
            partitions[0].push(i);
        } else {
            let k = key(state, None);
            let index = *accepting_groups.entry(k).or_insert_with(|| {
                partitions.push(Vec::new());
                partitions.len() - 1
            });
            partitions[index].push(i);
        }
    }
    let mut waiting: VecDeque<Vec<usize>> = partitions[1..].iter().cloned().collect();
    while let Some(a) = waiting.pop_front() {
        let mut in_a = vec![false; states.len()];
        for state in a {
            in_a[state] = true;
        }
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut group_index: HashMap<Key, usize> = HashMap::new();
        for (i, state) in states.iter().enumerate() {
            for arc in &state.arcs {
                if in_a[arc.target] {
                    let index = *group_index.entry(key(state, Some(arc))).or_insert_with(|| {
                        groups.push(Vec::new());
                        groups.len() - 1
                    });
                    groups[index].push(i);
                }
            }
        }
        for x in groups {
            let mut in_x = vec![false; states.len()];
            for &i in &x {
                in_x[i] = true;
            }
            for i in (0..partitions.len()).rev() {
                let y = &partitions[i];
                let mut in_y = vec![false; states.len()];
                for &item in y {
                    in_y[item] = true;
                }
                let mut subset1 = Vec::new();
                for &item in &x {
                    if in_y[item] {
                        subset1.push(item);
                        in_y[item] = false;
                    }
                }
                if subset1.is_empty() {
                    continue;
                }
                let subset2: Vec<usize> = y.iter().copied().filter(|&item| !in_x[item]).collect();
                if let Some(j) = waiting.iter().position(|w| same_set(w, y)) {
                    waiting[j] = subset1.clone();
                    if !subset2.is_empty() {
                        waiting.push_back(subset2.clone());
                    }
                } else if subset1.len() <= subset2.len() {
                    waiting.push_back(subset1.clone());
                } else if !subset2.is_empty() {
                    waiting.push_back(subset2.clone());
                }
                partitions[i] = subset1;
                if !subset2.is_empty() {
                    partitions.push(subset2);
                }
            }
        }
    }
    let mut representative: Vec<usize> = (0..states.len()).collect();
    let mut merged = false;
    for partition in partitions {
        if let Some((&first, rest)) = partition.split_first() {
            for &state in rest {
                representative[state] = first;
                merged = true;
            }
        }
    }
    if !merged {
        return;
    }
    let mut keep = vec![false; states.len()];
    keep[start] = true;
    for state in states.iter_mut() {
        for arc in &mut state.arcs {
            arc.target = representative[arc.target];
            keep[arc.target] = true;
        }
    }
    let mut remap = vec![0; states.len()];
    let mut next = 0;
    for (i, &kept) in keep.iter().enumerate() {
        if kept {
            remap[i] = next;
            next += 1;
        }
    }
    for state in states.iter_mut() {
        for arc in &mut state.arcs {
            arc.target = remap[arc.target];
        }
    }
    let mut index = 0;
    states.retain(|_| {
        let kept = keep[index];
        index += 1;
        kept
    });
}

// --- optimize driver (Fst.cs:819-901) ---------------------------------------------------------

fn optimize(nfa: &Nfa, deterministic: bool, direction: Direction) -> Fst {
    let mut b = Builder {
        nfa,
        states: Vec::new(),
        canonical: Vec::new(),
        lookup: HashMap::new(),
        reg_indices: HashMap::new(),
        next_tag: nfa.next_tag,
    };

    let start_nsi = Nsi {
        state: nfa.start,
        max_priority: 0,
        last_priority: 0,
        tags: BTreeMap::new(),
    };
    let subset_start = SubsetState::new(epsilon_closure(nfa, vec![start_nsi], 0));

    let start_idx = b.create_state(&subset_start);
    b.lookup.insert(subset_start.clone(), start_idx);
    b.canonical.push(subset_start.clone());

    // initializers (Fst.cs:848-859): one command per distinct tag in the start closure.
    let mut cmd_tags: BTreeMap<i32, i32> = BTreeMap::new();
    for s in &subset_start.states {
        for (&k, &v) in &s.tags {
            cmd_tags.insert(k, v);
        }
    }
    let mut initializers: Vec<Cmd> = cmd_tags
        .iter()
        .map(|(&k, &v)| Cmd {
            dest: get_register_index(&mut b.reg_indices, b.next_tag, k, v),
            src: CURRENT_POSITION,
        })
        .collect();

    let mut queue: VecDeque<(SubsetState, usize)> = VecDeque::new();
    queue.push_back((subset_start.clone(), start_idx));

    while let Some((cur, cur_idx)) = queue.pop_front() {
        let arcs = if deterministic {
            deterministic_get_arcs(nfa, &cur)
        } else {
            epsilon_removal_get_arcs(nfa, &cur)
        };
        for (target, input, priority) in arcs {
            b.create_arc(&mut queue, &cur, cur_idx, target, input, priority);
        }
    }

    // RenumberCommands (Fst.cs:889-919): compact register numbers into 0..register_count and sort each command list.
    let mut reg_nums: HashMap<i32, i32> = HashMap::new();
    for i in 0..b.next_tag {
        reg_nums.insert(i, i);
    }
    let mut register_count = b.next_tag;
    let renumber =
        |cmds: &mut Vec<Cmd>, reg_nums: &mut HashMap<i32, i32>, register_count: &mut i32| {
            let assign =
                |reg: i32, reg_nums: &mut HashMap<i32, i32>, register_count: &mut i32| -> i32 {
                    *reg_nums.entry(reg).or_insert_with(|| {
                        let r = *register_count;
                        *register_count += 1;
                        r
                    })
                };
            for cmd in cmds.iter_mut() {
                if cmd.src != CURRENT_POSITION {
                    cmd.src = assign(cmd.src, reg_nums, register_count);
                }
                cmd.dest = assign(cmd.dest, reg_nums, register_count);
            }
            cmds.sort();
        };
    for st in b.states.iter_mut() {
        renumber(&mut st.finishers, &mut reg_nums, &mut register_count);
        for arc in st.arcs.iter_mut() {
            renumber(&mut arc.commands, &mut reg_nums, &mut register_count);
        }
    }
    // Keep initializer register references inside the compacted range too, matching C#'s 0.._nextTag band.
    renumber(&mut initializers, &mut reg_nums, &mut register_count);

    if deterministic {
        minimize(&mut b.states, start_idx);
    }

    // Freeze to CSR.
    let mut pool = ConstraintPool::default();
    let mut states: Vec<StateMeta> = Vec::with_capacity(b.states.len());
    let mut arcs: Vec<Arc> = Vec::new();
    let mut commands: Vec<Cmd> = Vec::new();
    let mut accept_infos: Vec<AcceptInfo> = Vec::new();

    for st in &b.states {
        let accept_lo = accept_infos.len() as u32;
        accept_infos.extend(st.accept_infos.iter().cloned());
        let accept_hi = accept_infos.len() as u32;

        let fin_lo = commands.len() as u32;
        commands.extend(st.finishers.iter().copied());
        let fin_hi = commands.len() as u32;

        let arc_lo = arcs.len() as u32;
        for a in &st.arcs {
            let cmd_lo = commands.len() as u32;
            commands.extend(a.commands.iter().copied());
            let cmd_hi = commands.len() as u32;
            let cid = pool.intern(a.input.clone());
            arcs.push(Arc {
                target: a.target as u32,
                constraint: cid,
                cmd_lo,
                cmd_hi,
                priority: a.priority,
            });
        }
        let arc_hi = arcs.len() as u32;

        states.push(StateMeta {
            accepting: st.accepting,
            accept_lo,
            accept_hi,
            fin_lo,
            fin_hi,
            arc_lo,
            arc_hi,
        });
    }

    // Min-hops-to-accept lower bound, consumed by the nondeterministic traversal's pruning. Why ignoring
    // arc constraints still gives an admissible bound: `docs/research/pg-fst-optimize-design-notes.md`, "`min_hops_to_accept`".
    let min_hops_to_accept = {
        let mut preds: Vec<Vec<u32>> = vec![Vec::new(); states.len()];
        for (s, st) in states.iter().enumerate() {
            for arc in &arcs[st.arc_lo as usize..st.arc_hi as usize] {
                preds[arc.target as usize].push(s as u32);
            }
        }
        let mut dist = vec![u32::MAX; states.len()];
        let mut bfs: VecDeque<u32> = VecDeque::new();
        for (s, st) in states.iter().enumerate() {
            if st.accepting {
                dist[s] = 0;
                bfs.push_back(s as u32);
            }
        }
        while let Some(t) = bfs.pop_front() {
            let d = dist[t as usize] + 1; // finite: only reached states are enqueued
            for &p in &preds[t as usize] {
                if dist[p as usize] > d {
                    dist[p as usize] = d;
                    bfs.push_back(p);
                }
            }
        }
        dist
    };

    // Max-hops-to-accept upper bound: the longest constraint-free path to an accepting state, exact on an acyclic automaton and `u32::MAX` (unbounded) as soon as a state that can reach acceptance lies on a cycle.
    let max_hops_to_accept = {
        let n = states.len();
        let live: Vec<bool> = min_hops_to_accept.iter().map(|&d| d != u32::MAX).collect();
        let mut indegree = vec![0u32; n];
        for (s, st) in states.iter().enumerate() {
            if live[s] {
                for arc in &arcs[st.arc_lo as usize..st.arc_hi as usize] {
                    if live[arc.target as usize] {
                        indegree[arc.target as usize] += 1;
                    }
                }
            }
        }
        let mut order: Vec<u32> = (0..n as u32)
            .filter(|&s| live[s as usize] && indegree[s as usize] == 0)
            .collect();
        let mut at = 0;
        while at < order.len() {
            let st = &states[order[at] as usize];
            at += 1;
            for arc in &arcs[st.arc_lo as usize..st.arc_hi as usize] {
                let t = arc.target as usize;
                if live[t] {
                    indegree[t] -= 1;
                    if indegree[t] == 0 {
                        order.push(t as u32);
                    }
                }
            }
        }
        if order.len() == live.iter().filter(|&&l| l).count() {
            let mut longest = vec![u32::MAX; n];
            for &s in order.iter().rev() {
                let st = &states[s as usize];
                let mut best = if st.accepting { 0 } else { u32::MAX };
                for arc in &arcs[st.arc_lo as usize..st.arc_hi as usize] {
                    let t = longest[arc.target as usize];
                    if live[arc.target as usize] && t != u32::MAX {
                        best = if best == u32::MAX {
                            t + 1
                        } else {
                            best.max(t + 1)
                        };
                    }
                }
                longest[s as usize] = best;
            }
            longest
        } else {
            vec![u32::MAX; n]
        }
    };

    Fst {
        states,
        arcs,
        commands,
        accept_infos,
        constraints: pool.items,
        start: start_idx as u32,
        deterministic,
        direction,
        register_count: register_count as u32,
        groups: nfa.groups.clone(),
        initializers,
        min_hops_to_accept,
        max_hops_to_accept,
    }
}

impl crate::compile::CompileInput {
    /// Compile this pattern all the way to a frozen CSR `Fst`: NFA → `Determinize()` or `EpsilonRemoval()`.
    pub fn compile(&self) -> Fst {
        let nfa = self.build_nfa();
        optimize(&nfa, self.deterministic, Direction::LeftToRight)
    }

    /// Compile with an explicit traversal direction (only affects `ResultCompare` sign).
    pub fn compile_with_direction(&self, direction: Direction) -> Fst {
        let nfa = self.build_nfa();
        optimize(&nfa, self.deterministic, direction)
    }
}

#[cfg(test)]
mod tests;
