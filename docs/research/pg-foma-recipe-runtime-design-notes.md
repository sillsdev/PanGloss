# pg-foma recipe_runtime.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/src/recipe_runtime.rs` implementation comments
so the source can carry a one-line pointer instead of the full argument. Each section corresponds
to one call site; the site names the function/constant so this doc can be found from either
direction.

## `finished_net_digest`: what is in the preimage, and why that is sufficient

Two nets with the same digest have, byte for byte, the same `arity`, the same size/property
counters, the same tri-state flags, the same `sigma` (number to symbol, in order), the same
minimum-edit-distance table, and the same compressed line table — every state block and every arc
label/target, in order. That is the entire automaton plus the entire symbol table, so `apply_up`
over any query must return the same raw paths in the same order, and therefore the same proposals,
the same confirmation calls, and the same `confirmation_steps`/`raw_paths`. Arc order is included
deliberately and not normalized: it is observable (it fixes the order raw paths come back in) and
normalizing it would make the digest claim an equivalence the measurement does not have.

Two exclusions. `name` is a cosmetic label: `apply` never reads it, and including it would let a
per-candidate naming convention defeat every dedup opportunity while claiming to protect something.
Nothing else is excluded — `medlookup` is only read by minimum-edit-distance lookup, which this
pipeline never calls, but it is a handful of `i32`s and is hashed anyway rather than argued about.

A cryptographic hash is used rather than a cheaper structural one because a collision here would
hand one candidate another's score; SHA-256 makes that negligible where a 64-bit structural hash
would not.

## `realize_plan_composed`'s `prepare_network_for_apply` call is worth keeping despite measuring inert

`FomaProposer::new` (the hand-spun path) calls `prepare_network_for_apply`; `from_precompiled_network`
— the constructor every plan-composed candidate goes through — deliberately did not, so above
`ARC_SORT_MIN_ARCS` the hand-spun baseline got foma's binary-search arc traversal and the
plan-composed candidate compared against it did not. Adding the same call here closes that
asymmetry, not a measured hot spot in its own right.

Measured, and the measurement is a null result on everything checked in: of the 45 discoverable
conformance fixtures that build a plan-composed net, zero cross `ARC_SORT_MIN_ARCS` (10,000) — the
largest is 479 arcs. Verified by reading `net.arcs_sorted_out` directly: on those nets it is `false`
as built, still `false` after this call, and only `true` under a forced `fsm_sort_arcs`. So on the
current fixtures this line is provably inert, and the speedup figure in `ARC_SORT_MIN_ARCS`'s own
doc says nothing about them. It engages only on a large real grammar — a private test corpus's
plan-composed net reaches five figures of arcs — which is why the call is worth having despite
buying nothing in CI.

## Net-level dedup: what a donor measurement can and cannot carry over

Plan-shape recipes are erased by minimization — measured spread 0 across 8 fixtures, with every
Indonesian plan-composed permutation landing on identical states/arcs and identical proposals — so
by the time a candidate's network is realized, several candidates in one run routinely hold
networks that are identical arc for arc. Everything downstream (propose, confirm, the whole corpus
traversal, the certification) is a pure function of (network, grammar, corpus), so an earlier
candidate's measurement IS this candidate's measurement, and score attribution is trivially sound —
identical networks legitimately have identical deterministic scores, so nothing becomes a function
of evaluation order. This is why the dedup unit is the whole measurement rather than confirmation
memoized by proposal set: a set-difference scheme would be sound as a result and unsound as a
measurement, exactly what `Score::key`'s work-not-time design exists to prevent.

Two things are never inherited from the donor, because both would smuggle a clock into a verdict:

- `build` is this candidate's own measured compile time — its network still had to be built to know
  the digest, so that number is real and never the donor's.
- The breach ladder is re-run over the reconstructed score, never copied. The production optimizer
  passes a wall-clock allowance that declines as the run proceeds, so serving the donor's
  certification verbatim would let a late candidate inherit an early candidate's larger allowance.

`apply` is reported as 0, never as the donor's reading, for three reasons in order of weight. It is
literally true — no corpus traversal happened, matching this module's existing "honestly 0, not
'not yet measured'" convention for a zeroed `apply`. `pg_cli`'s evaluator charges the optimizer's
wall-clock allowance by `build + apply`, so charging a deduped candidate the donor's apply time
would bill the run for work it never did and cut exploration short — throwing away most of what
this optimization is for. And it cannot mislead a ranking, because `Score::key` excludes `apply` by
design. The cost is that `apply` stops being a usable per-candidate traversal-cost diagnostic for a
deduped candidate; `net_dedup_savings` is where that cost now lives, in deterministic units.
Consequently a hit is refused outright when an `apply` limit is declared, since serving one could
let a deduped candidate pass a budget an unduplicated one would have failed — nothing in production
sets that limit today, so this costs nothing now and cannot rot into a wrong answer later.

Dedup is scoped to this net's measurement only, never to the candidate's final routing: the
selectability/role logic downstream reads the plan and the candidate's declared role, neither of
which the network determines, so two candidates with identical nets can legitimately route
differently. Dedup replaces the corpus pass and nothing else; every candidate still runs its own
routing over the result.

