# STAGING: circumfix-conditioned-halves

## Why this fixture exists

`circumfix-cross-product-and-infix-drop`'s own STAGING.md ("Deviations from the original sketch")
records that per-side phonological conditioning on a circumfix's two halves was tried and abandoned
during that fixture's authoring, reasoning that `pg-rules/src/validity.rs`'s per-run environment
check "cannot" scope an environment to only one of a circumfix's two pieces. That reasoning is
backwards: `environments_ok` is `.any()` over an allomorph's declared environments, evaluated
independently at each contiguous `MorphRecord` run (W3.3), so a combined allomorph carrying the
UNION of both halves' environments naturally partitions itself across the two runs a circumfix
produces -- at the prefix run only the prefix half's own environment can ever hold, and vice versa.
This fixture proves that mechanism directly, and pins the same corrected reading that motivated
removing `pg-grammar/src/compile/affixes.rs::build_circumfix_allomorphs`'s environment-carrying-half
refusal (see `docs/research/circumfix-cross-product-loading.md`).

**Important caveat on what this fixture does and does not cover.** This fixture is authored as raw
HC-XML `MorphologicalRule`/`MorphologicalSubrule` elements (each subrule directly declaring its own
leading+trailing `InsertSegments` and `RequiredEnvironments`), loaded via `pg_grammar::load`'s native
HC-XML path -- the same loading path every `conformance-staging`/`machine/conformance` fixture uses.
`build_circumfix_allomorphs` is reachable ONLY from `pg_grammar::compile_project`'s
`Snapshot`/`.fwdata`-import path, which this fixture's loader never calls. So this fixture pins the
downstream mechanism (`pg-rules/src/validity.rs`'s per-run `environments_ok`) that the loader fix
relies on, but it does NOT exercise `build_circumfix_allomorphs` itself. The regression pin for that
specific function is `rust/crates/pg-grammar/src/compile/tests.rs`'s
`a_circumfix_half_carrying_an_environment_builds_with_it_unioned_in` and
`a_circumfix_with_environments_on_both_halves_unions_them`, which build a synthetic `Snapshot`
directly and call `compile_project`.

## What it pins

- `aa` (bare root): a plain control.
- `puaamo`: `mrCirc`'s `subVV` (prefix `pu-` /_[V], suffix `-mo` /[V]_) applied to a V-initial,
  V-final stem -- both of the combined allomorph's environments hold, each at its own run. `subVV`
  is declared FIRST among `mrCirc`'s four subrules, so it has no earlier "passed-over" disjunctive
  sibling to conflict with (see the "genuinely combinatorial, but only the first cell" finding
  below).
- `puabzo`/`kibamo`/`kibbzo`: the other three cross-product cells (`subVC`/`subCV`/`subCC`).
  **Oracle-confirmed** (both C# `hc-conformance.exe` and `pg_parse::Morpher`) that these do NOT
  parse, even though each cell's own two environments individually hold for its stem. This is not
  the bug this fixture was written to close -- it is a SEPARATE, pre-existing, C#-faithful mechanism
  (the disjunctive-allomorph re-check) that also applies here. See "Oracle discipline" below and
  `docs/research/circumfix-cross-product-loading.md`'s "A separate, oracle-confirmed limitation"
  section for the full mechanism.
- `pubbmo`/`puabmo`: over-generation controls proving `subVV`'s own union-of-environments still
  correctly rejects a stem that violates one or both sides (the per-run AND across runs, not "one
  side is enough").
- `aamo`: a structurally-invalid control (missing the required prefix material entirely).

## Grammar shape

One `MorphologicalRule` (`mrCirc`) with four `MorphologicalSubrule`s, each a leading+trailing
`InsertSegments` around one `CopyFromInput` (mirroring the shape `build_circumfix_allomorphs`
constructs from a FieldWorks-imported circumfix's prefix/suffix halves), and each declaring TWO
`RequiredEnvironments` entries directly on the subrule -- one for the prefix side (`RightEnvironment`,
what the stem must start with), one for the suffix side (`LeftEnvironment`, what the stem must end
with). This is the literal-HC-XML equivalent of `build_circumfix_allomorphs`'s post-fix
`AffixAllomorphDef.environments` union. Four root entries (`aa`/`ab`/`ba`/`bb`) cover the four
initial/final V-or-C combinations, per the task's motivating shape (a prefix pair and a suffix pair
each conditioned on the adjacent stem edge -- structurally the same as Aweti's real circumfix, per
`docs/research/circumfix-cross-product-loading.md`, but with fresh synthetic segments/forms per this
repo's synthetic-data rule).

## Oracle discipline

**Oracle: the C# founding oracle (`hc-conformance.exe`), self-check mode.** Command run:
```
rust\tools\oracle-conformance.ps1
```
This runs `hc-conformance.exe --fixtures conformance-staging` (self-check: it computes its own
result for every fixture's words and diffs against the committed `words.yaml`). First pass (with
this fixture's words.yaml guessing that all four cross-product cells parse) reported:
```
[FAIL] edge-cases/circumfix-conditioned-halves (14ms) 3/8 word(s) mismatched
    word 'puabzo': expected [CIRC+ROOTAB|puabzo] got []
    word 'kibamo': expected [CIRC+ROOTBA|kibamo] got []
    word 'kibbzo': expected [CIRC+ROOTBB|kibbzo] got []
```
i.e. the C# oracle ITSELF produces no analysis for those three words -- confirming the disjunctive-
allomorph rejection is genuine HermitCrab behavior, not a Rust divergence. `words.yaml` was corrected
to `expect_fail: true` for those three words to match, and the self-check now reports:
```
[PASS] edge-cases/circumfix-conditioned-halves (13ms)
```
with zero divergence for this fixture (`rust/tools/oracle-conformance.ps1` exits with the fixture
absent from the new-divergence list). `machine` checkout commit `9358825cdeb7585756f4ada0fc28bce
1574ab364` (the same commit `circumfix-cross-product-and-infix-drop`'s own oracle-provenance marker
records). This fixture's `words.yaml` carries `# oracle-provenance: founding-oracle`.

## Verification

Signatures for the currently-parsing words (`aa`, `puaamo`) were transcribed via a throwaway
`pg-parse` integration test driving `pg_parse::Morpher::parse_word` directly (the conformance-
grammars skill's own step-3 shortcut), deleted once transcription and trace debugging were done. The
`expect_fail` words were confirmed both by that same run (Rust: signature `"-"`) and by the C# oracle
self-check above.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/circumfix-conditioned-halves/`. On acceptance, delete this staged
copy in the same change (graduation guard enforces this mechanically).
