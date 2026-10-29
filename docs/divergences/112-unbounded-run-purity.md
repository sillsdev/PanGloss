# 112: A slot whose unmatched parts are unbounded runs counts as pure for the required-slot look-ahead

Kind: optimization (argued parse-preserving).
Status: optimized-in-rust. Machine PR #579 (open, branch `perf/hc-impure-slot-lookahead`, commit `faa24998`) is the C# change; it refines entry 109.

## C# site

`RequiredSlotMaterial.cs` (`IsPureAllomorph`, `RestoredSegments`) on Machine `perf/hc-impure-slot-lookahead`.

## Rust site

`pg-rules/src/required_slots.rs` (`is_pure_allomorph`, `restored_segments`).

## What changed

Entry 109 stopped the look-ahead behind any slot with a left-hand part that holds a segment and is not
copied (or may capture nothing), because the analysis of such a part writes segments back. The test now
counts what the Rust `untruncate` writes for the part and treats the allomorph as pure when that is zero.
The count mirrors the Rust function, which differs from C#: a segment context or segment character
definition writes one node, a bounded quantifier writes `max` copies of its children, and an unbounded
quantifier, a segment string (`PatternNode::Segments`) and an anchor write none. A slot whose deleted or
possibly empty part is an unbounded run (Mbugwe's vowel-fusion slots) no longer ends the look-ahead.

## Why it cannot change a parse

Purity exists so that unapplying the slot adds no segment node that is not a copy of an input node. A part
that `untruncate` writes nothing for adds none, however it was captured. A bounded run still writes nodes,
so the slot stays impure.

## Evidence

Shadow: 0 violated and 0 completed under a fired check on every conformance fixture and the five reference
corpora; `work_steps` per corpus is in the slice report.

## Coverage

`pg-rules/src/stratum/required_slot_order_tests.rs`:
`a_slot_deleting_an_unbounded_run_restores_nothing_so_the_look_ahead_goes_on` (fails when the restored
count is ignored; a bounded run stays impure); `the_ordered_look_ahead_never_changes_an_output` covers
the deleter templates.
