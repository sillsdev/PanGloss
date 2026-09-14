# 008 — Morph-attribution drop on input-morph subsumption

## Kind
Behavioural.

## Status
Fixed-in-rust.

## C# site
`SynthesisAffixProcessAllomorphRuleSpec.ApplyRhs` (cs:185-205), specifically `MarkSubsumedMorph`
(marks a morph as a child of a new morph, rendering before its host in postorder) and
`MarkMorph(Shape.First)` for pure truncation.

## Rust site
`pg_rules::morph::attribute_morphs` (`rust/crates/pg-rules/src/morph.rs`) and the `MorphStatus` enum
(`pg-rules/src/word.rs`).

## What differs
Two residuals surfaced after fixing entry 007's char_def bug: (a) "tags"/"tagsv"/"tag" recovered
only a "PAST"-style morph set with the `u_suffix`-chained "3SG" component missing; (b) "bubib"
dropped the pure-deletion rule's own "PRES" morph. Both share one root cause: `attribute_morphs`
lacked a way to record that one rule's output *fully subsumes* (captures without any of its own
independent surface material, or via pure deletion) a prior rule's morph.

- (b): a pure-truncation rule's own allomorph was never recorded at all — fixed by porting C#'s
  floating-marker mechanism as `MorphStatus::Floating`.
- (a): on synthesis-confirm of tag+u+s, `s_suffix` captures the "u" (3SG's entire realization) as
  part "2" and never copies it forward, so the 3SG record contributed zero output positions and was
  silently dropped even though the analysis chain itself was correct. C# handles this via
  `MarkSubsumedMorph` (the subsumed morph becomes a child, rendered before its host) or
  `MarkMorph(Shape.First)` for pure truncation.

Ported as `MorphStatus::SubsumedChild`/`SubsumedFirst`.

## Can it change a parse?
Yes — a dropped morph is a dropped gloss component in the output analysis, an incomplete (and thus
wrong) morpheme sequence even when the surface string and overall parse succeed.

## Evidence
`csharp_port_affix_process.rs::subsumed_affix_findings` pins both sub-cases. Regression witness
named directly in the source doc: dropping the `Real`-with-no-runs fallback arm in
`attribute_morphs` returns "tags" to the wrong `{"47 PAST"}` and "tag" to `{"47 PRES", "47"}`.
Fixture: `rust/conformance/affix-shapes/truncate/` for sub-case (b).

## Upstream
None, not applicable — pure Rust-side bug (an under-ported mechanism), fixed by completing the port
of `MarkSubsumedMorph`/`MarkMorph(Shape.First)`.

## Notes
Directly related to entry 006 (both concern `attribute_morphs`' record bookkeeping for morphs whose
material doesn't map onto one contiguous, un-subsumed span). Together they cover the bulk of the
"MarkMorphs"-family behavior this port needed to get right.
