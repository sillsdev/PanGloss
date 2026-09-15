# Gate-only Template Analysis Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Preserve Machine's template-state invariant in PanGloss without changing merge behavior.

**Architecture:** Template admission remains a compatibility check; slot analysis owns the returned
feature structures. A Machine-authored shared grammar supplies full-parse expectations, while a
direct state assertion supplies the load-bearing red/green witness.

**Tech Stack:** Machine C# conformance harness, Rust pg-rules/pg-parse, managed PowerShell test runner.

## Task 1: Machine grammar and independent expectations

Create `conformance/edge-cases/shared-template-unconstrained-suffix/{grammar.xml,words.yaml}`
and `conformance/docs/shared-template-unconstrained-suffix.md` in the isolated Machine worktree.
Use C# `SameRuleUsedInMultipleTemplates_AffixHasNoRequiredFeatures` as the semantic source.
The grammar has N root `mi`, ordinary N-to-IV `v`, and unconstrained template-only suffix `d`
shared by mandatory TV and IV templates. Include a TV root as the opposite-category positive
and an unrelated category as a negative. Do not presume the derived but uninflected `miv` is legal.

- [x] Derive complete expected signatures and rule identities by forward synthesis.
- [x] Verify the fixture against Machine's Release conformance harness at `a20bce12`, memo on/off,
  both template orders. Capture every row and count; zero skips.
- [x] Regenerate existing manifest/coverage artifacts with the harness's existing commands.
- [x] Review fixture semantics and complete file diff before committing on the conformance branch.

## Task 2: Red template-state regression

Modify `rust/crates/pg-rules/src/stratum.rs`'s private tests, or reuse the public analysis test seam
in `rust/crates/pg-rules/tests/stratum_gate.rs` if it exposes the actual template output. Do not
add a production API solely for testing. An all-optional template requires V; its suffix is absent
from the input. Compare the actual template result's syntactic FS with the empty input FS.
Assert one output, unchanged shape and unchanged features. Add a disjoint-input rejection control.

- [x] Build actual grammar/Word values and call production template analysis, not a copied fold.
- [x] Run `./rust/tools/pg.ps1 -Mode quick -Package pg-rules -Filter template_analysis_tests` and record the
  feature assertion failing on injected V, not an unrelated error or absent result.
- [x] Ensure an ordinary stratum seed cannot satisfy the test without executing the template.

## Task 3: Minimal implementation and preservation gates

In `StratumAnalyzer::analyze_template` in `rust/crates/pg-rules/src/stratum.rs`, retain the existing
`is_unifiable` rejection. Delete the local `unify(input, req)` value and the final `add` loop.
Return slot outputs directly:

```rust
out.into_values().collect()
```

Remove only imports made unused by that deletion; leave every `generalize_syn_fs` caller unchanged.
Correct the adjacent function documentation to describe gate-only admission.

- [x] Apply the minimal deletion only after observing the red state assertion.
- [x] Run `./rust/tools/pg.ps1 -Mode check -Package pg-rules`.
- [x] Re-run the state regression green; restore only the deleted accumulation temporarily and
  observe red again, then restore the fix and re-run green.
- [x] Mirror Machine's committed fixture unchanged under
  `conformance-staging/edge-cases/shared-template-unconstrained-suffix/`; add `STAGING.md` with
  the exact upstream commit and verification, not Rust-derived expected results.
- [x] Add `rust/crates/pg-parse/tests/template_analysis_conformance.rs`, loading the fixture via
  `pg-conformance-fixtures` and comparing with `assert_matches_oracle`. For both original and
  reversed template order, run `Morpher::new(&grammar, usize::MAX).with_memo(memo)` with memo
  false and true. Assert expected row count and fail if fixture missing.
- [x] Verify the focused grammar before/after the fix, plus managed pg-rules stratum/template,
  pg-parse Exact, existing C#-ported template tests and the conformance fixture gate.
- [x] Investigate any unexpected new failure; do not alter the oracle to hide it.

## Task 4: Review, ledger and publication

Update `docs/divergences/034-stratum-merge-equivalence.md` and
`035-template-slot-fs-collisions.md` with exact tests, commits and counts. Keep #505 open.

- [ ] Obtain independent spec-compliance review, then code-quality/correctness review.
- [x] Inspect every changed file and `git diff --check`; verify fixture mirrors by hashes.
- [x] Run `./rust/tools/pg.ps1 -Mode test -Package pg-cli -TestTarget divergence_catalogue_gate`.
- [ ] Commit scoped files, push Machine conformance branch and PanGloss main without force.
- [ ] Verify remote refs and preserve shared-worktree dirty files unchanged.
