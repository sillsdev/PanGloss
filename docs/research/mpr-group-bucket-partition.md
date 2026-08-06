# `mpr_group_buckets`: partitioning MPR features the way C#'s `GroupBy` does

`pg-grammar::model::mpr_group_buckets` is the shared partitioning step behind `mpr_required_ok`,
`mpr_excluded_ok`, and `mpr_add_output` — the group-aware replacements for every
`requiredMPRFeatures`/`excludedMPRFeatures` check. It is a free function over `&[MprGroup]` rather
than a `Grammar` method so it is unit-testable without standing up a full loaded `Grammar`;
`Grammar`'s own methods are thin `&self.mpr_groups` wrappers around the same free functions, which
are the actual call sites reached from `pg-rules`.

## What it does

It partitions a candidate bit set (`test: MprSet`) the way C#'s `this.GroupBy(mf => mf.Group)`
does: every bit belongs to at most one `MprGroup`. It returns `(ungrouped_bits, [(match_type,
bucket_bits), ...])`, where `ungrouped_bits` is exactly C#'s `Group == null` bucket. That bucket
always uses "All" semantics in both callers, alongside true `All`-type groups.

## The deliberate simplification

A well-formed grammar never puts one MPR feature in two groups. If it did, C#'s last-write-wins
`MprFeature.Group` backpointer would decide which group owns it — but no FLEx-emittable grammar
does this, so this port simplifies to: the **first** declared group claims a shared bit. That
divergence is flagged in code (`owned` tracks claimed bits so a later group cannot reclaim them),
not silently assumed to be unreachable.
