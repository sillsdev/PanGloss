# Cleanup and Recipe Parity: Claude Continuation Handoff

Snapshot: 2026-08-01 18:51 America/New_York

> **Historical/superseded handoff.** This snapshot's four-language recipe-parity objective is
> retained for provenance. The current shipping slice is Indonesian, Amharic, and Aweti; do not use
> this handoff's four-language certification bar as the current acceptance gate.

## Objective and completion bar

Continue the user’s standing objective: complete three cleanup rounds and reach verified recipe parity for Indonesian, Sena, Amharic, and Aweti. Use Luna implementation/research agents as the normal path and independently review every delegated diff. Do not call the objective complete until the cleanup rounds, full four-language certification, final review gates, merge/push to `main`, and owned-worktree alignment are all proven.

Recipe parity means deduplicated `pg-assess::AnalysisIdentity` set equality. Repeated corpus occurrences remain separate observations. Duplicate analysis paths and guessed annotations are separate evidence; they are not identity. Full Rust `WordAnalysis::Eq` and vector multiplicity are not the parity relation.

## Authoritative integration branch

- Worktree: `C:\Users\johnm\Documents\repos\PanGloss\.claude\worktrees\cleanup-and-recipe-parity`
- Branch: `cleanup-and-recipe-parity`
- HEAD: `e1c0d35 docs: specify fallible parity projection`
- Status at snapshot: only untracked `.tmp/`; preserve it unless inspected and proven disposable.
- Main plan: `docs/superpowers/plans/2026-08-01-grammar-compiler-and-recipe-parity.md`
- OpenSpec tasks: `openspec/changes/cleanup-and-recipe-parity/tasks.md`

Nothing described below has been integrated into this branch yet.

## Plan state

1. Grammar identity/digest architecture: in progress.
2. Typed recipe semantics plus fallible `AnalysisIdentity` set comparison: pending.
3. Mandatory second compiler-architecture cleanup audit: pending.
4. Typed subrecipes and expanded conformance coverage: pending.
5. Cross-cutting xhigh reviews plus fresh Sol/xhigh consolidation: pending.
6. Full eligible-corpus certification for all four languages: pending.
7. Three-round completion audit, merge/push `main`, and reusable-worktree alignment: pending.

## Branch and worktree map

### Corpus certification completeness

- Worktree: `.claude/worktrees/crp-search-efficiency`
- Branch: `crp-search-efficiency`
- Committed handback: `c53b636 fix: fail closed on incomplete recipe corpora`
- Base includes integration HEAD `e1c0d35`.
- First Luna handback result: `C:\tmp\pangloss-fail-closed-certification.result.md`
- Reported gates: focused regression 1 passed; oracle gate 53 passed; full pg-foma 692 passed/59 skipped; CLI recipe optimization 6 passed/101 skipped; formatting/diff checks passed.
- Primary review did not accept `c53b636` unchanged.
- A Luna/xhigh correction is active through `C:\tmp\pangloss-corpus-scope-correction-runner.ps1`.
- Completion/status files:
  - `C:\tmp\pangloss-corpus-scope-correction.status.json`
  - `C:\tmp\pangloss-corpus-scope-correction.result.md`
  - `C:\tmp\pangloss-corpus-scope-correction.error.txt`
- Primary findings are persisted in `C:\tmp\pangloss-corpus-scope-primary-review.txt`.

Required correction review:

1. Directly test a `RunEvaluationCache` prepared for fewer/different occurrences than later requested, including excess duplicate multiplicity.
2. Prove typed `Truncated`, no selected winner, no Pareto frontier, and occurrence-preserving counts/digests.
3. Give exclusions a stable requested occurrence ordinal/ID; word text alone is ambiguous for duplicate rows.
4. Bind exclusion reason and occurrence identity into the exclusion-ledger digest rather than hashing only excluded word strings.
5. Enforce `requested == included + excluded` and deterministic ordering through a cohesive constructor/validator.
6. Prove a pilot subset is not poisoned by an unrelated excluded row in a larger prepared corpus.
7. Treat this evidence as transitional/non-authoritative until versioned `CorpusSnapshot`/`CertificationScope` lands.
8. Review serialized `Certification::Truncated` compatibility/versioning.

### Model revision and bound grammar foundation

- Worktree: `.claude/worktrees/crp-golden`
- Branch: `crp-model-revision`
- Existing committed foundation: `1268cbe feat: add grammar model identity foundation`
- Worktree is currently dirty with an active Luna/xhigh correction. Do not reset or discard it.
- Active runner: `C:\tmp\pangloss-model-revision-correction-runner.ps1`
- Completion/status files:
  - `C:\tmp\pangloss-model-revision-correction.status.json`
  - `C:\tmp\pangloss-model-revision-correction.result.md`
  - `C:\tmp\pangloss-model-revision-correction.error.txt`
- Primary findings are persisted in `C:\tmp\pangloss-model-revision-primary-review.txt`.
- At snapshot the changes span pg-grammar, pg-digest, pg-fwdata, pg-cli, pg-assess, schemas, tests, Cargo manifests/lock, plus new `pg-grammar/build.rs`.

Sound direction already present:

- `pg-digest` is a leaf owner for canonical/domain-framed digest mechanics.
- Source bytes, normalized compiler input, compiler identity, and `ModelRevision` are separate concepts.
- `BoundGrammar` is created only after successful compilation.
- CLI assessment/diagnostics are migrating to a single bound load path.
- Typed setup failure artifacts omit model identity.
- Assessment and investigation-handoff schemas have been moved to v2 during the active correction.
- XML validation is being tightened for text/CDATA/reference outside the single root.

Primary blockers still requiring explicit resolution and tests:

1. Enforce that successful reports carry `ModelRevision`, compiler-input digest, compiler ID, and assurance. Current `ReportDraft::finish` only enforces identity absence for setup failure; successful fixtures can still use `None`.
2. Remove presentation-only `WarningPolicy` from semantic compile identity. Warning suppression must not move `ModelRevision`.
3. Imported `.fwdata` must bind its canonical snapshot as the compiler input, so an equivalent direct snapshot can share compiler-input/model identity; raw `.fwdata` bytes and importer lineage remain distinct source provenance.
4. Eliminate double parsing of snapshot input in the CLI/binder flow; check XML for validator/parser divergence too.
5. Reconcile `PROFILE`, target, and rustc/toolchain fields with the compiler-identity contract. Include only inputs that can alter compiler semantics and add invariance/movement tests.
6. Prove the claimed local PanGloss compiler closure is complete for local normal/build/proc-macro dependencies and resolved features. An incomplete assurance must not issue an authoritative `ModelRevision`.
7. Add strong tests: relevant source change moves ID; path and CRLF/LF invariance; dependency/feature movement; missing/malformed/stale required provenance; equivalent imported/direct snapshot identity; XML/JSON/fwdata one compile/bind path; failed artifacts omit revision.
8. Ensure all evidence/certification paths consume `BoundGrammar`; compatibility loaders/`into_grammar` must not let authoritative callers discard identity.
9. Review schema-v1 compatibility explicitly. A legacy model fingerprint must never be coerced into `ModelRevision`.

### Capability projection correction

- Worktree: `.claude/worktrees/crp-ci-clippy`
- Branch: `crp-semantic-digest-falsification`
- Commit: `e444667 fix(pg-foma): scope grammar digest to capability projection`
- This commit is NO-GO unchanged after two-axis review.
- Reviews:
  - `C:\tmp\pangloss-capability-review\spec.result.md`
  - `C:\tmp\pangloss-capability-review\standards.result.md`
- Queued correction prompt: `C:\tmp\pangloss-capability-correction-prompt.txt`

Required correction:

1. Encode the exact canonical domain bytes `pangloss.capability-projection/v1`; current code splits base string and numeric version.
2. Remove typed-propagation escape hatches: loader-permitted zero-strata grammars can still reach `surface_table(...).last().expect(...)`.
3. Do not classify derivation/infrastructure failure as ordinary `Certification::CapabilityRejected` in recipe optimization.
4. Remove or make result-bearing the public deprecated `capability::characterize` panic wrapper.
5. Consolidate repeated characterization/envelope derivation and duplicated capability-error formatting where a cohesive API reduces drift.
6. Reassess the name `GrammarSemantics` if it owns only capability-profile facts; the broader name is acceptable only if imminent `BoundGrammar` integration makes it true.
7. Preserve the good portions: `CapabilityProjectionDigest` name, many-to-one/non-load-bearing documentation, fallible propagation, lexical-shape falsification, and `LoweredSpan` migration note.

Do not integrate `e444667` before this correction and before rebasing it onto the corrected model-identity foundation.

## Architecture decisions already adjudicated

Fresh Sol/xhigh review rejected `1268cbe` unchanged. The full result was returned in the prior Codex session; the operative decisions are recorded here and in the primary-review file.

- `SourceBytesDigest`: exact raw source lineage.
- `CompilerInputDigest`: normalized input actually consumed by the grammar compiler.
- `GrammarCompilerId`: deterministic relevant compiler implementation/semantic-build identity, never package version, timestamp, random, absolute path, or Git dirty Boolean.
- `ModelRevision`: successful compilation only; binds compiler input, compiler identity, and typed semantic compile options.
- `CapabilityProjectionDigest`: many-to-one typed capability projection; never a cache/model/certification identity.
- `ExecutableInputDigest`: lowered executable artifact/cache identity.
- `CandidateDigest`: recipe candidate provenance.
- `CorpusSnapshot`/`CertificationScope`: versioned occurrence-aware corpus claim boundary.

Git revision/dirtiness may be diagnostic provenance only. A source package or incomplete compiler closure may compile raw grammars, but must fail closed when asked to issue authoritative `BoundGrammar`/`ModelRevision` unless verified provenance is available.

## Analysis identity migration queued next

Prepared task prompt: `C:\tmp\pangloss-analysis-identity-migration-prompt.txt`.

Required behavior:

- Keep `pg-assess::AnalysisIdentity` v1 as the sole cross-engine identity: ordered stable source morpheme keys, root position, optional stable category/POS.
- Make projection and `AnalysisSet` construction fallible.
- Reject empty/colliding keys, unresolved ordinals, invalid roots, duplicate-count overflow, digest collisions, and conflicting guessed annotations.
- Replace recipe runtime’s full `WordAnalysis::Eq`, exact-vector-length/multiplicity checks, and dense tuple matching with deduplicated identity-set equality.
- Preserve repeated corpus rows as separate occurrences.
- Preserve distinct identities.
- Treat duplicate paths and guessed status as separate typed evidence.
- Refuse supplied roots and disable guessing for v1 four-language certification.
- Reuse one shared projector; do not duplicate identity logic in pg-foma.

Do not dispatch this until a suitable foundation/integration base is stable or an isolated worktree can be cleanly rebased onto it.

## Recipe/subrecipe architecture and parity state

The executable subrecipe foundation and dossiers are already present on the integration branch, but a prior xhigh linguistic review found that several family labels still map to generic plan-preserving transforms rather than executable linguistic mechanisms. The next architecture audit must verify that offered candidates vary a relevant mechanism derived from a construct-dependency graph, not merely carry a label.

Four-language state:

- Indonesian: strongest measured result and existing FullHC-confirmed recipe observation, but not certification-grade due corpus contamination/provenance and held-out-scope gaps.
- Sena: synthetic template-without-phonology routing defect is addressed. Full 7,121-row eligible-corpus evidence is still missing; apostrophe-bearing rows can be valid Sena and must not be categorically discarded.
- Amharic: no completed full-corpus winner. Templatic/interdigitating coverage and pathological runtime remain critical.
- Aweti: only a small pilot has confirmed; the full 208-row eligible corpus remains, with deep chains/zero-width/pathological words.

Before any parity claim, each language needs raw source hash, source/revision/license, deterministic eligibility transformation, explicit exclusion ledger, eligible-corpus digest, zero runtime omissions, compatible model/identity/corpus scope, all candidates reported, and corrected Pareto/winner evidence.

## Cleanup rounds still required

The objective requires three cleanup rounds, not merely passing patches.

1. Current foundation round: grammar identity/digest ownership, corpus fail-closed behavior, capability projection, shared analysis identity.
2. Mandatory compiler-architecture round: understand the complete grammar compilation pipeline, verify deep module boundaries/ownership, remove duplicate semantic walkers and identity reconstruction, and ensure the magic-sauce compiler surfaces claim the right abstraction level.
3. Cross-cutting round: after architecture stabilizes, run 2–4 Luna/xhigh reviews from different angles (maintainability, correctness/error contracts, schema/evidence compatibility, recipe/linguistic generality). Have a fresh Sol/xhigh review their combined findings before implementing/merging them.

Do not mark a cleanup task complete merely because a test passes. Update plans/OpenSpec only for facts proven by merged code and broad-enough evidence.

## Recommended continuation sequence

1. Wait for both active Luna runners to finish; do not kill legitimate Cargo/rustc descendants or stack retries.
2. Inspect final status, commit SHAs, changed files, and exact reported gates. Reject dirty/uncommitted handbacks or scope drift.
3. Apply the primary-review checklists above. Send focused Luna corrections in the same warm worktrees if needed.
4. Run primary `git diff --check`, focused managed tests, and proportional package/CLI gates. Do not use `-NoSccache` or private targets.
5. Obtain fresh heavy Sol/xhigh review of the corrected model-identity diff before integration.
6. Rebase corrected model identity onto `cleanup-and-recipe-parity`; integrate and verify.
7. Rebase/integrate corrected corpus scope.
8. Correct capability projection on top of the model foundation, then review/integrate.
9. Dispatch and integrate the fallible `AnalysisIdentity`/recipe-runtime migration.
10. Run the mandatory second compiler-architecture cleanup audit before treating Round 1 as stable.
11. Continue typed executable subrecipes/conformance coverage and full four-language parity measurements.
12. Complete cross-cutting cleanup reviews, full completion audit, merge/push `main`, and align/reuse owned worktrees.

## Orchestration constraints

- Reuse clean warm worktrees when their previous commit is contained in the target base; merged does not automatically mean disposable.
- Inspect existing worktrees before creating another. Retire only genuinely complete/inactive merged worktrees.
- Up to two managed builds have run concurrently successfully under ProcGov; do not start a third without measured headroom.
- Use the repository’s managed shared target/cache. Do not pass `-NoSccache`, invent target roots, or retry a timed-out managed build without proving no descendant/lease remains.
- ProcGov/rustc descendants can legitimately outlive a shell; do not kill them merely because a wrapper exits or a tool call times out.
- Keep Luna implementation agents medium+; use xhigh here for cross-component correctness. Luna research is xhigh.
- Heavy architecture/planning/final review gets a fresh Sol/xhigh agent. Primary still adjudicates and reruns gates.
- Avoid short polling loops. React to completion/status files and meaningful stale thresholds.

## Files outside the repository that matter

These temporary files are not committed and must be copied/summarized before machine cleanup if still needed:

- `C:\tmp\pangloss-model-revision-primary-review.txt`
- `C:\tmp\pangloss-corpus-scope-primary-review.txt`
- `C:\tmp\pangloss-capability-review\standards.result.md`
- `C:\tmp\pangloss-capability-review\spec.result.md`
- `C:\tmp\pangloss-capability-correction-prompt.txt`
- `C:\tmp\pangloss-analysis-identity-migration-prompt.txt`
- active runner/status/result/error files named above.

The key findings from those files are duplicated in this handoff so Claude can proceed even if `C:\tmp` is unavailable.

## Merge and completion warning

No branch in this handoff is approved for direct merge as-is. `c53b636` has a correction in flight; `crp-model-revision` is dirty/in flight; `e444667` is explicitly NO-GO. The integration branch and `main` have not received these slices. Four-language parity remains uncertified, cleanup rounds 2–3 remain, and the user’s objective is active.
