**Status note:** section 1 (the shared gloss-signature contract) landed in Rust
(`pg-realize/src/signature.rs`) and is genuinely used by `add-grammar-diagnostics`. Everything else in
this change — the entire C# HermitCrab oracle harness (sections 2, 2A, 3, 4) and its verification
(section 5) — is **not started**: no `.cs` files or `gloss-batch`/`analysis-batch` implementation exist
anywhere in this repo outside the unrelated `machine` submodule's own library code, and no invocation
wrapper/orchestrator exists.

## 1. Shared signature contract

- [x] 1.1 Implement the tagged signature grammar with RFC 8785 canonical JSON strings:
      `g:<string>` for literal gloss, `m:<id-string>` for missing gloss, and `s:<string>` for shape;
      recognize separators only outside strings and perform no Unicode normalization
      (`pg-realize/src/signature.rs`: `gloss_signature_entry`, `gloss_analysis_set_signature`,
      `word_gloss_signature`)
- [x] 1.2 Add shared golden cases for duplicates, missing/literal collision, `+|;`, tab/CR/LF,
      quote/backslash, empty strings, non-ASCII/non-BMP text, multi-character segments, boundaries,
      zero analyses, skipped words, malformed escapes, and truncated strings
      (golden cases present in `signature.rs`'s own test module)
- [x] 1.3 Sort analysis entries by unsigned canonical UTF-8 bytes and verify Rust diagnostic gloss
      signatures conform to every shared case
      (`canonical_json_string`; `word_gloss_signature_combines_entry_building_and_assembly` and
      sibling tests)

## 2. C# `gloss-batch`

- [ ] 2.1 Add `gloss-batch` without colliding with the existing stats command (not done — no C# harness code exists in this repo)
- [ ] 2.2 Produce the canonical five-column TSV with per-word `ms` and duplicate-sensitive tagged
      canonical-JSON-string gloss-chain/shape signatures; no timing sidecar (not done)
- [ ] 2.3 Preserve STARTED/crash/status conventions and add focused C# tests (not done)

## 2A. C# `analysis-batch`

- [ ] 2A.1 Add a supported XML-loader result/callback mapping loaded morpheme object references to
      unique XML `id` keys; do not use reflection or enumeration order (not done)
- [ ] 2A.2 Add `analysis-batch` using `Morpher.AnalyzeWord`, canonical stable-key/root/category
      identities, sorted semantic-set output, separate duplicate counts, and typed outcomes (not done)
- [ ] 2A.3 Test empty/duplicate `<MorphemeId>`, distinct XML keys, root/category differences,
      duplicate discovery, zero analyses, skip/crash, and mapping collision/not-comparable behavior (not done)

## 3. Invocation and format scope

- [ ] 3.1 Extend the wrapper/orchestrator to create temporary `analysis-batch` and optional
      `gloss-batch` scripts and invoke `dotnet hc.dll -i <grammar.xml> -s <script>` (not done — no wrapper/orchestrator exists)
- [ ] 3.2 Accept `--full`/`-Full` only for `.xml`; fail clearly and before C# startup for `.json` or `.fwdata` (not done)
- [ ] 3.3 Report first-build/load time separately from per-word timing and reuse the built tool for subsequent words/runs (not done)
- [ ] 3.4 Fail clearly when dotnet, hc.dll, output TSV, or a well-formed result row is missing (not done)

## 4. Multiset comparison and reporting

- [ ] 4.1 Parse structured identities as deduplicated semantic sets and gloss signatures as
      duplicate-count-sensitive diagnostics keyed by word; ignore idx/ms for correctness (not done)
- [ ] 4.2 Report missing/extra rows, status differences, gloss-chain differences, shape differences, and duplicate-count differences separately (not done)
- [ ] 4.3 Store `reference_hermitcrab_gloss_multiset_parity` as diagnostic evidence, not corpus-recall certification (not done)
- [ ] 4.4 Extend the native CLI/PowerShell cross-engine validation batch to run caller-supplied words
      against combined Rust, Rust HermitCrab-only, and optional C# HermitCrab; name each pipeline (not done)
- [ ] 4.5 Keep C# validation code and invocation entirely outside WASM dependencies and exports
      (vacuously true today — no C# validation code exists at all, so nothing has leaked into WASM;
      not counted as done since there is nothing yet to keep out)
- [ ] 4.6 Return evidence-rich match, mismatch, incomplete, and `not_run` states without implementing
      any publication allow/deny policy (not done)
- [ ] 4.7 Support before/after HC XML contexts with different fingerprints and align word-level
      structured deltas without refusing the run because context differs (not done)
- [ ] 4.8 Add explicit `--rerun-deltas-with-tracing`: trace every unique participating side for
      grammar and engine deltas, deduplicate runs, and retain analysis equality as authoritative (not done)
- [ ] 4.9 Bound trace nodes/serialized bytes independently of semantic analysis; emit trace
      completeness, truncation, effective limits, and non-authoritative structural differences (not done)
- [ ] 4.10 Emit machine-readable FieldWorks investigation handoffs in assessment-delta or
      trace-diagnostic reports with stable associated IDs,
      suggested filters, and trace references; add no FieldWorks invocation or UI dependency (not done)

## Verification

- [ ] 5.1 Exercise the C# tool through the real `-i/-s` wrapper, not a direct invented positional interface (not_run — no C# tool exists)
- [ ] 5.2 Verify XML success and explicit JSON/fwdata rejection (not_run)
- [ ] 5.2a Verify C# structured output matches Machine `WordAnalysis.Equals` dimensions while using
      collision-free XML source keys instead of optional `<MorphemeId>` labels (not_run)
- [ ] 5.3 Run strict OpenSpec validation when available
      (see this bookkeeping pass's own final `openspec validate --all --strict` run)
