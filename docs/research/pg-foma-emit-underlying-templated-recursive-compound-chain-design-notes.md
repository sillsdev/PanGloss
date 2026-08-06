# pg-foma emit_underlying_templated_recursive_compound_chain.rs: design notes moved out of comments

Longer arguments pulled out of
`rust/crates/pg-foma/tests/emit_underlying_templated_recursive_compound_chain.rs` implementation
comments so the source can carry a one- or two-line pointer instead of the full argument.

## Module doc: why this file exists

The P6 templated emitter's own "bounded compound loop" (`emit_underlying_templated`,
`pg_foma::emit`) used to hardcode exactly one extra non-head root level, regardless of any
`CompoundingRuleDef`'s own `compounding_max_depth` bound — so it could never propose a genuinely
recursive (>2-stem) compound at all, even though `crate::capability::CompoundingRecursionSafePredicate`
is `ConfirmOnly` unconditionally for `Compounding`. That made the templated path silently
under-propose for a templated grammar that also compounds recursively — the one failure mode
propose-and-confirm cannot recover from (a candidate that is never even offered can never be
confirmed).

`emit_underlying_templated` now reuses `pg_foma::emit::build_compound_chain` — the same shared,
depth-budgeted chain construction `emit_with_budget_profiled` (the production
`SurfaceProbed`/`FomaProposer` path) already used for this, extracted out of that function's own
former closure so both emitters drive one construction, not two that can drift
(`cover_compounding_recursive_depth_bound.rs`'s own `SurfaceProbed`-side regression test is the
sibling of this file for that path).

This file exercises the template-less section's own `TLCmp` chain specifically (no
`<AffixTemplate>` declared at all — `group_keys`/the per-group `G{gi}Cmp` chain only exist when
`g.templates` is non-empty, so a template-less grammar is the minimal vehicle that still reaches
`emit_underlying_templated`'s compounding code path; `has_template_less_section` is true whenever
`has_compounding_rules` is, independent of templates). The per-group `G{gi}Cmp` chain calls the
exact same shared `build_compound_chain` function with the same `compound_extra_levels`/license
arguments (see `emit.rs`'s own "Per-group root sections" comment), so this file's coverage of the
shared function is not narrowed by avoiding templates here.

This file drives `pg_foma::emit::emit_underlying_templated` directly, at a lower level than its
production caller `crate::templated_compile::compile_templated_morphotactics`
(`TemplatedUnderlyingTokens`'s strategy): emit -> `foma::lexcread::fsm_lexc_parse_string` ->
`foma::apply::apply_init` -> `apply_up` -> `pg_foma::tags::decode_path`/`to_candidates`, the same
shape `tests/p6_templated_morphotactics_gate.rs`'s own `run_emit_compile_compose`/`run_spot_check`
helpers use. No phonological rule composition is needed here (this fixture has none), so a bare
`apply_up` against the compiled lexc net alone is the templated path's own analogue of
`FomaProposer::propose`.

## `small_bound_grammar_xml`: the fixture builder

A synthetic `CompoundingRule` grammar with an exact, hand-picked depth bound: one isolated rule,
`multipleApplication="{max_apps}"`, so `compounding_max_depth` = `1 + max_apps` total stems
(`crate::capability::compounding_max_depth`'s own established isolated-rule equivalence, pinned
independently by `cover_compounding_recursive_depth_bound.rs`). `roots.len()` distinct CVCV roots
are freely licensed on both head and non-head sides. Deliberately not wrapped in any
`<AffixTemplate>`, to exercise the template-less `TLCmp` chain.

## `compile_templated`: the emit-through-compile pipeline

Emits `g` via the templated path, asserts the result actually compiled (never `Unsupported`),
foma-compiles the lexc, and returns an `apply_init` handle plus the `SegAlphabet` needed to encode
query words into token space. Factored out since both tests below need it.

## `propose_templated`: the templated path's analogue of `propose`

Every candidate `apply_up` decodes for `word` against the compiled templated network —
`emit_underlying_templated` has no `FomaProposer`/`FomaAnalyzer` wiring at all. The bounded
raw-result cap and wall-clock ceiling mirror `p6_templated_morphotactics_gate.rs`'s own
`run_spot_check` termination discipline.

## `templated_path_proposes_a_bounded_recursive_compound`: the load-bearing recursion-recall proof

`multipleApplication="2"` bounds `compounding_max_depth` at `1 + 2 = 3` stems. Before the templated
chain was fixed, `emit_underlying_templated`'s `TLCmp` chain hardcoded exactly one extra root
regardless of this bound, so it could never propose the genuine 3-stem self-feeding compound below
at all. It now must.

Non-vacuous containment: `Morpher`'s default `max_stem_count` is 2, so the oracle confirms zero
analyses for a 3-root word at the default cap — a containment check against that empty set would be
vacuously true, proving nothing about the templated path's own recall.
`Morpher::with_max_stem_count(3)` (mirroring `cover_compounding_recursive_depth_bound.rs`'s own
identical non-vacuity fix) makes the oracle genuinely accept the 3-stem analysis, so the containment
check is real.

## `templated_path_respects_the_depth_bound_never_proposing_k_plus_one_stems`: the depth-bound-respected gate

The same fixture shape at a smaller, exactly-controlled bound (`multipleApplication="1"` ->
`max_depth = 1 + 1 = 2`, ordinary single-level compounding only) must propose a 2-stem word but must
never propose a 3-stem word: over-approximation is licensed up to the computed bound, never past it.
`build_compound_chain` only ever unrolls `max_depth - 1 = 1` extra non-head level for this grammar,
so a 3-root word is structurally unreachable through the compiled network — mirroring
`cover_compounding_recursive_depth_bound.rs`'s own
`depth_bound_is_respected_a_k_plus_one_stem_word_is_never_proposed` test on the `SurfaceProbed` path.
