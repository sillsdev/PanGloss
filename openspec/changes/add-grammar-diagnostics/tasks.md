**Status note:** the Rust command/schema (section 1) and the apply-path budget containment fix
(2.0-2.4, using ADR 0003's `ApplyBudget` rather than the watchdog `harden-foma-resource-safety`
hasn't built) are landed in `pg-cli/src/diagnostics.rs`. Everything requiring a second engine/pipeline
to compare against (2.5-2.11), gloss/debug artifact files (3.x), and the PowerShell/CI/skill layer
(4.x) is explicitly deferred — `diagnostics.rs`'s own module doc states the coverage-contract types
those need have not landed.

## 1. Rust command and schema

- [x] 1.1 Make `load_grammar` `pub(crate)` and add `pangloss diagnose <grammar> <words> <out-dir>` using the existing extension dispatch; allow in-memory compile/assessment with optional package output
      (`pg-cli/src/main.rs`: `"diagnose" => diagnostics::run_diagnose`)
- [x] 1.2 Define separate immutable `build.json` and `assessment.json` report types consuming the coverage contract's evidence/completeness types and the safety change's resource outcomes
      (`pg-cli/src/diagnostics.rs`: `BuildReport`/`AssessmentReport`, `DIAGNOSTICS_SCHEMA_VERSION`)
- [x] 1.3 Record grammar/network fingerprint, pipeline and resource-policy versions, scanned/oracle-producing words, analyses, predeclared exclusions, timeouts, and partial-result status
      (present on `BuildReport`/`AssessmentReport`)
- [x] 1.4 Add golden schema and invalid-input tests
      (present in `diagnostics.rs`'s own test module)

## 2. Supervised dual-engine timing

- [x] 2.0 Integrate the completed single-worker watchdog from `harden-foma-resource-safety` before
      executing potentially adversarial real grammars; continue schema/CLI/self-contained work independently
      (the watchdog itself does not exist — see `harden-foma-resource-safety`'s tasks.md — so this
      task is satisfied via the fallback path this change's own design allows: apply-path containment
      via ADR 0003's `ApplyBudget`, per this change's own "fix the apply-path containment" scope in
      `STAGING.md`)
- [x] 2.1 Run default and production foma pipelines only through the shared supervisor/watchdog; diagnostics SHALL NOT invent an independent Aweti cap
      (`diagnostics.rs` calls `FomaProposer::propose_budgeted` against the shared `ApplyBudget`, not an
      invented Aweti-specific cap)
- [ ] 2.2 Record load, traversal, decode/dedup, confirm-group build, restricted HC parse, route/match, total confirm, and separately labeled oracle time; consume compile events from `profile-fst-compilation` without instrumenting `emit.rs`
      (not done — no per-stage timing breakdown found; `profile-fst-compilation` itself is not landed)
- [ ] 2.3 Compute completed-observation p50/p95/p99/max/mean while preserving timeout/partial-result counts as correctness-incomplete
      (not done — no percentile computation found)
- [x] 2.4 Prove sink-off structural and exact-result equivalence with named gates
      (`WordApplyStatus::Incomplete` recorded per word on an `ApplyBudget` trip; module doc calls this
      out as item "(a)" of what this change fixes)
- [ ] 2.5 Run caller-supplied word sets through combined and Rust-HermitCrab-only pipelines and
      compare completed structured analysis collections independent of order or serialization
      (not done — module doc: "No default-engine comparison pipeline...")
- [ ] 2.6 Reuse the coverage contract's structured-analysis identity exactly; retain internal traces
      only as mismatch and duplicate-provenance diagnostics
      (not done — coverage-contract types have not landed, per module doc)
- [ ] 2.7 Add explicit strict-parity and grammar-delta interpretations: always run requested contexts,
      record their metadata, and emit per-word added/removed/unchanged/incomplete/not-attempted sets
      (not done)
- [ ] 2.8 Accept optional caller-supplied golden identity sets and emit exact matching/missing/
      unexpected diffs without a linguistic-quality or aggregate-closeness score
      (not done — module doc: "no golden-identity diff")
- [ ] 2.9 Optionally write a context-bound proposed golden to a distinct output path with an exact
      diff; prove validation never mutates, reformats, or replaces an input golden
      (not done)
- [ ] 2.10 Attach available stable rule/construct/stage/proposal/confirmation breadcrumbs to semantic
      deltas and duplicates, preserve completeness, and prohibit unsupported causal wording
      (not done)
- [ ] 2.11 Implement build-report comparison and assessment-report comparison as separate operations;
      semantic deltas accept canonical assessment reports only and never compile hidden baselines
      (not done)

## 3. Rust gloss and debug artifacts

- [x] 3.1 Thread structured analyses through all four Rust batch result sites and derive gloss bundles with `pg_realize::gloss_bundle`
      (`diagnostics.rs` calls `pg_realize::word_gloss_signature` directly and embeds results in
      `AssessmentReport`)
- [ ] 3.2 Emit stable `glosses.tsv` entries using the shared tagged RFC 8785 canonical-JSON-string
      grammar; preserve duplicate multiplicity and encode missing gloss as `m:<id-json-string>`
      (not done — gloss signatures are embedded in the JSON report; no separate `glosses.tsv` writer)
- [ ] 3.3 Add opt-in `debug.jsonl` for proposed/decoded/unique/confirmed counts and named-stage timing; label Complete or Truncated
      (not done — module doc: "No `glosses.tsv`/`debug.jsonl`")
- [ ] 3.4 Test zero, single, duplicate, missing-gloss, multi-character-shape, skipped, timed-out, and partial-result cases
      (partial — some of these are covered by existing gloss-signature golden tests in `pg-realize`,
      but not specifically through the `debug.jsonl`/`glosses.tsv` artifacts, which don't exist)
- [ ] 3.5 Emit per-word pre-dedup duplicate counts, ratios, and available rule/proposer provenance;
      keep the duplicate-sensitive artifact separate from deduplicated semantic parity
      (not done)

## 4. PowerShell, rendering, CI, and skill

- [ ] 4.1 Add `scripts/diagnose.ps1` with `<lang>`, `-All`, and `-Project`; the Rust CLI remains single-grammar
      (not done — no `scripts/diagnose.ps1` found)
- [ ] 4.2 Establish `incoming/<lang>/{grammar.*,words.txt}` with explicit gitignore negations for committed README/fixture files
      (not done)
- [ ] 4.3 Render build/assessment Markdown from their respective JSON artifacts, clearly separating compiler health from word-test evidence
      (not done — no Markdown renderer found, consistent with `define-fst-compilation-health`'s own
      missing Markdown side)
- [ ] 4.4 Add a tiny committed synthetic CI smoke fixture and validate the report schema
      (not done)
- [ ] 4.5 Add `.claude/skills/grammar-diagnostic/SKILL.md`, accurately handing slow-confirm investigation to `dead-end-census` rather than claiming three counters reproduce its d1–d6 census
      (not done — no such skill file found under `.claude/skills/`)

## 5. Verification

- [ ] 5.1 Produce a single-grammar artifact suitable for later consumption by `certify-four-language-matrix`; do not run or certify the four-language matrix here
      (partial — `diagnose` produces a single-grammar artifact; downstream consumption by the
      renamed/reframed Stage 4 matrix is not wired up)
- [ ] 5.2 Verify the authoritative Indonesian, Amharic, and pinned Aweti structural/result gates
      (not_run — real-language sample data for these gates is gitignored/absent per delanguaging Part
      A/B, and per `p6_aweti_gate.rs`'s own "NOT RUN" note)
- [ ] 5.3 Run strict OpenSpec validation when the CLI is available
      (see this bookkeeping pass's own final `openspec validate --all --strict` run)
