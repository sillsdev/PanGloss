# `mpr_group_buckets`: partitioning MPR features the way C#'s `GroupBy` does

`pg-grammar-model::model::mpr_group_buckets` is the shared partitioning step behind `mpr_required_ok`
and `mpr_excluded_ok` — the group-aware replacements for every
`requiredMPRFeatures`/`excludedMPRFeatures` check. It is a free function over `&[MprGroup]` rather
than a `Grammar` method so it is unit-testable without standing up a full loaded `Grammar`;
`Grammar`'s own methods are thin `&self.mpr_groups` wrappers around the same free functions, which
are the actual call sites reached from `pg-rules`.

## What it does

It partitions a candidate bit set (`test: MprSet`) the way C#'s `this.GroupBy(mf => mf.Group)`
does: every bit belongs to at most one `MprGroup`. It returns an ungrouped set and a lazy
iterator of `(match_type, bucket_bits)` pairs.
The ungrouped set is computed first by removing every group's members; this is the same
remainder as removing the union of first-owned buckets. Lazy group construction avoids a
heap-allocated bucket list and leaves unvisited buckets unmaterialized after a short circuit.
The ungrouped set is exactly C#'s `Group == null` bucket. That bucket
always uses "All" semantics in both callers, alongside true `All`-type groups.

## The deliberate simplification

A well-formed grammar never puts one MPR feature in two groups. If it did, C#'s last-write-wins
`MprFeature.Group` backpointer would decide which group owns it — but no FLEx-emittable grammar
does this, so this port simplifies to: the **first** declared group claims a shared bit. That
divergence is flagged in code (`owned` tracks claimed bits so a later group cannot reclaim them),
not silently assumed to be unreachable.

## Optional observations

`mpr_match` owns each bucket's boolean decision for both observed and unobserved callers.
It uses borrowed subset/overlap predicates, then materializes blocked-member sets only when
an observer is present. Passing buckets need no blocked members. `mpr_required_ok` and
`mpr_excluded_ok` pass no observer; `mpr_match_observed` attaches one without changing the
decision or the ungrouped-first short circuit. `stats_off_mpr_allocation_regression` checks
allocator effects with inputs constructed before measurement, including low requirements
against borrowed wide candidates in both All and Any groups.
