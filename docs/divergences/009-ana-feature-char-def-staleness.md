# 009 — Analysis-side `char_def` staleness in rewrite rules

## Kind
Behavioural.

## Status
Fixed-in-rust.

## C# site
`CharacterDefinitionTable.GetMatchingStrReps`, always re-derived from a node's current feature
struct — the same always-lane-based mechanism cited in entry 007.

## Rust site
`pg_rules::rewrite::ana_feature` (`rust/crates/pg-rules/src/rewrite.rs`) and
`pg_parse::root_trie::RootAllomorphIndex::search`.

## What differs
`ana_feature` correctly widens a changed feature's lanes to full-mask on analysis-unapply (the
documented "underspecify on unapply" behavior) but, before the fix, never touched the node's
`char_def`/`cd_set`. Confirmed via direct calls: unapplying a rule that changes "v" back toward "p"
produced a node whose lanes were correctly widened but whose `char_def` was still literally "v"'s.
Root-allomorph lookup keys off that literal `char_def`, so it returned zero matches for the
widened-but-still-"v" node even though a "p"-rooted lexical entry exists and the node's lanes are
lane-compatible with "p".

**Precise rule each side follows:** C#'s root lookup is pure lane-based `FeatureStruct` unification,
with no separate char-def-identity gate — `GetMatchingStrReps` is always re-derived. Pre-fix Rust's
root lookup kept relying on a node's literal (and, after unapply, stale) `char_def` even when the
node's lanes had already moved on.

## Can it change a parse?
Yes: an analysis-side reconstruction could never find a lexical root whose underlying segment
differs from the word's own surface segment at that position — exactly the shape a feature-changing
phonological rule creates.

## Evidence
Fix: `ana_feature` now clears the changed node's `char_def` to `NO_CHAR_DEF` after widening its
lanes, mirroring `syn_feature`'s pre-existing identical clearing, so root lookup falls back to pure
lane unification. This fixed `common_feature_rules` and the `boundary_rules` sub-cases that depended
only on char_def staleness (`csharp_port_rewrite.rs`). `ana_narrow`/`ana_epenthesis` needed no
change.

## Upstream
None, not applicable — pure Rust-side bug; C# was already correct via its always-lane-based
`GetMatchingStrReps`.

## Notes
This crate's `MutNode` (`rewrite.rs`) carries no separate `cd_set` column, unlike
`pg-rules/src/morph.rs`'s `OutNode` (entry 007), which needed the fuller `ctx_cd_set`-based fix — the
two fixes are structurally parallel but not code-shared, because the two node representations
differ. `anchor_rules` (entry 010), the remaining `boundary_rules` sub-cases (entries 011-012), and
the epenthesis findings (entries 013-014) each turned out to have separate, deeper root causes not
reached by this fix alone.
