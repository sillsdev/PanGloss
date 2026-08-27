# Claude continuation handoff: finish the PanGloss simplification cleanup

Date: 2026-08-27

## Objective

Continue the pre-alpha PanGloss cleanup as **remove old cruft**. Finish the authorized demolition
before designing or wiring replacement functionality. Do not restore rejected behavior merely
because compilation or an old test expects it. Delete or rewrite the obsolete test contract first,
delete its source second, and remove stale prose last.

The cleanup is substantial but not nearly complete end to end. The committed rip-first range is
currently **17,506 deletions / 1,520 additions, net -15,986 lines across 196 files**. The broader
cleanup range, which includes prerequisite containment infrastructure, is **20,568 deletions /
10,508 additions, net -10,060 lines**. Do not count the three protected uncommitted files in either
total.

The authoritative decision ledger and marching orders are in `docs/simplification-rip-list.md`.
This handoff summarizes the live state; update that ledger as reviewed slices land.

## Repository and branch state

- Repository: `C:\Users\johnm\Documents\repos\PanGloss`
- Integration worktree:
  `C:\Users\johnm\Documents\repos\PanGloss\.claude\worktrees\cleanup-worker-contract-acceptance`
- Branch: `cleanup/worker-contract-acceptance`
- Current HEAD: `78e0d319c04e42d8cdba2ec938011fc30efebd44`
- Local `main`: `4d4db3f86afe8810c5e2a30bcec3e8edc295aab4`
- Merge base with `main`: `4d4db3f86afe8810c5e2a30bcec3e8edc295aab4`

After this handoff itself is committed, the pre-task worktree baseline must contain only these three
pre-existing dirty files. Every authorized continuation edit must be committed before moving to the
next task, returning status to this same three-file baseline; do not revert an authorized committed
docs/source change merely to make status resemble the baseline:

| Protected file | Required SHA-256 |
|---|---|
| `rust/crates/pg-cli/src/make_report.rs` | `A5EB00E386230510B018CD4B012538F657C11A6B50B08A2A0D57E06CF11B88D2` |
| `rust/crates/pg-cli/src/pack.rs` | `5EB5406BA49EE1B628D77A951618BC328374349D02CD92BB16306B7AF7F04036` |
| `rust/crates/pg-foma/src/emit.rs` | `19C177E874D3F83D7DD4B84AE3F13EC738F43FF132E70AEAE1AA32D573BF0825` |

Do not reset, restore, overwrite, format, or casually stage these files. Recheck the hashes before
and after every delegated write. The `make_report.rs`/`pack.rs` diff contains a protected
publication-gate deletion discussed below. The `emit.rs` diff crosses a containment boundary and is
not approved for staging.

The main checkout has unrelated user work. During the last docs correction a Luna writer briefly
patched four main-checkout paths, then inverse-patched only its own textual hunks. Two paths are
clean; `docs/conformance/representative-typology-basis.md` and
`docs/research/grammar-feature-space.md` may show line-ending-only `.M` state while their Git blob
IDs match `HEAD` and `git diff` has no textual hunks. No pre-edit status snapshot exists for those
two paths. Inspect; do not normalize, restore, or clean them automatically.

## Ratified behavior decisions

These are not open questions.

### Pipeline

1. **Analyze** runs independently without compiling. It reports per-backend representability,
   warnings, and grammar-derived cost estimates.
2. **Choose** is explicit. Production configuration names one backend; a local command may
   explicitly request one or several. There is no preferred backend, top-N, fallback, or retry.
3. **Build** revalidates representability and compiles each explicitly requested backend exactly
   once in a supervised worker. Requests run sequentially and independently; one failure does not
   suppress another requested build.
4. **Test** applies completed artifacts to a large corpus and reports raw comparison metrics. It
   does not select a winner.
5. **Package** consumes one explicitly named completed artifact. Packaging never compiles or
   substitutes another backend.

### Execution limits

- No named resource envelopes and no “increase the envelope” retry.
- Every build attempt has configurable, finite, positive limits:
  - 1 GiB final serialized FST payload by default;
  - 10 GiB committed memory for the entire worker process tree by default;
  - 10 minutes wall-clock time by default.
- Limits may be raised or lowered but not disabled.
- They are operational containment, not representability facts or backend-selection inputs.
- Do not claim all production compile paths are contained yet; spawn wiring remains incomplete.

### Proof and publication

- Process/resource failure is distinct from representability and publication readiness.
- `--allow-unproven` remains **local FST generation/testing only**. It may retain local evidence and
  omit parses. It is never accepted by publication and creates no persistent pack trust override.
- Corpus success never promotes an unproven artifact.
- Packs bind an artifact to grammar digest, backend, protocol/compiler identity, and effective
  semantic build configuration.
- Pre-1.0 schemas/protocols are strict lockstep; delete compatibility shims rather than preserving
  old data.

### Backend switches

- Cross-backend automatic preference/ranking/selection must die.
- Grammar-required correctness routing stays automatic and cannot be configured off.
- Within-backend performance switches normally use `auto` and eventually become configurable, but
  their redesign/configuration exposure is deferred to the immediate post-cleanup round.

## Major cleanup already landed

The following behavior has been removed and must not return:

- named resource-envelope identity, size modes, envelope retry, `--remove-size-limits`, and legacy
  `--no-enforce-capability` behavior;
- the direct standard-library worker supervisor and old selected-artifact filesystem transport;
- top-N/preference chooser APIs and the unused selected-worker convenience wrapper;
- the legacy Pack compile/watchdog/placeholder path and its producer tests;
- persistent publication override/trust representation;
- direct Foma parse/batch/stats routes, automatic WASM Foma analysis, and the CLI `diagnose`
  command;
- the grammar/corpus assessment producer and grammar-rerun investigation route; compare,
  golden-diff, and report-only investigation remain;
- standalone health compilation and build-time corpus evaluation;
- duplicate closure/pre-expansion characterization traversal and callerless closure advice;
- compile-retry/backend-substitution advice;
- old pack/report/schema compatibility defaults and fixtures;
- net-size, generated-line, compose-timeout, tuple/group, ordering-multiplicity, eager-enumeration,
  profile-band, and finite-quantifier refusal caps;
- numerous callerless wrappers, accessors, diagnostic variants, outcome fixtures, and dead tests.

The real build pre-expansion traversal remains because it is needed for interdigitation and
boundary-fusion correctness.

## Current tranche: finite quantifier ceiling

The arbitrary `MAX_QUANTIFIER_BOUND = 512` policy was removed in this order:

- `8b8277bf`: removed the stale finite-cap fixture/test first (`+5/-33`);
- `f23ad388`: removed the constant and both refusal checks second (`+9/-16`);
- `9bf811f6`: removed stale source/docs claims (`+47/-60`);
- `78e0d319`: corrected additional staging/status prose (`+41/-59`).

Total: `+102/-168`, net `-66`.

Primary inspection confirms large finite quantifiers now use native lowering while the following
remain rejected: inverted finite ranges, empty children, alpha-nested quantifiers,
disagree-polarity alpha variables, and other unsupported pattern constructs. The useful positive
unbounded-large-min test remains.

This tranche is **not accepted yet**. Fresh Luna spec re-review found three Important documentation
blockers:

1. `conformance-staging/edge-cases/right-to-left-bounded-quantifier-rewrite/grammar.xml:25-28`
   still names the old refusal test and says unbounded RTL quantifiers are refused.
2. The same fixture's `STAGING.md:63-68` still says the coverage test expects
   `CompileDecision::Refuse`; current behavior is `ConfirmOnly`.
3. `docs/benchmark-matrix.md:169-170` describes unbounded-quantifier task 4.5 as future work even
   though the same document records it closed.

Minor: `docs/conformance/representative-typology-basis.md:118` has a stale historical section
reference to the unbounded-quantifier gap.

### Immediate next action

If the current native-agent sessions survive, use implementation writer
`/root/luna_rip_quantifier_cap` and spec reviewer `/root/luna_quantifier_spec_review`. If they do not,
dispatch replacements with the exact findings and scope in this section; continuity of requirements
matters more than an expired session identifier. Make one docs/fixture-only commit correcting those
four items. Do not touch Rust source or the three protected files. Then:

1. personally inspect the exact diff and residue searches;
2. send it back to the same spec-review role for re-review; **spec GO means zero unresolved
   Critical or Important findings and explicit confirmation that all previous findings are fixed**;
3. only after spec GO, dispatch a different fresh Luna quality reviewer over
   `bf551516..$(git rev-parse HEAD)` (resolve the endpoint to the actual corrective commit SHA in the
   review prompt; do not send the literal placeholder);
4. fix and re-review every Critical/Important finding;
5. after both reviews approve, record the accepted tranche and fresh counts in
   `docs/simplification-rip-list.md` as its own narrow tracker commit; inspect that commit separately.

No Cargo/build/test/format command belongs in this demolition review.

## Next authorized removal: dead ComposeBudget forwarding

A read-only Luna/xhigh audit found that the next smallest coherent source deletion is the no-op
budget-parameter chain:

- `uflexc::emit_underlying_filtered_with_budget` accepts `&ComposeBudget` but does not read it;
- `gate::compile_gated_grammar_with_budget` only forwards that argument;
- `build::build_controllable` only forwards it onward.

Stage this as a new bounded task:

1. Update/remove only tests and fixtures whose contract is the no-op argument.
2. Collapse the uflexc pair into the existing honest name `emit_underlying_filtered`: delete the
   env-reading wrapper body and `_with_budget` name, move/retain the real core body under
   `emit_underlying_filtered`, and update callers to omit the budget argument.
3. Collapse the gate pair the same way: retain the public name `compile_gated_grammar`, delete the
   env-reading wrapper and `_with_budget` name, and update callers to omit the argument.
4. Remove only the `&ComposeBudget` parameter from `build_controllable` and its direct call sites;
   do not recursively delete higher-level budget parameters without a new caller/behavior audit.
5. Remove stale prose that says these paths enforce removed size/time budgets.
6. Commit tests/contracts first, source second, prose last.
7. Use structural checks only during demolition, followed by fresh spec and quality reviews.

Protected boundaries for that tranche:

- retain `ComposeBudget` itself;
- retain `CHAIN_DEPTH_ABSOLUTE_CEILING`, `check_chain_depth`, and peel/F6 depth contracts;
- retain `ApplyBudget`, apply path/candidate limits, and OOM-protection behavior;
- retain compound-chain depth and closure work/depth guards;
- retain real pre-expansion and probe termination guards;
- retain marker representability checks;
- do not extend the tranche into analyzer/worker `chain_depth_cap` plumbing;
- do not touch protected `emit.rs`.

`ComposeBudget::with_caps` is already missing while many integration tests still call it. Treat
those as intentional compile holes. Classify each stale call; remove budget-only test plumbing, but
do not delete protected F6/termination tests merely to hide the hole. Never add `with_caps` back to
make an intermediate test compile.

## Remaining removal and decision queue

Keep every item staged independently and tracked in the rip list.

1. **Direct make-report compile/corpus route:** still present in protected `make_report.rs`. The
   command's ultimate fate requires an explicit decision. Recommended direction is to delete the
   producer route, but do not infer approval.
2. **Publication severity gate:** its stale test was deleted in `0e001bdc`. A matching protected
   `make_report.rs`/`pack.rs` source diff exists, but no live Pack writer currently publishes a
   payload. The exact consequence must be accepted before staging; do not claim this presently
   changes real publication rejection.
3. **`REP_VARIANT_CAP`:** authorized in principle as recall-losing resource overflow plumbing, but
   the current protected `emit.rs` patch is NO-GO because not every production compile path is
   contained. Do not stage it until a read-only production-call inventory proves every caller of
   `surface_variants`, `surface_variants_concat`, `surface_insert_action_variants`,
   `pattern_variants`, and `stripped_variants` runs exclusively inside the supervised worker's
   process tree under finite memory and time limits. The inventory must explicitly cover or remove
   the direct make-report, FFI, precision, and pre-expansion compile entry points. If even one live
   in-process production path remains, the cap-removal patch stays NO-GO. After a Luna/xhigh
   architecture audit, require a fresh independent review of that conclusion before editing or
   staging protected `emit.rs`.
4. **Cross-backend ranking residue:** chooser/top-N APIs are removed, but run a fresh call-site audit
   before declaring D2 complete. Do not touch deferred registry/mechanism/Plan tuning internals.
5. **F10 dead-test sweep:** remains the largest unexplored surface. Remove only tests proven
   callerless, vacuous, or tied to rejected behavior. Preserve semantic/termination proofs.
6. **Small stale-doc residue:** examples include `analyzer.rs` claiming `pg-cli pack` writes a pack,
   `fix-a-grammar/NOTES-research.md` describing a live pack gate, old ADR/STAGING producer wording,
   and `fst_health.rs` saying characterization performs no compile although simultaneous-subrule
   characterization constructs Foma FSMs.
7. **Narrow open cleanup items:** duplicated `ConfirmedBuckets` flattening and repeated
   `HealthFinding` construction remain, but they are consolidation rather than high-priority cruft
   deletion.

Do not mix correctness bugs (`G1-G5` in the rip list) into mechanical deletion slices. The emitter
consolidation and tag-reachability issue require their own evidence and are not authorized pure
removals.

## Explicitly deferred until immediately after cleanup

- redesigning within-backend `auto` and exposing tuning controls;
- recipe/Plan search and experimental transformations;
- precision/strategy scaffolding;
- whole-grammar Plan IR decisions;
- the final explicit Analyze/Choose/Build/Test/Package replacement surface.

When demolition is exhausted, build the smallest coherent replacement, repair intentional compile
holes, add only tests for the ratified final contracts, and then perform authoritative verification.

## Required working discipline

- Follow repository `AGENTS.md`.
- Use the `agent-handoff` skill at high reasoning and Luna as the normal implementation/research
  path. Up to two Luna agents may have Workspace access.
- One implementation writer at a time. Read-only audits may run orthogonally.
- Give every writer exact paths, protected hashes, acceptance metric, staging order, and prohibited
  scope.
- The primary agent personally inspects every delegated diff and claim.
- Each task requires fresh spec review, then a different fresh quality review. Do not reverse the
  order or accept unresolved Critical/Important findings.
- Never stage the whole dirty tree. Stage exact paths/hunks and inspect cached diffs.
- Use `apply_patch` for edits.
- During demolition do not run Cargo, tests, builds, formatting, or compile-hole repairs. Use
  `git diff --check`, exact positive and negative residue `rg` searches, per-commit `--numstat`,
  `git status --short`, full diff inspection, and protected-file hashes.
- Before any later build-heavy delegation, measure live physical memory, committed headroom, CPU,
  procgov, Cargo, and rustc trees. Keep all Rust work inside `rust/tools/pg.ps1`.
- Up to two concurrent managed builds are allowed only when fresh measurements show both 19 GiB job
  caps fit with ample headroom.
- A failing old test never authorizes restoring rejected behavior.
- `docs/simplification-rip-list.md` is the completion checklist: every row must finish as reviewed
  `VERIFIED`, reviewed `LANDED UNVERIFIED` followed by the final verification gate, deliberately
  `RETAINED`/`REJECTED`/`DEFERRED NEXT`, or a user-resolved decision. No `AUTHORIZED`, `PARTIAL`,
  `OPEN`, `VERIFY`, or unreviewed tranche may remain when declaring demolition exhausted.
- Do not mark the overall cleanup complete until that ledger gate is satisfied, replacement
  contracts are coherent, authoritative verification passes, all delegated claims are personally
  checked, the protected diffs are resolved intentionally, and the final worktree is clean.

## Suggested continuation sequence

1. Correct and fully review the remaining finite-quantifier docs residue.
2. Update the rip-list ledger with the accepted quantifier tranche and current line totals.
3. Remove and review the dead uflexc/gate/build `ComposeBudget` forwarding chain.
4. Run fresh read-only audits for remaining cross-backend ranking residue and F10 dead tests.
5. Execute only newly proven, bounded removal slices.
6. Return to the user for the direct make-report and exact publication-gate decisions if local
   evidence cannot resolve them.
7. Resolve the REP/containment boundary separately; never slip it into another cleanup commit.
8. Sweep stale contracts and quantify the final demolition.
9. Only then implement the minimum replacement surface and run managed authoritative verification.

The governing principle is simple: **the old behavior is not innocent until a test passes; it is
dead when the ratified contract says “kill it.”** Preserve only semantics, termination, and explicit
operational containment.
