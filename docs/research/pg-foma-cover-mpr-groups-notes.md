# pg-foma cover_mpr_groups.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/tests/cover_mpr_groups.rs` implementation
comments so the source can carry a one- or two-line pointer instead of the full argument.

## What this file proves

Proposer-to-confirm containment for `MprGroupOutput::Append`'s `mpr-group.append-output`
configuration predicate (target: `ConfirmOnly` via a non-tracking baseline), plus the
`mpr-group.overwrite-output` witness and the Append/Overwrite order-(in)dependence distinction.

## The non-tracking baseline this file proves, not merely asserts

Neither `crate::gate`'s static root-entry partition (keyed only on `LexEntryDef::mpr`, never an
accumulated derivation-chain value) nor the ordinary morphological affix-allomorph emitter ever
reads `AffixAllomorphDef::required_mpr`/`excluded_mpr`/`out_mpr` at all — every allomorph is offered
unconditionally at every derivation-chain level, gated only by RHS emittability and `Role`
classification. This means propose was already at the safe `ConfirmOnly` baseline before this file
existed: `MprGroupAppendNonNarrowingPredicate` documents and verifies this fact; it does not fix a
narrowing bug, because there is none to fix.

## The synthetic fixture

One stratum, `morphologicalRuleOrder="unordered"` (needed so the cascade itself, not just the MPR
gate, admits both orderings as legal candidates for confirm to weigh — under `Linear`,
`Cascade::permutation`'s non-decreasing-index restriction would already rule out the reverse order
for a reason that has nothing to do with MPR groups, confounding the witness). One `all`-type,
`append`-output `MprGroup` over `{mprX, mprY}`. Two loose suffix rules: `mrP`'s subrule declares no
MPR gate and its output carries `MPRFeatures="mprX mprY"` (`out_mpr` — sets both group members at
once, an `Append` accumulation); `mrQ`'s subrule requires `mprX mprY` (the whole `all`-type group)
via `requiredMPRFeatures`. Root `eK` carries no `ruleFeatures` (starts with an empty MPR set), so
`mrQ` can only apply once `mrP` has already fired and added the group's members — an
order-dependent gate riding on top of an order-invariant accumulation.

Two more roots (`eL`/`eM`) isolate the group-aware `all`-type semantics directly, independent of
`out_mpr` timing: `eL` carries `ruleFeatures="mprX"` (partial group membership, missing `mprY`),
`eM` carries `ruleFeatures="mprX mprY"` (full group membership). Applying `mrQ` directly to each (no
`mrP` involved) proves `Grammar::mpr_group_ok`'s `all`-type fold correctly excludes the partial
match (a flat, group-unaware overlap test would have wrongly admitted `eL`, since `{mprX,mprY}`
overlaps `{mprX}`) — the group-(un)awareness contract, from the ordinary-affix-rule side rather than
the compounding side's own `compound_match` (`tests/cover_compounding.rs`'s
`head_a_word_over_propose_confirm_prune` is the existing group-unaware-side witness for that other
half; `MprSet::compound_match` is out of scope here).
