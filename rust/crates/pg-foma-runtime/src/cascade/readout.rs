//! Pin-group read-out of a trimmed cascade graph (plan S.6, section 4; W4).
//!
//! Only [`ReadoutMode::Single`] is built (plan revision 7; `Split`, `Auto` and the
//! bypass rule are demoted to Appendix A).
//!
//! # Input graph
//!
//! [`TrimmedGraph`] is a deliberately small mirror of what W3's trimmed product graph
//! has to provide; W5 adapts W3's graph to it. The contract:
//!
//! - states are `0..num_states`; one `start` state; any number of `accepting` states;
//! - an arc is stored per `(src, dst, written character)` with the union of its tags.
//!   `written` is `Some(c)` for an arc that writes the character `c` through `Repr`,
//!   or `None` for an arc that writes nothing (an epsilon arc, for example the final
//!   tagged `eps:eps` arc of an entry, plan 2.2). Several arcs may share endpoints
//!   (parallel arcs: stacks, casings), and self-loops and cycles are allowed;
//! - `written` is ONE `Option<char>`. A W3 product move that writes more than one
//!   character must be split by W5 into intermediate states, with the move's tags on
//!   exactly one of the resulting arcs. Emitting only the first character would make the
//!   read-out reject or mis-read strings;
//! - when W5 collapses parallel arcs (same `src`, `dst`, `written`; differing in weight
//!   or case mark) their tags are UNIONED, never taken from the first arc;
//! - every `src`, `dst`, `start` and accepting state must be below `num_states`; arcs
//!   that are not make [`ReadoutIndex::new`] panic with a message naming the arc;
//! - tags live on arcs only, never on states (plan 2.0). One arc may carry a root tag
//!   and a rule tag at once (one product move can resolve a surface symbol through every
//!   layer, review r3 R3-2);
//! - root tags are **head** root entries only (plan 4.1).
//!
//! # What is computed
//!
//! For an exact string `s` (plan S.6): forward sets along `s` (layered, so the same
//! state at two string positions is two nodes), then a backward filter from the
//! accepting states along the same string. `roots(s)` and `rules(s)` are the union of
//! the tags on arcs whose source node survives the forward pass and whose target node
//! survives the backward pass, self-loops included. This is the union over **all**
//! accepting paths spelling `s`, never one path (mutation C95). The result is a
//! `Vec<PinGroup>` with zero groups when `s` is not accepted and exactly one group when
//! it is (a group with empty sets is a real answer: accepted, nothing to pin).

use std::collections::HashSet;

use pg_grammar::model::{LexEntryId, MRuleId};

/// Read-out mode. Only `Single` exists; the other variants are demoted (Appendix A).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReadoutMode {
    /// One group: all root tags and all rule tags of the accepting paths of `s`.
    #[default]
    Single,
}

// TODO(W5/WC): replace by `crate::confirm::PinGroup` once PR #419 (confirm_pin_groups)
// is on main; the field names and types mirror its declaration in plan 1.2.
/// Local mirror of `confirm::PinGroup` (PR #419, not on main yet).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PinGroup {
    /// Head root entries only.
    pub roots: HashSet<LexEntryId>,
    pub rules: HashSet<MRuleId>,
}

/// Tags carried by one arc (union over the product moves merged into it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArcTags {
    pub roots: Vec<LexEntryId>,
    pub rules: Vec<MRuleId>,
}

/// One stored arc of the trimmed graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arc {
    pub src: u32,
    pub dst: u32,
    /// Character written by `Repr`, or `None` for an arc that writes nothing.
    pub written: Option<char>,
    pub tags: ArcTags,
}

/// Minimal trimmed graph the read-out works on (see the module docs for the contract).
#[derive(Clone, Debug, Default)]
pub struct TrimmedGraph {
    pub num_states: u32,
    pub start: u32,
    pub accepting: Vec<u32>,
    pub arcs: Vec<Arc>,
}

/// Adjacency lists of a graph, built once and reused for every string (S.6 reads out
/// once per variant).
pub struct ReadoutIndex {
    out_arcs: Vec<Vec<usize>>,
    in_arcs: Vec<Vec<usize>>,
}

impl ReadoutIndex {
    /// Panics with a clear message if an arc names a state `>= num_states`.
    pub fn new(graph: &TrimmedGraph) -> Self {
        let ns = graph.num_states as usize;
        let mut out_arcs: Vec<Vec<usize>> = vec![Vec::new(); ns];
        let mut in_arcs: Vec<Vec<usize>> = vec![Vec::new(); ns];
        for (k, a) in graph.arcs.iter().enumerate() {
            assert!(
                a.src < graph.num_states && a.dst < graph.num_states,
                "TrimmedGraph arc {k} ({} -> {}) names a state outside 0..{}",
                a.src,
                a.dst,
                graph.num_states
            );
            out_arcs[a.src as usize].push(k);
            in_arcs[a.dst as usize].push(k);
        }
        Self { out_arcs, in_arcs }
    }
}

/// Pin groups of the exact string `s` under `mode` (builds the index; use
/// [`read_out_indexed`] to reuse it across strings).
pub fn read_out(graph: &TrimmedGraph, s: &str, mode: ReadoutMode) -> Vec<PinGroup> {
    read_out_indexed(graph, &ReadoutIndex::new(graph), s, mode)
}

/// As [`read_out`], with a prebuilt [`ReadoutIndex`] of the same graph.
pub fn read_out_indexed(
    graph: &TrimmedGraph,
    index: &ReadoutIndex,
    s: &str,
    mode: ReadoutMode,
) -> Vec<PinGroup> {
    match mode {
        ReadoutMode::Single => read_out_single(graph, index, s),
    }
}

fn read_out_single(graph: &TrimmedGraph, index: &ReadoutIndex, s: &str) -> Vec<PinGroup> {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let ns = graph.num_states as usize;
    if ns == 0 || graph.start >= graph.num_states {
        return Vec::new();
    }
    let node = |i: usize, st: u32| i * ns + st as usize;

    let (out_arcs, in_arcs) = (&index.out_arcs, &index.in_arcs);

    // Forward sets along `s`: node (i, state) = `i` characters consumed.
    let mut fwd = vec![false; (n + 1) * ns];
    let mut stack = vec![(0usize, graph.start)];
    fwd[node(0, graph.start)] = true;
    while let Some((i, u)) = stack.pop() {
        for &k in &out_arcs[u as usize] {
            let a = &graph.arcs[k];
            let j = match a.written {
                None => i,
                Some(c) if i < n && chars[i] == c => i + 1,
                Some(_) => continue,
            };
            if !fwd[node(j, a.dst)] {
                fwd[node(j, a.dst)] = true;
                stack.push((j, a.dst));
            }
        }
    }

    // Backward filter from the accepting states along the same string.
    let mut bwd = vec![false; (n + 1) * ns];
    let mut stack = Vec::new();
    for &acc in &graph.accepting {
        if acc < graph.num_states && !bwd[node(n, acc)] {
            bwd[node(n, acc)] = true;
            stack.push((n, acc));
        }
    }
    while let Some((j, v)) = stack.pop() {
        for &k in &in_arcs[v as usize] {
            let a = &graph.arcs[k];
            let i = match a.written {
                None => j,
                Some(c) if j > 0 && chars[j - 1] == c => j - 1,
                Some(_) => continue,
            };
            if !bwd[node(i, a.src)] {
                bwd[node(i, a.src)] = true;
                stack.push((i, a.src));
            }
        }
    }

    if !(fwd[node(0, graph.start)] && bwd[node(0, graph.start)]) {
        return Vec::new();
    }

    // Tags on every arc between surviving nodes, self-loops included.
    let mut group = PinGroup::default();
    for a in &graph.arcs {
        for i in 0..=n {
            let j = match a.written {
                None => i,
                Some(c) if i < n && chars[i] == c => i + 1,
                Some(_) => continue,
            };
            if fwd[node(i, a.src)] && bwd[node(j, a.dst)] {
                group.roots.extend(a.tags.roots.iter().copied());
                group.rules.extend(a.tags.rules.iter().copied());
            }
        }
    }
    vec![group]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(roots: &[u32], rules: &[u32]) -> ArcTags {
        ArcTags {
            roots: roots.iter().map(|&r| LexEntryId(r)).collect(),
            rules: rules.iter().map(|&r| MRuleId(r)).collect(),
        }
    }

    fn arc(src: u32, dst: u32, written: Option<char>, t: ArcTags) -> Arc {
        Arc {
            src,
            dst,
            written,
            tags: t,
        }
    }

    fn group(roots: &[u32], rules: &[u32]) -> PinGroup {
        PinGroup {
            roots: roots.iter().map(|&r| LexEntryId(r)).collect(),
            rules: rules.iter().map(|&r| MRuleId(r)).collect(),
        }
    }

    /// Explicit enumeration: every walk start -> accepting spelling `s`, each
    /// (position, state) node visited at most twice (enough to cover every arc that lies on
    /// some accepting walk, cycles included), tags unioned over the walks.
    fn oracle(g: &TrimmedGraph, s: &str) -> Vec<PinGroup> {
        let chars: Vec<char> = s.chars().collect();
        let ns = g.num_states as usize;
        let mut visits = vec![0u8; (chars.len() + 1) * ns];
        let mut path: Vec<usize> = Vec::new();
        let mut walks: Vec<Vec<usize>> = Vec::new();
        fn go(
            g: &TrimmedGraph,
            chars: &[char],
            i: usize,
            u: u32,
            visits: &mut Vec<u8>,
            path: &mut Vec<usize>,
            walks: &mut Vec<Vec<usize>>,
        ) {
            let ns = g.num_states as usize;
            let idx = i * ns + u as usize;
            if visits[idx] >= 2 {
                return;
            }
            visits[idx] += 1;
            if i == chars.len() && g.accepting.contains(&u) {
                walks.push(path.clone());
            }
            for (k, a) in g.arcs.iter().enumerate() {
                if a.src != u {
                    continue;
                }
                let j = match a.written {
                    None => i,
                    Some(c) if i < chars.len() && chars[i] == c => i + 1,
                    Some(_) => continue,
                };
                path.push(k);
                go(g, chars, j, a.dst, visits, path, walks);
                path.pop();
            }
            visits[idx] -= 1;
        }
        go(g, &chars, 0, g.start, &mut visits, &mut path, &mut walks);
        if walks.is_empty() {
            return Vec::new();
        }
        let mut pg = PinGroup::default();
        for w in &walks {
            for &k in w {
                pg.roots.extend(g.arcs[k].tags.roots.iter().copied());
                pg.rules.extend(g.arcs[k].tags.rules.iter().copied());
            }
        }
        vec![pg]
    }

    fn graph(num_states: u32, accepting: &[u32], arcs: Vec<Arc>) -> TrimmedGraph {
        TrimmedGraph {
            num_states,
            start: 0,
            accepting: accepting.to_vec(),
            arcs,
        }
    }

    #[test]
    #[should_panic(expected = "outside 0..2")]
    fn arc_outside_num_states_panics_clearly() {
        let g = graph(2, &[1], vec![arc(0, 5, Some('a'), ArcTags::default())]);
        let _ = read_out(&g, "a", ReadoutMode::Single);
    }

    #[test]
    fn unaccepted_string_has_no_group() {
        let g = graph(2, &[1], vec![arc(0, 1, Some('a'), tags(&[1], &[]))]);
        assert!(read_out(&g, "b", ReadoutMode::Single).is_empty());
        assert!(read_out(&g, "", ReadoutMode::Single).is_empty());
        assert!(read_out(&g, "aa", ReadoutMode::Single).is_empty());
    }

    #[test]
    fn accepted_string_without_tags_is_one_empty_group() {
        let g = graph(2, &[1], vec![arc(0, 1, Some('a'), ArcTags::default())]);
        assert_eq!(
            read_out(&g, "a", ReadoutMode::Single),
            vec![group(&[], &[])]
        );
    }

    #[test]
    fn arc_with_root_and_rule_tag_gives_both() {
        // One arc carries root 7 and rule 3; a second path to a dead end carries rule 9.
        let g = graph(
            3,
            &[1],
            vec![
                arc(0, 1, Some('a'), tags(&[7], &[3])),
                arc(0, 2, Some('a'), tags(&[8], &[9])),
            ],
        );
        assert_eq!(
            read_out(&g, "a", ReadoutMode::Single),
            vec![group(&[7], &[3])]
        );
    }

    #[test]
    fn rules_after_the_root_on_the_path_are_kept() {
        // root on the first arc, rules on the second and third (plan C18).
        let g = graph(
            4,
            &[3],
            vec![
                arc(0, 1, Some('a'), tags(&[1], &[])),
                arc(1, 2, Some('b'), tags(&[], &[10])),
                arc(2, 3, Some('c'), tags(&[], &[11])),
            ],
        );
        assert_eq!(
            read_out(&g, "abc", ReadoutMode::Single),
            vec![group(&[1], &[10, 11])]
        );
    }

    #[test]
    fn tags_of_all_paths_are_unioned_not_first_path() {
        // Two accepting paths for "a" with different roots and rules (plan C95).
        let g = graph(
            4,
            &[3],
            vec![
                arc(0, 1, Some('a'), tags(&[1], &[])),
                arc(1, 3, None, tags(&[], &[10])),
                arc(0, 2, Some('a'), tags(&[2], &[])),
                arc(2, 3, None, tags(&[], &[20])),
            ],
        );
        assert_eq!(
            read_out(&g, "a", ReadoutMode::Single),
            vec![group(&[1, 2], &[10, 20])]
        );
    }

    #[test]
    fn parallel_duplicate_paths_make_one_group_each_tag_once() {
        // Multiplicity: the same root on parallel arcs (casings/stacks) is in one group,
        // once. Disjointness of root sets across groups is trivial with one group
        // (plan 1.2, 4.1; C19/C38 are about confirm and Split, see the report).
        let g = graph(
            2,
            &[1],
            vec![
                arc(0, 1, Some('a'), tags(&[5], &[1])),
                arc(0, 1, Some('a'), tags(&[5], &[1])),
                arc(0, 1, Some('a'), tags(&[5, 6], &[2])),
            ],
        );
        let r = read_out(&g, "a", ReadoutMode::Single);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0], group(&[5, 6], &[1, 2]));
    }

    #[test]
    fn self_loop_tags_count_only_when_on_a_surviving_path() {
        // State 1 has a self-loop writing 'b' with rule 4; state 2 (dead end) has one with rule 5.
        let g = graph(
            4,
            &[1],
            vec![
                arc(0, 1, Some('a'), tags(&[1], &[])),
                arc(1, 1, Some('b'), tags(&[], &[4])),
                arc(0, 2, Some('a'), tags(&[2], &[])),
                arc(2, 2, Some('b'), tags(&[], &[5])),
                arc(0, 3, Some('a'), ArcTags::default()),
            ],
        );
        assert_eq!(
            read_out(&g, "abb", ReadoutMode::Single),
            vec![group(&[1], &[4])]
        );
        // With no 'b' written, the loop is not used.
        assert_eq!(
            read_out(&g, "a", ReadoutMode::Single),
            vec![group(&[1], &[])]
        );
    }

    #[test]
    fn epsilon_cycle_is_closed_and_terminates() {
        // 1 <-> 2 on epsilon with rules; 2 accepts. Both rules lie on accepting walks.
        let g = graph(
            3,
            &[2],
            vec![
                arc(0, 1, Some('a'), tags(&[1], &[])),
                arc(1, 2, None, tags(&[], &[1])),
                arc(2, 1, None, tags(&[], &[2])),
            ],
        );
        assert_eq!(
            read_out(&g, "a", ReadoutMode::Single),
            vec![group(&[1], &[1, 2])]
        );
    }

    #[test]
    fn root_tag_position_on_the_path_does_not_matter() {
        // C71: moving the root tag from the first arc of the entry to its last arc must
        // not change the Single group.
        let early = graph(
            3,
            &[2],
            vec![
                arc(0, 1, Some('a'), tags(&[1], &[])),
                arc(1, 2, Some('b'), tags(&[], &[3])),
            ],
        );
        let late = graph(
            3,
            &[2],
            vec![
                arc(0, 1, Some('a'), tags(&[], &[])),
                arc(1, 2, Some('b'), tags(&[1], &[3])),
            ],
        );
        let e = read_out(&early, "ab", ReadoutMode::Single);
        assert_eq!(e, read_out(&late, "ab", ReadoutMode::Single));
        assert_eq!(e, vec![group(&[1], &[3])]);
    }

    #[test]
    fn same_state_at_two_positions_is_two_nodes() {
        // Self-loop 'a' at state 1 with rule 7; accepting 2 after 'b'. "ab" must not take the
        // loop; "aab" must. A state-only (not layered) filter would give rule 7 for both.
        let g = graph(
            3,
            &[2],
            vec![
                arc(0, 1, Some('a'), ArcTags::default()),
                arc(1, 1, Some('a'), tags(&[], &[7])),
                arc(1, 2, Some('b'), ArcTags::default()),
            ],
        );
        assert_eq!(
            read_out(&g, "ab", ReadoutMode::Single),
            vec![group(&[], &[])]
        );
        assert_eq!(
            read_out(&g, "aab", ReadoutMode::Single),
            vec![group(&[], &[7])]
        );
    }

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, m: u32) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) as u32) % m
        }
    }

    fn random_graph(rng: &mut Lcg) -> TrimmedGraph {
        let ns = 2 + rng.next(4);
        let n_arcs = 2 + rng.next(9);
        let mut arcs = Vec::new();
        for _ in 0..n_arcs {
            let written = match rng.next(5) {
                0 => None,
                1 | 2 => Some('a'),
                _ => Some('b'),
            };
            let mut t = ArcTags::default();
            if rng.next(3) == 0 {
                t.roots.push(LexEntryId(rng.next(4)));
            }
            if rng.next(2) == 0 {
                t.rules.push(MRuleId(rng.next(5)));
            }
            if rng.next(6) == 0 {
                t.rules.push(MRuleId(rng.next(5)));
            }
            arcs.push(arc(rng.next(ns), rng.next(ns), written, t));
        }
        let accepting: Vec<u32> = (0..ns).filter(|_| rng.next(3) == 0).collect();
        TrimmedGraph {
            num_states: ns,
            start: 0,
            accepting,
            arcs,
        }
    }

    #[test]
    fn random_small_graphs_match_explicit_enumeration() {
        let mut rng = Lcg(0x5eed);
        let strings = ["", "a", "b", "ab", "ba", "aa", "aab", "abb", "abab"];
        let mut accepted = 0;
        for _ in 0..250 {
            let g = random_graph(&mut rng);
            for s in strings {
                let got = read_out(&g, s, ReadoutMode::Single);
                let want = oracle(&g, s);
                assert_eq!(got, want, "graph {g:?} string {s:?}");
                accepted += got.len();
            }
        }
        assert!(accepted > 80, "generator too sparse: {accepted} accepted");
    }
}
