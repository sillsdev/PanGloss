## 1. Shared signature contract

- [ ] 1.1 Implement the tagged signature grammar with RFC 8785 canonical JSON strings:
      `g:<string>` for literal gloss, `m:<id-string>` for missing gloss, and `s:<string>` for shape;
      recognize separators only outside strings and perform no Unicode normalization
- [ ] 1.2 Add shared golden cases for duplicates, missing/literal collision, `+|;`, tab/CR/LF,
      quote/backslash, empty strings, non-ASCII/non-BMP text, multi-character segments, boundaries,
      zero analyses, skipped words, malformed escapes, and truncated strings
- [ ] 1.3 Sort analysis entries by unsigned canonical UTF-8 bytes and verify Rust diagnostic gloss
      signatures conform to every shared case

## 2. C# `gloss-batch`

- [ ] 2.1 Add `gloss-batch` without colliding with the existing stats command
- [ ] 2.2 Produce the canonical five-column TSV with per-word `ms` and duplicate-sensitive tagged
      canonical-JSON-string gloss-chain/shape signatures; no timing sidecar
- [ ] 2.3 Preserve STARTED/crash/status conventions and add focused C# tests

## 2A. C# `analysis-batch`

- [ ] 2A.1 Add a supported XML-loader result/callback mapping loaded morpheme object references to
      unique XML `id` keys; do not use reflection or enumeration order
- [ ] 2A.2 Add `analysis-batch` using `Morpher.AnalyzeWord`, canonical stable-key/root/category
      identities, sorted semantic-set output, separate duplicate counts, and typed outcomes
- [ ] 2A.3 Test empty/duplicate `<MorphemeId>`, distinct XML keys, root/category differences,
      duplicate discovery, zero analyses, skip/crash, and mapping collision/not-comparable behavior

## 3. Invocation and format scope

- [ ] 3.1 Extend the wrapper/orchestrator to create temporary `analysis-batch` and optional
      `gloss-batch` scripts and invoke `dotnet hc.dll -i <grammar.xml> -s <script>`
- [ ] 3.2 Accept `--full`/`-Full` only for `.xml`; fail clearly and before C# startup for `.json` or `.fwdata`
- [ ] 3.3 Report first-build/load time separately from per-word timing and reuse the built tool for subsequent words/runs
- [ ] 3.4 Fail clearly when dotnet, hc.dll, output TSV, or a well-formed result row is missing

## 4. Multiset comparison and reporting

- [ ] 4.1 Parse structured identities as deduplicated semantic sets and gloss signatures as
      duplicate-count-sensitive diagnostics keyed by word; ignore idx/ms for correctness
- [ ] 4.2 Report missing/extra rows, status differences, gloss-chain differences, shape differences, and duplicate-count differences separately
- [ ] 4.3 Store `reference_hermitcrab_gloss_multiset_parity` as diagnostic evidence, not corpus-recall certification
- [ ] 4.4 Extend the native CLI/PowerShell cross-engine validation batch to run caller-supplied words
      against combined Rust, Rust HermitCrab-only, and optional C# HermitCrab; name each pipeline
- [ ] 4.5 Keep C# validation code and invocation entirely outside WASM dependencies and exports
- [ ] 4.6 Return evidence-rich match, mismatch, incomplete, and `not_run` states without implementing
      any publication allow/deny policy
- [ ] 4.7 Support before/after HC XML contexts with different fingerprints and align word-level
      structured deltas without refusing the run because context differs
- [ ] 4.8 Add explicit `--rerun-deltas-with-tracing`: trace every unique participating side for
      grammar and engine deltas, deduplicate runs, and retain analysis equality as authoritative
- [ ] 4.9 Bound trace nodes/serialized bytes independently of semantic analysis; emit trace
      completeness, truncation, effective limits, and non-authoritative structural differences
- [ ] 4.10 Emit machine-readable FieldWorks investigation handoffs in assessment-delta or
      trace-diagnostic reports with stable associated IDs,
      suggested filters, and trace references; add no FieldWorks invocation or UI dependency

## Verification

- [ ] 5.1 Exercise the C# tool through the real `-i/-s` wrapper, not a direct invented positional interface
- [ ] 5.2 Verify XML success and explicit JSON/fwdata rejection
- [ ] 5.2a Verify C# structured output matches Machine `WordAnalysis.Equals` dimensions while using
      collision-free XML source keys instead of optional `<MorphemeId>` labels
- [ ] 5.3 Run strict OpenSpec validation when available
