# pg-foma prefilter_census.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/examples/prefilter_census.rs` implementation
comments so the source can carry a one-line pointer instead of the full argument. Each section
corresponds to one call site; the site names the function/type so this doc can be found from either
direction.

## Module doc: what this census measures, and the counterfactual-timing methodology

This measures, per grammar, what fraction of failing-candidate confirm time is spent on candidates a
cheap deterministic predicate (env/co-occurrence/stem-name/bound-root — `pg_rules::validity`'s gate)
could have rejected before the engine ever ran (category a), versus candidates where the
unapply/synthesis cascade never produced a derivation at all (category b), versus everything else
(category c).

**Why this isn't just "sum per-candidate `confirm_all` times".** `pg_foma::confirm::confirm_batch`'s
whole point is that batching/fusion (root-set grouping, `RULE_UNION_SLACK` sub-chunking,
cross-root-set fusion) makes real production confirm time much less than the sum of per-candidate
reparses. Timing per-candidate unbatched calls would measure a workload the real filter never runs
in, and the bias direction (inflating category b, which repeats cascade work every unbatched call, or
inflating category a via repeated synthesis) can't be signed — an untrustworthy census near a
go/no-go threshold this close to the noise floor.

So time comes from a counterfactual under the real batched/fused confirm, per word:
- `baseline` = `confirm_batch(all candidates)`
- `keep_confirming` = `confirm_batch(only candidates that end up confirming)`
- `minus_x` = `confirm_batch(all candidates except the ones classified category x)`, for each of x in
  {a, b, c}
- category x's share of failing time = `(baseline - minus_x) / (baseline - keep_confirming)`

This is "how much of the failing-candidate time would a perfect category-x predicate have saved, run
through the real fused confirm path". It is not required to be perfectly additive across a+b+c
(removing candidates changes which chunks fuse, so marginals overlap or leave gaps); the ratio is the
decision signal, not the sum.

**Classification** (which category a failing candidate belongs to) is a separate, untimed pass using
`pg_foma::confirm::confirm_one_traced`: first a cheap untraced call reads
`ParseOutcome::candidates_generated` (no tracing overhead) — `== 0` is category b by construction. Only
if `candidates_generated > 0` does a second, traced call run (tracing forces `merge_equivalent = false`
and disables the analysis memo, so this expensive path is scoped to the smaller subset that needs it).
The resulting trace tree's `Failed` nodes are walked for `FailureReason`s; precedence is (a)
validity-gate reasons > (b) cascade/surface reasons > (c) everything else — a candidate reaching a
validity-gate `Failed` node on any explored branch demonstrates a deterministic predicate over its own
fixed morpheme/allomorph set could have rejected it, independent of which cascade branch happened to
reach the gate.
