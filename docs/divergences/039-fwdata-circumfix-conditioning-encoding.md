# 039 — fwdata circumfix conditioning is encoded as an environment union, not as HCLoader's stem pattern

## Kind
Behavioural (loader-side: the grammar PanGloss compiles from `.fwdata` differs from the grammar
HCLoader builds from the same project, so the same word can parse differently).

## Status
Fixed-in-rust. Found 2026-09-16 while converting
`conformance-staging/edge-cases/circumfix-conditioned-halves` to the FieldWorks-producible shape;
fixed the same day in `build_circumfix_allomorphs`/`build_circumfix_lhs`
(`pg-grammar/src/compile/affixes.rs`), pinned by
`pg-grammar/tests/circumfix_conditioning_parity.rs::circumfix_cross_product_with_conditioned_halves_parses_all_four_cells`.

## C# site
`HCLoader.LoadCircumfixAffixProcessAllomorph` (FieldWorks `Src/LexText/ParserCore/HCLoader.cs:1273-1332`).
For each (prefix half, suffix half, prefix env, suffix env) combination it splits each half's
`PhEnvironment` at `_` and embeds the prefix half's RIGHT context and the suffix half's LEFT context
as literal mandatory nodes inside the single `stem` Lhs pattern (`PrefixNull`, `LoadPatternNodes`,
`AnyStar`, `LoadPatternNodes`, `SuffixNull`; :1286-1300). Only the OUTER contexts become one
`AllomorphEnvironment` (:1313-1322); `Environments.Add` is called at most once.

## Rust site
`pg_grammar::compile::affixes::build_circumfix_allomorphs` (`rust/crates/pg-grammar/src/compile/affixes.rs`).
The bug: `lhs = [any_plus]` on every pairing, with BOTH halves' environments (plus `positions`)
pushed onto the one `AffixAllomorphDef.environments` list. The environment-union reasoning was
recorded in `docs/research/circumfix-cross-product-loading.md` ("Corrected: a conditioned half IS
representable"), now marked superseded. The fix ports HCLoader's split into a new
`build_circumfix_lhs` helper, fanning out over each half's resolved environment the way
`resolve_environments` already does for concatenative affixes.

## What differs
Both encodings gate a single pairing identically (prefix run must satisfy the prefix half's
condition, suffix run the suffix half's). They diverge once a rule carries two or more pairings that
share a literal half: under the union encoding the disjunctive-allomorph re-check (entry 030,
`Allomorph.cs:127-152` / `pg-rules/src/validity.rs`) rejects every later-declared cell whose shared
half's environment also holds for an earlier sibling, because the two cells' full environment sets
differ and so do not free-fluctuate. Under HCLoader's encoding the conditions live in the Lhs
pattern, each cell's pattern matches exactly one stem shape, and no environment competition arises.

## Can it change a parse?
Yes. Measured with the C# oracle on the synthetic 2x2 fixture (2 prefix halves x 2 suffix halves,
each half conditioned on the adjacent stem edge): the union encoding parsed one of the four cells and
rejected `puabzo`/`kibamo`/`kibbzo`; the HCLoader encoding parses all four (confirmed both on the
XML-authored path and, after the fix below, on the fwdata-compiled path). A FieldWorks project with a
conditioned circumfix whose halves are reused across pairings would have parsed more words in
FieldWorks than in PanGloss-from-fwdata before this fix. Whether any real project has such an entry
was never verified (`circumfix-cross-product-loading.md` records the same open question for its
source data) and no longer matters for correctness now that both paths agree.

## Evidence
`conformance-staging/edge-cases/circumfix-conditioned-halves/` pins the HCLoader shape
(oracle-verified 2026-09-16, all four cells parse) on the XML-authored-grammar path, which never
went through the buggy loader. The fwdata compile path is reached only by
`pg_grammar::compile_project`, which the XML fixture cannot exercise, so
`pg-grammar/tests/circumfix_conditioning_parity.rs::circumfix_cross_product_with_conditioned_halves_parses_all_four_cells`
builds the same 2x2 shape from a `Snapshot` and is red on revert (before the fix, only the
first-declared cell, `puaamo`, parsed; `puabzo`/`kibamo`/`kibbzo` did not).
`pg-grammar/src/compile/tests.rs::circumfix_cross_product_embeds_each_halfs_context_in_lhs_not_environment_union`
pins the structural half (no allomorph carries an inner-edge environment) from the same crate's own
unit tests, since parsing there would need a dev-dependency cycle on `pg_parse`.

## Remaining work
None. `build_circumfix_allomorphs` now fans out over each half's resolved environment (mirroring
`GetAffixAllomorphEnvironments`'s cross product) and `build_circumfix_lhs` builds the Lhs as
`[prefix-right-context nodes] AnyStar [suffix-left-context nodes]` when either half is conditioned,
putting only the outer contexts into `environments` (HCLoader.cs:1276-1323).

## Upstream
None needed: HCLoader is the reference behaviour here; the gap is PanGloss-only.
