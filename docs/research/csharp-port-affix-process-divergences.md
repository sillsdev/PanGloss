# Affix-process port divergences (`pg-parse/tests/csharp_port_affix_process.rs`)

Findings from porting `AffixProcessRuleTests` from the C# HermitCrab test suite
(`tests/SIL.Machine.Morphology.HermitCrab.Tests/MorphologicalRules/AffixProcessRuleTests.cs`).
Each entry names the divergence, its root cause, and the fix.

## `ModifyFromInput` never rendered a modified segment (`simulfix_rules`, `modify_from_input_rules`)

Every sub-case needing `ModifyFromInput` to change a segment to a different character (e.g. "p" ->
"b") produced an empty `Morpher::parse_word` result, even though the underlying lane-level
modification was already correct (`pg-rules/tests/morph_gate.rs::
simulfix_synthesis_voices_target_segment`, which asserts on `node_lanes` directly, never on the
rendered surface string).

Root cause: a `Modify`-produced `OutNode` (`pg-rules/src/morph.rs::copy_part`) kept the source
node's own `char_def` unchanged, and `Shape::node_cd_set` (`pg-shape/src/lib.rs`) treats any node
whose `char_def != NO_CHAR_DEF` as an implicit singleton of that original char-def, ignoring the
stored `cd_set` entirely. `pg_parse::surface::matching_str_reps` therefore restricted a modified
segment's renderable representations to its pre-modification character forever, regardless of how
its lanes changed — so a modified "p" always printed/matched as "p", never "b".

Fix: `Modify`'s `OutNode` now gets `char_def: NO_CHAR_DEF` plus a context-derived `cd_set`
(`ctx_cd_set`), mirroring `OutputAction::InsertContext`'s handling immediately below it in
`synth_affix_allomorph`'s match arm.

## Morph-attribution drops on input-morph subsumption (`subsumed_affix_findings`)

Two residuals surfaced after the `char_def` fix above: (a) "tags"/"tagsv"/"tag" recovered only a
"PAST"-style set with the `u_suffix`-chained "3SG" component missing, and (b) "bubib" dropped the
pure-deletion rule's own "PRES" morph. Both are the same root-cause family in
`pg_rules::morph::attribute_morphs`, not an analysis-cascade gap:

- (b): a pure-truncation rule's own allomorph was never recorded — fixed by porting C#'s floating
  marker as `MorphStatus::Floating` (fixture `rust/conformance/affix-shapes/truncate/`).
- (a): the input-morph-subsumption half of the same C# branch
  (`SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs`, cs:185-205): on the synthesis-confirm of
  tag+u+s, `s_suffix` captures the "u" (3SG's entire realization) as part "2" and never copies it,
  so the 3SG record contributed zero output positions and was silently dropped even though the
  analysis chain itself was fine. C# marks such a morph via `MarkSubsumedMorph` (child of the new
  "s" morph, rendering "3SG" before its host, postorder) or `MarkMorph(Shape.First)` for pure
  truncation. Ported as `MorphStatus::SubsumedChild`/`SubsumedFirst` (see `attribute_morphs`'s own
  doc).

Regression witness: dropping the `Real`-with-no-runs fallback arm in `attribute_morphs` returns
"tags" to `{"47 PAST"}` and "tag" to `{"47 PRES", "47"}"`.
