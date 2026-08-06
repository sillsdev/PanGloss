# pg-fst optimize.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-fst/src/optimize.rs` implementation comments so the
source can carry a one- or two-line pointer instead of the full argument.

## `min_hops_to_accept`: why ignoring arc constraints still gives an admissible bound

The freeze-time BFS that computes each state's minimum-hops-to-accept runs on the reversed arc
graph from every accepting state, ignoring arc constraints entirely (`u32::MAX` marks a dead state
no accepting state can reach). Dropping constraints only ever *adds* edges relative to what any
real input could actually traverse, so the resulting distance never over-estimates the number of
arcs a real thread still needs — it is admissible by construction, not merely a heuristic. All arcs
are unit-cost because the frozen automaton is epsilon-free, which makes plain BFS exact rather than
just a bound, in O(states + arcs), computed once per compile. This lower bound feeds the
nondeterministic traversal's pruning (`Fst::min_hops_to_accept` / `traverse.rs`).
