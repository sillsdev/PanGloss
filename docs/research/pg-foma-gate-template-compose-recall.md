# `gate_template::recall_reachable` — why `apply_up`, not `fsm_intersect`

`recall_reachable` (`rust/crates/pg-foma/tests/common/gate_template.rs`) checks whether a candidate
tag sequence is reachable from a surface string through a composed net. The technique: fix the
net's lower (surface) tape to exactly one word via `fsm_compose` against a small linear identity
transducer (`O(|net states| * |word length + 1|)`, independent of the net's total path count), then
project the result's upper tape. This is safe where a direct `apply_up` search over the *full* net
is not, because the restricted-and-projected net is tiny by construction.

The first implementation finished with `fsm_intersect` against a linear acceptor for the expected
tag string, then `fsm_isempty`. That is wrong on a structural-composite entry (one whose lexc
encoding pairs the whole literal surface span identity-wise — upper and lower carry the same
phoneme characters after the leading tag arcs, unlike token-space emitters where non-tag positions
are epsilon-upper): the projected upper net still contains one arc per phoneme position between and
after the real tag arcs. These are epsilon-labelled in effect but are real forward-advancing
transitions, not removed by `fsm_minimize` alone. `fsm_intersect`'s synchronized product does not
appear to epsilon-close across these before pairing states with the tag acceptor's epsilon-free
path, so the intersection comes back empty even though the tag sequence is genuinely reachable.
Verified directly: `apply_init(&upper_net).up(&concatenated_tag_string)` — a proper epsilon-closing
search — finds it on the same projected net, for every case `fsm_intersect` missed.

`recall_reachable` therefore finishes with an `apply_up` search, but only on the already
word-restricted upper net (a handful of states), never on the full composed net, whose search space
`apply_up` cannot safely traverse. `tag_string_fsm` remains available as a reusable acceptor builder
for a future gate shaped like the original diagnostic this technique was adapted from (token-space
only, no structural composites), where the epsilon issue above does not arise.

**Trade-off.** This proves reachability of one expected tag sequence for one surface string, not
`FomaProposer` candidate-set fidelity — the real proposer could enumerate that reachable path plus
spurious ones, or fail to terminate trying. A gate about `FomaProposer` behavior itself should call
`FomaProposer::propose` directly; a gate asking "does the net I built even relate this surface
string to this analysis" should use `recall_reachable`.
