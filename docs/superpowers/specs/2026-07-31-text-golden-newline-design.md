# Text-golden newline robustness

## Goal

Eliminate false golden-test failures caused solely by CRLF versus LF checkout bytes without hiding real semantic, formatting, ordering, or canonical-output regressions.

## Artifact inventory and comparison contracts

Golden tests must select the narrowest comparison contract that matches the artifact. The repository currently has four external `include_str!` goldens:

| Artifact | Contract | Migration |
|---|---|---|
| `pg-cli/src/make_report_golden.md` | Rendered Markdown | Normalize newlines on expected and actual |
| `pg-foma/src/plan_diagram_golden.mmd` | Rendered Mermaid | Normalize newlines on expected and actual |
| `pg-foma/src/coverage_ledger_golden.json` | Canonical LF JSON text | Normalize only the checked-out expected fixture; actual output must already be canonical LF |
| `pg-foma/src/readiness_verdict_golden.json` | Canonical LF JSON text | Normalize only the checked-out expected fixture; actual output must already be canonical LF |

Use these contracts:

1. **Rendered text:** Markdown, Mermaid, and other human-readable text compare after converting CRLF and lone CR to LF on both sides. All other content, ordering, identifiers, whitespace, Unicode, and trailing-newline presence/count remain significant.
2. **Semantic JSON:** Parse expected and actual text and compare values only when formatting and object-key order are outside the artifact contract. Reject duplicate object keys on both sides before comparison. Arrays remain ordered; schema-specific set or multiset handling occurs outside the generic helper, preserving multiplicity where required. Under the current `serde_json` features, numeric representation remains significant (`1` and `1.0` compare unequal). There is no current external semantic-JSON golden migration; the helper is reusable policy with contract tests until a real call site needs it.
3. **Canonical LF text:** For reviewable canonical text whose exact layout and serialization remain part of the contract, normalize only the checked-out expected fixture to LF and compare the actual output unchanged. Thus expected CRLF versus actual canonical LF passes, while actual CRLF, whitespace/order drift, or trailing-newline drift fails.
4. **Raw byte-exact artifacts:** Signed or hashed payloads, JCS where raw bytes are themselves the contract, binary formats, and other opaque byte fixtures compare raw bytes with no normalization.

`.gitattributes` remains broad checkout hygiene and must cover golden text extensions, but it is not the only correctness defense because it does not retroactively repair an already-populated worktree.

## Implementation

Add three strongly named test-only assertion helpers:

- `assert_rendered_text_eq`;
- `assert_semantic_json_eq`;
- `assert_canonical_lf_text_eq`.

All helpers use `#[track_caller]`. Place small `#[cfg(test)]` modules inside `pg-foma` and `pg-cli`; do not add a production dependency or production API solely for test support. Share within a crate where practical.

`normalize_newlines` converts only CRLF and remaining lone CR to LF. It must preserve trailing newline presence and count, spaces, tabs, BOM, NUL, Unicode normalization form, U+0085, U+2028, U+2029, and all non-ASCII text. Escaped `"\r\n"` inside parsed JSON is semantic string data and remains different from `"\n"`.

Diagnostics are part of the helper contract:

- rendered/canonical text mismatch reports normalized line and column plus escaped context, including EOF and trailing-newline differences;
- JSON parse failure identifies expected versus actual and retains the parser line and column;
- JSON value mismatch presents stable pretty-printed parsed values;
- duplicate JSON object keys fail distinctly rather than being silently collapsed.

Audit every external `include_str!` golden assertion and migrate it according to the table. Inline serialization tests that do not depend on checkout bytes stay exact. Other JSON tests that already parse JSON retain their existing semantic assertions.

## Verification

Helper contract tests must prove:

- LF expected versus CRLF actual passes for rendered text;
- lone CR is normalized consistently;
- non-newline changes and trailing-newline changes still fail;
- Unicode, BOM, NUL, spaces, tabs, and non-ASCII text are preserved without normalization beyond CR/LF;
- U+0085, U+2028, and U+2029 remain untouched;
- semantically equivalent JSON with different insignificant whitespace, object-key order, and source line endings passes;
- escaped newline differences inside JSON strings remain significant;
- duplicate object keys are rejected on either side;
- `1` and `1.0` remain distinct;
- array reordering fails by default and duplicate multiplicity is preserved;
- malformed JSON identifies the failing side and source position;
- expected CRLF versus actual LF passes for canonical LF text;
- expected LF versus actual CRLF fails for canonical LF text;
- raw binary/opaque comparisons remain byte-sensitive.

Run the four migrated golden tests, all helper contract tests, affected crate suites, `git check-attr` and `git ls-files --eol` checks, then the full managed workspace suite through `rust/tools/pg.ps1`.

## Scope and safety

This change affects tests and test-support code only. It does not rewrite production output, normalize user files, modify serialization formats, or weaken comparisons beyond the explicit artifact contract. Content drift exposed after newline handling—such as changed diagram node identifiers—remains a real test failure to diagnose separately.
