# 133: After a swap moves a segment out of its morph, hc.dll rebuilds overlapping morph annotations and later rules see the morph tree's order; PanGloss keeps shape order

Kind: behavioural (hc.dll has a C# defect; PanGloss does not copy it).
Status: permanent until hc.dll changes; upstream sillsdev/machine#650; PanGloss #391 tracks it.

## Decision

When a metathesis moves a segment of one morph past a segment of its neighbour, hc.dll re-creates each morph annotation from its children's ranges as `Range.Create(first child's Start, last child's Start)`. The re-created morphs overlap. `AnnotationList.FindDepthFirst` and the matcher's traversal then walk the annotation tree, not the shape, so a later rule in the same or a later stratum sees the wrong neighbours, and the start position for a resumed search can skip nodes.

PanGloss keeps one shape order for every rule. On 4 of the 2,241 review grammars C# loads, 16 words that v0.27.0 got right now differ: v12-056 (8 words) and v12-100 and v12-333 (1 each) where PanGloss gives an analysis C# does not, and d54-058 (6) where C# gives analyses PanGloss does not. Copying C# would mean emulating its annotation-tree traversal order in every rule after a swap that crosses a morph boundary. Owner decision 2026-11-20 (PanGloss #374, round 7): record it, do not emulate it. A further 22 words in 9 review grammars differ from C# or v0.27.0 only in the morph order or boundary placement inside the signature; v12-014, v12-018 and d54-396 now match C# up to that order, and the rest already disagreed with C# at v0.27.0.

FieldWorks can produce the shape: a metathesis across a morph boundary followed by another phonological rule whose match touches the moved segment.

It is common wherever that shape occurs, not rare. The 16 words above come from the review probes, most of which have no later rule after a cross-morph swap. A random sample (Opus round-7 review, `review-374-opus-probes/r7/`, seed 7) found the mechanism in 6 of 35 loadable metathesis-plus-deletion grammars (17%), 14 words: some are PanGloss parses C# never generates (del7-009 `ssp` = `R1+SU0`, where C# generates `sp`), the rest C# parses PanGloss misses (del7-001 `aps` and `sas`, where C# finds `SU0+R0+SU1` and `R1+SU1+SU0`). A FieldWorks project with metathesis and a later deletion should expect to see it.

## C# site

`SynthesisMetathesisRuleSpec.ApplyRhs` (rebuilds `newMorphAnn` from `children[0].Range.Start` to `children[last].Range.Start`); `AnnotationList.FindDepthFirst`; the matcher's annotation traversal.

## Rust site

`pg-rules/src/metathesis.rs` (`synthesize_with_permutation`) and `pg-rules/src/word.rs` (`Word::reorder_morphs`): morphs follow surface order, and every rule matches over the shape.

## Verification

Review probes `fable-evidence/g5/v12-056`, `g5/v12-100`, `g5/v12-333`, `g9/d54-058` in the #374 lane (hc-conformance self-check at Machine `1ddef2bd` against PanGloss). No staged fixture; sillsdev/machine#650 asks upstream whether the overlap is intended.
