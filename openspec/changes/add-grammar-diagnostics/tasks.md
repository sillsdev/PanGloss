## 1. Rust command and schema

- [ ] 1.1 Make `load_grammar` `pub(crate)` and add `pangloss diagnose <grammar> <words> <out-dir>` using the existing extension dispatch; allow in-memory compile/assessment with optional package output
- [ ] 1.2 Define separate immutable `build.json` and `assessment.json` report types consuming the coverage contract's evidence/completeness types and the safety change's resource outcomes
- [ ] 1.3 Record grammar/network fingerprint, pipeline and resource-policy versions, scanned/oracle-producing words, analyses, predeclared exclusions, timeouts, and partial-result status
- [ ] 1.4 Add golden schema and invalid-input tests

## 2. Supervised dual-engine timing

- [ ] 2.0 Integrate the completed single-worker watchdog from `harden-foma-resource-safety` before
      executing potentially adversarial real grammars; continue schema/CLI/self-contained work independently
- [ ] 2.1 Run default and production foma pipelines only through the shared supervisor/watchdog; diagnostics SHALL NOT invent an independent Aweti cap
- [ ] 2.2 Record load, traversal, decode/dedup, confirm-group build, restricted HC parse, route/match, total confirm, and separately labeled oracle time; consume compile events from `profile-fst-compilation` without instrumenting `emit.rs`
- [ ] 2.3 Compute completed-observation p50/p95/p99/max/mean while preserving timeout/partial-result counts as correctness-incomplete
- [ ] 2.4 Prove sink-off structural and exact-result equivalence with named gates
- [ ] 2.5 Run caller-supplied word sets through combined and Rust-HermitCrab-only pipelines and
      compare completed structured analysis collections independent of order or serialization
- [ ] 2.6 Reuse the coverage contract's structured-analysis identity exactly; retain internal traces
      only as mismatch and duplicate-provenance diagnostics
- [ ] 2.7 Add explicit strict-parity and grammar-delta interpretations: always run requested contexts,
      record their metadata, and emit per-word added/removed/unchanged/incomplete/not-attempted sets
- [ ] 2.8 Accept optional caller-supplied golden identity sets and emit exact matching/missing/
      unexpected diffs without a linguistic-quality or aggregate-closeness score
- [ ] 2.9 Optionally write a context-bound proposed golden to a distinct output path with an exact
      diff; prove validation never mutates, reformats, or replaces an input golden
- [ ] 2.10 Attach available stable rule/construct/stage/proposal/confirmation breadcrumbs to semantic
      deltas and duplicates, preserve completeness, and prohibit unsupported causal wording
- [ ] 2.11 Implement build-report comparison and assessment-report comparison as separate operations;
      semantic deltas accept canonical assessment reports only and never compile hidden baselines

## 3. Rust gloss and debug artifacts

- [ ] 3.1 Thread structured analyses through all four Rust batch result sites and derive gloss bundles with `pg_realize::gloss_bundle`
- [ ] 3.2 Emit stable `glosses.tsv` entries using the shared tagged RFC 8785 canonical-JSON-string
      grammar; preserve duplicate multiplicity and encode missing gloss as `m:<id-json-string>`
- [ ] 3.3 Add opt-in `debug.jsonl` for proposed/decoded/unique/confirmed counts and named-stage timing; label Complete or Truncated
- [ ] 3.4 Test zero, single, duplicate, missing-gloss, multi-character-shape, skipped, timed-out, and partial-result cases
- [ ] 3.5 Emit per-word pre-dedup duplicate counts, ratios, and available rule/proposer provenance;
      keep the duplicate-sensitive artifact separate from deduplicated semantic parity

## 4. PowerShell, rendering, CI, and skill

- [ ] 4.1 Add `scripts/diagnose.ps1` with `<lang>`, `-All`, and `-Project`; the Rust CLI remains single-grammar
- [ ] 4.2 Establish `incoming/<lang>/{grammar.*,words.txt}` with explicit gitignore negations for committed README/fixture files
- [ ] 4.3 Render build/assessment Markdown from their respective JSON artifacts, clearly separating compiler health from word-test evidence
- [ ] 4.4 Add a tiny committed synthetic CI smoke fixture and validate the report schema
- [ ] 4.5 Add `.claude/skills/grammar-diagnostic/SKILL.md`, accurately handing slow-confirm investigation to `dead-end-census` rather than claiming three counters reproduce its d1–d6 census

## 5. Verification

- [ ] 5.1 Produce a single-grammar artifact suitable for later consumption by `certify-four-language-matrix`; do not run or certify the four-language matrix here
- [ ] 5.2 Verify the authoritative Indonesian, Amharic, and pinned Aweti structural/result gates
- [ ] 5.3 Run strict OpenSpec validation when the CLI is available
