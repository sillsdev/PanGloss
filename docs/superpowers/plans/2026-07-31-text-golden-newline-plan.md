# Text-golden newline robustness implementation plan

Design authority: `docs/superpowers/specs/2026-07-31-text-golden-newline-design.md`.

## Task 1: Add pg-foma test-only comparison contracts

Files:

- create `rust/crates/pg-foma/src/test_support.rs`;
- modify `rust/crates/pg-foma/src/lib.rs` only to declare it under `#[cfg(test)]`.

Work test-first:

1. Add focused tests for `normalize_newlines`, `assert_rendered_text_eq`, and `assert_canonical_lf_text_eq`. Prove CRLF and lone-CR behavior, actual-side canonical-LF enforcement, trailing-newline sensitivity, and preservation of spaces, tabs, BOM, NUL, non-ASCII text, Unicode normalization form, U+0085, U+2028, and U+2029. Add `catch_unwind` tests for 1-based line/column, escaped context, and explicit EOF diagnostics before implementing them.
2. Run the focused test and record the expected compile failure before adding helpers.
3. Implement the smallest test-only helpers as `pub(crate)` (or narrower `pub(super)` where sufficient). Use `Cow<'_, str>`, `#[track_caller]`, and a deterministic first-mismatch diagnostic with normalized 1-based line/column, escaped context, and explicit EOF.
4. Add semantic-JSON contract tests before its implementation: key-order/whitespace and source CRLF/LF equivalence, ordered arrays, duplicate multiplicity, `1` versus `1.0`, escaped newline significance, and recursive nested duplicate-key rejection on both expected and actual. Before implementation, add `catch_unwind` tests for expected-versus-actual parse labels with parser position, stable pretty-printed value mismatch, and a distinct duplicate-key message.
5. Implement recursive duplicate-rejecting deserialization with a custom `serde::de::Visitor` that constructs `serde_json::Value`; never allow ordinary Value deserialization to collapse duplicate object keys silently. Keep schema-specific set/multiset handling outside this helper.
6. Run focused helper tests through `rust/tools/pg.ps1`.

## Task 2: Migrate the three pg-foma external goldens

Files:

- `rust/crates/pg-foma/src/plan_diagram.rs`;
- `rust/crates/pg-foma/src/coverage_ledger.rs`;
- `rust/crates/pg-foma/src/readiness_verdict.rs`.

Work test-first:

1. Extract the same small assertion-boundary functions used by each real golden. Demonstrate the pre-migration raw boundary fails on a CRLF-materialized fixture, then route both the regression and real golden through the migrated boundary.
2. Migrate Mermaid to `assert_rendered_text_eq`.
3. Migrate the two canonical JSON fixtures with the directional API `assert_canonical_lf_text_eq(actual, expected)`; call as `assert_canonical_lf_text_eq(&json, GOLDEN_JSON)`. Expected is normalized; actual is untouched.
4. Prove an actual CRLF canonical serialization still fails and formatting/order/trailing-newline drift still fails.
5. Run all three golden gates and helper contract tests through the managed wrapper. Do not regenerate any golden.

## Task 3: Add the pg-cli rendered-text helper and migrate Markdown

Files:

- create `rust/crates/pg-cli/src/test_support.rs`;
- modify `rust/crates/pg-cli/src/main.rs` to declare it under `#[cfg(test)]`;
- modify `rust/crates/pg-cli/src/make_report.rs`.

Work test-first:

1. Extract the same small assertion-boundary function used by the real Markdown golden. Demonstrate its pre-migration raw comparison fails on CRLF-materialized text, then add a CLI-local rendered-text helper contract test and observe the expected compile failure.
2. Implement only the rendered-text subset required by pg-cli as `pub(crate)` (or `pub(super)` where sufficient) inside the `#[cfg(test)]` module, matching pg-foma behavior and diagnostics; do not expose production APIs or add a cross-crate production dependency.
3. Migrate `make_report_golden.md` and add a CRLF-materialized expected/actual regression test.
4. Prove content, identifiers, whitespace, Unicode, and trailing newline remain significant.
5. Run the Markdown golden and CLI helper tests through the managed wrapper.

## Task 4: Checkout hygiene and verification

Files:

- modify `.gitattributes` only if the four golden naming/extensions are not already covered.

Checks:

1. Verify the existing exact golden-path attributes; edit `.gitattributes` only if one is uncovered. Do not impose blanket LF policy on every repository Markdown or JSON file.
2. Run `git check-attr text eol -- <four golden paths>`.
3. Run `git ls-files --eol -- <four golden paths>` and record index/worktree EOL state.
4. Run formatting and `git diff --check`.
5. Run affected pg-foma and pg-cli suites, then the full managed workspace suite.
6. Review all call sites against the four-row artifact inventory. Confirm no raw signed, hashed, JCS, or binary assertion was normalized, and name an existing raw-byte/signature gate exercised by the full suite (or record the unchanged raw call-site diff) as authoritative byte-sensitivity evidence.

## Integration discipline

- Implement in an isolated worktree based on integration commit `612be5b`.
- Keep production output and serialization code unchanged.
- Keep semantic JSON as tested reusable policy; do not invent a migration call site.
- Commit helper/migrations as one focused change after red/green evidence.
- The primary agent reviews the full diff and reruns representative gates before integration.
