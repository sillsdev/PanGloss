# 133: After a swap moves a segment out of its morph, hc.dll rebuilds overlapping morph annotations and later rules see the morph tree's order; PanGloss keeps shape order

Kind: behavioural (hc.dll has a C# defect; PanGloss does not copy it).
Status: permanent until hc.dll changes; upstream sillsdev/machine#650; PanGloss #391 tracks it.

## Decision

When a metathesis moves a segment of one morph past a segment of its neighbour, hc.dll re-creates each morph annotation from its children's ranges as `Range.Create(first child's Start, last child's Start)`. The re-created morphs overlap. `AnnotationList.FindDepthFirst` and the matcher's traversal then walk the annotation tree, not the shape, so a later rule in the same or a later stratum sees the wrong neighbours, and the start position for a resumed search can skip nodes.

PanGloss keeps one shape order for every rule. On 4 of the 2,241 review grammars C# loads, 16 words that v0.27.0 got right now differ: v12-056 (8 words) and v12-100 and v12-333 (1 each) where PanGloss gives an analysis C# does not, and d54-058 (6) where C# gives analyses PanGloss does not. Copying C# would mean emulating its annotation-tree traversal order in every rule after a swap that crosses a morph boundary. Owner decision 2026-11-20 (PanGloss #374, round 7): record it, do not emulate it. A further 22 words in 9 review grammars differ from C# or v0.27.0 only in the morph order or boundary placement inside the signature; v12-014, v12-018 and d54-396 now match C# up to that order, and the rest already disagreed with C# at v0.27.0.

FieldWorks can produce the shape: a metathesis across a morph boundary followed by another phonological rule whose match touches the moved segment.

It is common wherever that shape occurs, not rare. The 16 words above come from the review probes, most of which have no later rule after a cross-morph swap. A random sample (Opus round-7 review, `review-374-opus-probes/r7/`, seed 7) found the mechanism in 6 of 35 loadable metathesis-plus-deletion grammars (17%), 14 words: some are PanGloss parses C# never generates (del7-009 `ssp` = `R1+SU0`, where C# generates `sp`), the rest C# parses PanGloss misses (del7-001 `aps` and `sas`, where C# finds `SU0+R0+SU1` and `R1+SU1+SU0`). A FieldWorks project with metathesis and a later deletion should expect to see it.

### Allomorph environment on the swapped affix

The same overlap changes an affix's own right environment. After a metathesis swaps prefix `s+` into root `ta` (words `t+sa`, `tsa`; final shape `t + s a`), hc.dll's final annotation tree has `ROOT [t..a]` (children `t`, `a`) overlapping and enclosing `PRE [+..s]`. `AllomorphEnvironment.IsMatch` (AllomorphEnvironment.cs:88-96) calls the right matcher at `morph.Range.End.Next`, which is `a`. `Matcher.GetStartAnnotation` (Matcher.cs:326-339) resolves that node with `AnnotationList.Find` (AnnotationList.cs:182-218), which searches only the top-level list and returns the last annotation sorted at or before the offset: `PRE`. `FindDepthFirst` (AnnotationList.cs:225-233) descends only if that annotation contains the offset; `PRE` does not, and `ROOT`, which holds `a`, is never visited. `Find` then fails, the fallback `startAnn.GetNext` (Matcher.cs:331-334) lands on the right anchor, and the matcher starts there: `a` counts as end of input, so a right environment `#` matches and `any`, `A` and `P` fail. Confirmed by instrumenting a scratch build (grammar `PRE-RA-req-none`, word `t+sa`).

PanGloss reads the real segment `a`. In the minimal grammar `PRE-RA-req-none` (prefix PRE inserts `s+`, root `ta`, PRE requires right environment `A`), hc.dll finds no parse for `t+sa` or `tsa` and PanGloss gives `RT+PRE|t+?sa`. Other right environments on the same prefix flip the other way (`#` required: C# parses, PanGloss does not; `A` excluded: C# parses, PanGloss does not; `any` required: C# does not, PanGloss does). The root `tap` with right environments on the root, and left environments on the prefix, agree.

Reach: 32 of the 168 family-2 probe grammars (`PRE-R{A,H,any,AH}-*` right environments on the swapped prefix, and `RAH` or `prepb` with a later affix), always on the words with no later affix applied (`t+sa`, `tsa`). A later affix rebuilds flat annotations, so `t+saa`, `tsaa`, `p+t+sa` and `ptsa` agree in every grammar. Before PanGloss #503 (969d2c1d4) 52 grammars differed; #503 fixed 40 and exposed these 20 newly (and left 12 that now differ only on `t+sa` and `tsa`), net 52 to 32: #503 made PanGloss read the prefix's real next segment, which C# does not. Not emulated, for the reason above: it would need C#'s annotation tree and `Find`-then-descend on every allomorph environment. Added to sillsdev/machine#650. Pinned by `pg-parse/tests/metathesis_prefix_right_env.rs` (PanGloss's answer only; no suite fixture). Evidence: #380 lane `380-later-affix.md` (follow-up 2026-11-26), grammar `380-later-affix-evidence/PRE-RA-req-none/`.

## C# site

`SynthesisMetathesisRuleSpec.ApplyRhs` (rebuilds `newMorphAnn` from `children[0].Range.Start` to `children[last].Range.Start`); `AnnotationList.FindDepthFirst`; the matcher's annotation traversal; for the allomorph-environment case `AllomorphEnvironment.IsMatch` (AllomorphEnvironment.cs:88-96), `Matcher.GetStartAnnotation` (Matcher.cs:326-339) and `AnnotationList.Find` (AnnotationList.cs:182-233).

## Rust site

`pg-rules/src/metathesis.rs` (`synthesize_with_permutation`) and `pg-rules/src/word.rs` (`Word::reorder_morphs`): morphs follow surface order, and every rule matches over the shape.

## Verification

Review probes `fable-evidence/g5/v12-056`, `g5/v12-100`, `g5/v12-333`, `g9/d54-058` in the #374 lane (hc-conformance self-check at Machine `1ddef2bd` against PanGloss). For the allomorph-environment case: `pg-parse/tests/metathesis_prefix_right_env.rs` and the #380 probe `PRE-RA-req-none` (32 of 168 grammars). No staged fixture; sillsdev/machine#650 asks upstream whether the overlap is intended.
