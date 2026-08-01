# Text-golden newline robustness

## Goal

Eliminate false golden-test failures caused solely by CRLF versus LF checkout bytes without hiding real semantic or ordering regressions.

## Comparison contracts

Golden tests must select the narrowest comparison contract that matches the artifact:

1. **Semantic JSON:** Parse expected and actual text as `serde_json::Value` and compare values. Object key order and insignificant whitespace are ignored. Arrays remain ordered unless the artifact schema explicitly defines a particular array as a set; only those arrays may be canonicalized before comparison.
2. **Rendered text:** Markdown, Mermaid, and other human-readable text compare after converting CRLF and lone CR to LF on both sides. All other bytes, text, ordering, identifiers, and whitespace remain significant.
3. **Byte-exact artifacts:** Canonical JSON/JCS, signed or hashed payloads, binary formats, and tests whose stated contract is byte identity continue to compare raw bytes. These tests must not use newline normalization.

`.gitattributes` remains as checkout hygiene but is not correctness-critical.

## Implementation

Add a small test-support module with:

- `normalize_newlines(&str) -> Cow<'_, str>`;
- an assertion helper for normalized rendered text with a useful diff;
- an assertion helper for semantic JSON that reports parse failures distinctly from value mismatches.

Place the helper at the lowest shared test-support layer already consumed by the affected crates. If no such cross-crate layer exists, use minimal crate-local wrappers rather than adding a production dependency solely for tests.

Audit current `include_str!` and file-backed golden assertions. Migrate only tests whose contracts are rendered text or semantic JSON. Record byte-exact exceptions in the test next to the raw comparison.

## Verification

Tests must prove:

- LF expected versus CRLF actual passes for rendered text;
- lone CR is normalized consistently;
- a non-newline text change still fails;
- semantically equivalent JSON with different whitespace, key order, and line endings passes;
- array reordering still fails by default;
- malformed JSON fails distinctly;
- representative Markdown, Mermaid, and JSON goldens pass from a CRLF-materialized temporary copy;
- canonical/byte-exact tests retain raw-byte behavior.

Run the affected crate suites through `rust/tools/pg.ps1`, then the full managed workspace suite.

## Scope and safety

This change affects tests and test-support code only. It does not rewrite production output, normalize user files, modify serialization formats, or weaken comparisons beyond the explicit artifact contract. Any content drift exposed after newline normalization—such as changed diagram node identifiers—remains a real test failure to diagnose separately.
