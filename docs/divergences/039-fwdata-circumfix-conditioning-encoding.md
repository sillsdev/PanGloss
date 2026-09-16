# 039 — fwdata circumfix conditioning is encoded as an environment union, not as HCLoader's stem pattern

## Kind
Behavioural (loader-side: the grammar PanGloss compiles from `.fwdata` differs from the grammar
HCLoader builds from the same project, so the same word can parse differently).

## Status
Open. Found 2026-09-16 while converting `conformance-staging/edge-cases/circumfix-conditioned-halves`
to the FieldWorks-producible shape; not yet fixed in Rust.

## C# site
`HCLoader.LoadCircumfixAffixProcessAllomorph` (FieldWorks `Src/LexText/ParserCore/HCLoader.cs:1273-1332`).
For each (prefix half, suffix half, prefix env, suffix env) combination it splits each half's
`PhEnvironment` at `_` and embeds the prefix half's RIGHT context and the suffix half's LEFT context
as literal mandatory nodes inside the single `stem` Lhs pattern (`PrefixNull`, `LoadPatternNodes`,
`AnyStar`, `LoadPatternNodes`, `SuffixNull`; :1286-1300). Only the OUTER contexts become one
`AllomorphEnvironment` (:1313-1322); `Environments.Add` is called at most once.

## Rust site
`pg_grammar::compile::affixes::build_circumfix_allomorphs` (`rust/crates/pg-grammar/src/compile/affixes.rs`,
the `environments` union at the end of the pairing loop): `lhs = [any_plus]` and BOTH halves'
environments (plus `positions`) pushed onto the one `AffixAllomorphDef.environments` list. The
environment-union reasoning is recorded in `docs/research/circumfix-cross-product-loading.md`
("Corrected: a conditioned half IS representable"), now marked superseded.

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
each half conditioned on the adjacent stem edge): the union encoding parses one of the four cells
and rejects `puabzo`/`kibamo`/`kibbzo`; the HCLoader encoding parses all four. A FieldWorks project
with a conditioned circumfix whose halves are reused across pairings therefore parses more words in
FieldWorks than in PanGloss-from-fwdata today. Whether any real project has such an entry is not yet
verified (`circumfix-cross-product-loading.md` records the same open question for its source data).

## Evidence
`conformance-staging/edge-cases/circumfix-conditioned-halves/` now pins the HCLoader shape
(oracle-verified 2026-09-16, all four cells parse). The pre-conversion grammar, with the union
encoding and the three rejections, is in git history at the commit before that conversion. The
fwdata compile path itself is reached only by `pg_grammar::compile_project`, so the fix needs its own
red-on-revert test in `pg-grammar/src/compile/tests.rs`, not a conformance fixture.

## Remaining work
Port HCLoader's split: build the Lhs as `[prefix-right-context nodes] AnyStar [suffix-left-context
nodes]` when either half is conditioned, and put only the outer contexts into `environments`. Then
compile a conditioned 2x2 circumfix project through `compile_project` and assert all four cells
parse, red on revert.

## Upstream
None needed: HCLoader is the reference behaviour here; the gap is PanGloss-only.
