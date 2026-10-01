# HC review follow-up and Machine PR 480 repair

The user authorizes a new commit on Machine PR 480, updating the PanGloss submodule link, and fixing the remaining HC review findings. Foma/FST work is deliberately deferred. This follows the completed architecture assurance review at df697c7c and its evidence ledger.

## Ownership and sequence

1. Primary: repair the PhaseTraceRecorder ITraceManager signature on an isolated Machine worktree, preserve the failure reason, run focused regression tests and the complete `local_check.sh --agent-strict`, create a descendant commit, push normally to PR 480, verify its remote SHA, and update the PanGloss Machine checkout/gitlink. Preserve unrelated source and worktrees.
2. Luna interface lane: native option APIs, grammar-aware WASM tokenization and generated JavaScript smoke. Primary owns the rich-generation runtime and thin native JSON adapter. Preserve legacy ABI layouts; add rich JSON generation rather than erase supplied-root identity.
3. Luna reporting lane: pg-stats observation reuse by owning run options and counter semantics, CLI stats metadata, embedded executable build identity, and a required HC-only JavaScript/WASM CI job.
4. Primary: CLI completion status and successful auto-create substrate evidence at the compiler/loader seam, integration, independent Sol review, authoritative verification and updated review ledger.

At most two Luna agents write Workspace content. All Rust build/test work uses fresh managed pg.ps1 invocations; at most two managed builds run concurrently after memory/load measurement. Primary personally inspects every delegated diff and repeats authoritative checks.

## Acceptance contracts

- A14: reuse a requested word only when its current owning run has matching effective options_hash and counter_semantics. Recompute overlapping words when guess or timeout options change; preserve unrelated words and existing step-cap/final-template compatibility refusals. Preparation and flush share the same option identity computation. Test unchanged reuse, guess off/on/off, timeout changes, mixed-option accumulation, atomic replacement and rollback.
- A15: rich generation validates supplied identities against one immutable runtime snapshot and retains all WordAnalysis fields. Its completion field is explicitly notAssessed: the existing synthesis owner enforces independent caps without publishing a complete outcome, so neither false cap flags nor a complete claim can be derived. Generation completeness instrumentation is a separately identified engine limitation; existing budgets and synthesis semantics stay unchanged. Existing native v3 wire layouts and authored-root round trips remain compatible. Rich JSON analyze-to-generate carries supplied root identity/revision, grammatical data and authority through the runtime owner. Test supplied root plus affix, distinct roots sharing a surface, multiplicity, stale/forged identity, and explicit unsupported legacy supplied-root generation.
- A16: all native option APIs consume the supplied-lexicon runtime owner with the requested ParseOptions. Compare add/override/remove across single/batch and guess off/on, including full identities and completion flags.
- A17: text tokenization consumes grammar-owned orthographic facts, retains combining marks and grammar-supported punctuation, and reconstructs original text exactly. Test NFC/NFD whole-word/text parity and authored spellings/cache keys through actual generated JavaScript/WASM bindings.
- A18: complete plain parse stdout remains compatible. Incomplete plain or ordinary trace searches emit all applicable owner flags and exit unsuccessfully; rich trace retains its explicit search envelope. Expose the existing HC step-cap option without changing default budgets. Test real capped/invalid-shape outcomes and complete hit/rejection controls through the executable.
- A19: executable build revision is embedded at build time and distinguished from invocation context. Unknown remains unknown. Stats uses build provenance; the same executable run in another checkout or outside a checkout retains its build identity. Build-script invalidation follows HEAD/ref/index changes.
- A21: required CI builds HC-only WASM with the managed launcher and executes generated JavaScript transport checks with deterministic checked-in synthetic fixtures. Missing bindings or transport/status/identity failures fail the job. Optional private corpora remain explicitly skipped when absent.
- Successful auto-create phonology: retain the compiler's full SubstrateReport through the CLI loader, including inferred segments/boundaries and unresolved/ambiguous source witnesses. Successful inference is visible even with zero warnings. Call/extract the compiler owner; never infer policy from diagnostic text. Preserve importer/compiler warning deduplication. Test inference-only and authored controls and machine-readable parse stdout.
- Fresh C# evidence belongs to the repaired PR SHA and newly built executable. Run the strict oracle wrapper with explicit source-pinned executable paths; retain exact divergences and skips without broadening waivers.

## Authorized integration and release

After the remaining HC fixes and authoritative verification, integrate onto current main with linear history, push, wait for green Rust CI at that exact main tip, and dispatch the CI-only Release workflow for 0.5.2. Author its CHANGELOG section before dispatch; the workflow owns version stamping, tagging and all four platform artifacts.

## Explicit deferrals

Needs to be done later: A20 optional Foma health missing-measurement contract; Foma-only make-report build provenance (A19); explicit --workspace coverage CI Foma scope; all optional Foma/FST architecture, thresholds, retry, refusal, containment and production-admission work. No changes to these mechanisms or their optional checks are part of this pass. The previously implemented default Foma disconnection stays in place.

## Completion evidence

- [x] Machine focused regression and full strict check executed; descendant commit and remote PR head verified. Build/tests/fixtures/parity and pushed Linux/Windows CI pass; strict catalog authority remains red at its unchanged unclassified bootstrap catalog.
- [x] PanGloss root and review Machine checkout/gitlink updated to verified head; review gitlink commit a786eed9.
- [x] HC owning-seam regressions pass; native authority and final WASM fixtures independently reproduce baseline failures; six-run preserved-cache replay passes. CI transport requirement has source/contract evidence pending hosted execution.
- [x] Delegated source and diffs personally inspected; independent Sol review GO at final source 665b7d6b.
- [x] Managed all-target check and full default suite pass: 1,641 attempted / 1,641 PASS / 0 FAIL / 64 explicit SKIP; default rustdoc also passes.
- [x] Actual generated default Node/WASM package and API check pass: 10 assertions PASS / 0 FAIL / 2 private-corpus SKIP; all 29 PowerShell files PASS.
- [x] Fresh C# oracle evidence recorded separately: staged 27 PASS / one exact known FAIL / one changed zero-width identity FAIL, upstream43 PASS, filtermirror9 PASS; strict overall exit26 preserved.
- [x] Review ledger distinguishes fixed HC findings, notAssessed generation completeness, unresolved oracle divergence/catalog backlog, and explicit Foma/FST deferrals.

The authorized integration/release phase follows this local verification checkpoint: fast-forward and push main, verify Rust CI at its exact tip, then dispatch and verify CI Release 0.5.2. The workflow owns the version stamp, tag and four platform artifacts.


Integration checkpoint: main was merged and pushed at ba055fe7. Hosted formatting, clippy,
default build/test and Linux containment pass. The required WASM job exposed an omitted
memory-controller setup before Cargo; a bounded repair shares the existing setup across
both child paths, retaining the 6 GiB cap and adapter preflight. The exact repaired main
Rust CI result must pass before the authorized release dispatch. See the review ledger
for the first hosted run and the discriminating local regression.
