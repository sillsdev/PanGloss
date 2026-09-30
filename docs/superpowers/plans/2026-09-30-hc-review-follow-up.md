# HC review follow-up and Machine PR 480 repair

The user authorizes a new commit on Machine PR 480, updating the PanGloss submodule link, and fixing the remaining HC review findings. Foma/FST work is deliberately deferred. This follows the completed architecture assurance review at df697c7c and its evidence ledger.

## Ownership and sequence

1. Primary: repair the PhaseTraceRecorder ITraceManager signature on an isolated Machine worktree, preserve the failure reason, run focused regression tests and the complete `local_check.sh --agent-strict`, create a descendant commit, push normally to PR 480, verify its remote SHA, and update the PanGloss Machine checkout/gitlink. Preserve unrelated source and worktrees.
2. Luna interface lane: pg-lexicon option-aware lexical authority, pg-ffi rich generation and native option APIs, pg-wasm grammar-aware tokenization and generated JavaScript smoke. Preserve legacy ABI layouts; add rich JSON generation rather than erase supplied-root identity.
3. Luna reporting lane: pg-stats observation reuse by owning run options and counter semantics, CLI stats metadata, embedded executable build identity, and a required HC-only JavaScript/WASM CI job.
4. Primary: CLI completion status and successful auto-create substrate evidence at the compiler/loader seam, integration, independent Sol review, authoritative verification and updated review ledger.

At most two Luna agents write Workspace content. All Rust build/test work uses fresh managed pg.ps1 invocations; at most two managed builds run concurrently after memory/load measurement. Primary personally inspects every delegated diff and repeats authoritative checks.

## Acceptance contracts

- A14: reuse a requested word only when its current owning run has matching effective options_hash and counter_semantics. Recompute overlapping words when guess or timeout options change; preserve unrelated words and existing step-cap/final-template compatibility refusals. Preparation and flush share the same option identity computation. Test unchanged reuse, guess off/on/off, timeout changes, mixed-option accumulation, atomic replacement and rollback.
- A15: existing native v3 wire layouts and authored-root round trips remain compatible. Rich JSON analyze-to-generate carries supplied root identity/revision, grammatical data and authority through the runtime owner. Test supplied root plus affix, distinct roots sharing a surface, multiplicity, stale/forged identity, and explicit unsupported legacy supplied-root generation.
- A16: all native option APIs consume the supplied-lexicon runtime owner with the requested ParseOptions. Compare add/override/remove across single/batch and guess off/on, including full identities and completion flags.
- A17: text tokenization consumes grammar-owned orthographic facts, retains combining marks and grammar-supported punctuation, and reconstructs original text exactly. Test NFC/NFD whole-word/text parity and authored spellings/cache keys through actual generated JavaScript/WASM bindings.
- A18: complete plain parse stdout remains compatible. Incomplete plain or ordinary trace searches emit all applicable owner flags and exit unsuccessfully; rich trace retains its explicit search envelope. Expose the existing HC step-cap option without changing default budgets. Test real capped/invalid-shape outcomes and complete hit/rejection controls through the executable.
- A19: executable build revision is embedded at build time and distinguished from invocation context. Unknown remains unknown. Stats uses build provenance; the same executable run in another checkout or outside a checkout retains its build identity. Build-script invalidation follows HEAD/ref/index changes.
- A21: required CI builds HC-only WASM with the managed launcher and executes generated JavaScript transport checks with deterministic checked-in synthetic fixtures. Missing bindings or transport/status/identity failures fail the job. Optional private corpora remain explicitly skipped when absent.
- Successful auto-create phonology: retain the compiler's full SubstrateReport through the CLI loader, including inferred segments/boundaries and unresolved/ambiguous source witnesses. Successful inference is visible even with zero warnings. Call/extract the compiler owner; never infer policy from diagnostic text. Preserve importer/compiler warning deduplication. Test inference-only and authored controls and machine-readable parse stdout.
- Fresh C# evidence belongs to the repaired PR SHA and newly built executable. Run the strict oracle wrapper with explicit source-pinned executable paths; retain exact divergences and skips without broadening waivers.

## Explicit deferrals

Needs to be done later: A20 optional Foma health missing-measurement contract; Foma-only make-report build provenance (A19); explicit --workspace coverage CI Foma scope; all optional Foma/FST architecture, thresholds, retry, refusal, containment and production-admission work. No changes to these mechanisms or their optional checks are part of this pass. The previously implemented default Foma disconnection stays in place.

## Completion evidence

- [ ] Machine focused regression and full strict check; new commit and remote PR head verified.
- [ ] PanGloss root and review Machine checkout/gitlink updated to verified head.
- [ ] Each HC contract has red/green regression evidence at its owning seam.
- [ ] Delegated source and diffs personally inspected; independent Sol review resolved.
- [ ] Managed all-target check and full default tests pass, with exact attempted/pass/fail/skip counts.
- [ ] Actual JavaScript/WASM transport and managed PowerShell gates pass.
- [ ] Fresh C# oracle evidence recorded separately from historical evidence.
- [ ] Review ledger distinguishes fixed findings, unresolved oracle divergence, and explicit Foma/FST deferrals.
