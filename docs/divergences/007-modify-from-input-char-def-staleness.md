# 007 — `ModifyFromInput` never rendered a modified segment (char-def staleness)

## Kind
Behavioural.

## Status
Fixed-in-rust.

## C# site
`SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs` and `CharacterDefinitionTable.GetMatchingStrReps`,
which always re-derives renderable representations from a node's **current** feature lanes, never
from a cached/stale literal identity.

## Rust site
`pg_rules::morph::copy_part` (`rust/crates/pg-rules/src/morph.rs`), which builds the `OutNode` for a
`Modify`-produced segment, and `pg_shape::Shape::node_cd_set`
(`rust/crates/pg-shape/src/lib.rs`), and `pg_parse::surface::matching_str_reps`.

## What differs
Every sub-case needing `ModifyFromInput` to change a segment to a different character (e.g. "p" ->
"b") produced an empty `Morpher::parse_word` result, even though the underlying lane-level
modification itself was already correct. Root cause: a `Modify`-produced `OutNode` kept the source
node's own `char_def` unchanged; `Shape::node_cd_set` treats any node whose `char_def != NO_CHAR_DEF`
as an implicit singleton of that original char-def, ignoring the node's actual (updated) feature
lanes entirely. `pg_parse::surface::matching_str_reps` therefore restricted a modified segment's
renderable representations to its **pre-modification** character forever — a modified "p" always
printed/matched as "p", never "b", regardless of how its lanes changed.

**Precise rule each side follows:** C# always re-derives a node's renderable string reps from its
current feature struct (`GetMatchingStrReps` has no cached-identity fast path). Pre-fix Rust cached
the pre-modification literal identity and never invalidated it on a feature-changing modification.

## Can it change a parse?
Yes: this made an entire class of `ModifyFromInput` rules that change a segment's surface identity
produce zero analyses, a straightforward and severe recall loss (not merely a rendering cosmetic,
since the wrong/stale representation also fails to match the actual surface string at synthesis
confirm time).

## Evidence
`csharp_port_affix_process.rs::simulfix_rules`/`modify_from_input_rules` (porting
`AffixProcessRuleTests`) fail before the fix, pass after.
`pg-rules/tests/morph_gate.rs::simulfix_synthesis_voices_target_segment` independently confirms the
underlying lane-level modification was already correct even while the surface-rendering bug hid it —
useful for narrowing which layer actually held the bug.

## Upstream
None, not applicable — pure Rust-side bug in the surface-rendering layer, which has no C# analog
(the fix is `char_def: NO_CHAR_DEF` plus a context-derived `cd_set`, mirroring how
`OutputAction::InsertContext` is handled immediately below it in the same match arm).

## Notes
See entry 009 for the analysis-side twin of this same char-def-staleness class of bug (in
`rewrite.rs::ana_feature` rather than `morph.rs::copy_part`), found and fixed separately.
