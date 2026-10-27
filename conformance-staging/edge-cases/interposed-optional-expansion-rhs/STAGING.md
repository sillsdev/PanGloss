# Interposed Optional multi-node expansion RHS

GH#8. The expansion analogue of `interposed-optional-multi-rhs`: analysis of an expansion
`u -> i i` when the unapplied `x -> 0 / i _ i` deletion has first inserted an Optional `x` between
the two `i` positions.

Forward derivation: root `U` has shape `u`; `expand` maps `u` to `ii`; the deletion has no `x` to
remove, so the surface is `ii` with signature `U|ii`. The lexicon has no other root, so `u`, `i`,
`iii` and `ixi` (a real `x` is not deletable by analysis, which only re-inserts Optional ones)
are rejected and act as negative controls.

Oracle provenance: `hc-conformance` self-check built from Machine `aab4c598` (padding the `note`,
`rules: [expand]` and `exercises` fields the pin's schema requires in a scratch copy) passes all
five words, so C# is exact here and the loss in GH#8 is PanGloss-only. Before the fix, HC-Rust
returned no analysis for `ii`: expansion analysis used a positional span, which the interposed
Optional `x` widens past the two RHS positions, so `width_matches` discarded it.
