# 109: The affix-template walk drops a partial whose required slot can no longer find its material

Kind: optimization (argued parse-preserving).
Status: optimized-in-rust. Machine PR #575 (open, "Closes #561") is the C# change; this ports it.

## C# site

`AnalysisAffixTemplateRule.cs` (`Apply`, `ApplySlots`, `ParallelApplySlots`) and `RequiredSlotMaterial.cs`
on Machine branch `perf/hc-required-slot-lookahead` (`0bf9e646`). `Morpher.RequiredSlotLookahead` is
`Off`, `Shadow` or `On`.

## Rust site

`pg-rules/src/required_slots.rs` (`RequiredSlotMaterial`, `RequiredSlotLookahead`, the shadow counts),
`pg-rules/src/stratum/required_slot_check.rs` (`required_slot_cannot_unapply`), the per-template walk
`template_unapply_slots`, and `RuleCache::template_material`. `pg_parse::Morpher::with_required_slot_lookahead`
sets the mode (default `On`; `HC_RSL=off|shadow|on` overrides it for diagnostics).

## What changed

Per template, per slot, the tables hold the constraint lanes of the segments the slot's rules insert
(the top-level constraints of the allomorph's analysis LHS, so the same lanes the matcher uses) and a
purity flag. A partial is dropped when some required slot still to be walked has no allomorph whose
every inserted-segment constraint unifies with a segment node of the partial's shape. The test runs
at the template entry and after each slot unapplies, from the next slot to unapply downwards, and
stops after the first impure slot. Each shape node tested costs one work unit, so the check's own cost
is in `work_steps`; if the budget runs out mid-test the partial is kept.

The per-template walk (tracing, and the tests' own-walk oracle) applies it directly; the shared walk of 095 carries it as a per-template dead mask (110).

## Why it cannot change a parse

The analysis pattern of an allomorph holds one top-level constraint per inserted segment, and the
rule can only unapply if each constraint unifies with a segment node of the shape. A rule is pure when
every right-hand action copies an input part or inserts material, and every left-hand part holding a
segment is copied and must capture at least one segment. Then `generate_shape` only copies input nodes
and never untruncates a segment, and a constraint that unifies with no node of the shape unifies with
none after any number of pure unapplications. Inside one template walk only that template's slot
rules run, and the stratum's phonological rules ran before it. So the test for slot j is used only
while every slot walked since is pure. Optional nodes count as present, and a null affix, a
compounding rule or an allomorph whose analysis pattern did not build leaves the slot unprunable.

Rust's `untruncate` differs from C# in one respect that matters here: a `Segments` part and an anchor
restore nothing. The purity test keeps the C# conservative rule (any part holding a segment must be
copied and capture one), which is sound for both.

## Evidence

- Shadow mode (the look-ahead is evaluated and counted, never prunes): every conformance fixture run
  and variant, 141 runs, 237 fired checks, 0 violated and 0 completed under a fired check
  (`pg-parse/tests/required_slot_shadow_gate.rs`); the five reference corpora likewise
  (`HC_RSL=shadow pangloss batch`, `HC_STEP_STATS=1` prints `RSL_SHADOW`). Per-word analysis
  multisets equal main's on every corpus.
- `work_steps` per corpus is in the slice report (`result-pg-s2-template-walk.md`).

## Coverage

`pg-rules/src/stratum/required_slot_lookahead_tests.rs`: pruning at the root skips the optional slots
(fewer `work_steps`, same outputs); present material prunes nothing; an impure slot between stops the
test (`an_impure_slot_between_stops_the_look_ahead` fails when purity is ignored, as do the sweep and
the shadow test); null-affix and all-optional templates are never pruned; every template over every
word up to four segments gives the same outputs on and off; shadow counts no violation.
