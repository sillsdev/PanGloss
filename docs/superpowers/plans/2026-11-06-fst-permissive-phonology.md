# Safely permissive phonology implementation plan

> Execute inline with the executing-plans workflow; the owner requested no delegation.

**Goal:** Produce complete proposing rewrite cascades with independent site choices and typed lowering refusals.

**Architecture:** Keep lowering in `lower.rs` and replacement construction in `replace.rs`. Widen alpha constraints and material choices, then use one batch of obligatory replacement-or-unchanged choices per rule. Iterative contexts observe the processed side; simultaneous contexts observe the input. HC confirms all proposals.

**Tech stack:** Rust, the pinned foma 0.4.2 replacement calculus, and the existing conformance/differential tools.

**Spec:** Owner-approved `fst-structural-design.md`, section 4 stage 1, supplied with the FST production brief.

## Global constraints

- Stage 0 is already on main. Per the RTL re-review instruction, squash P2FIX and Stage 1 into one commit on `efbffef1` after verification; keep the earlier stack's evidence in the external report.
- No partial-specific capability, new cache, silent truncation, restored backend, or factored morphology construction.
- Production alpha admission stays guarded until a complete morphology/phonology route supplies recall.
- Heavy grammar attempts are serial, with 8 GiB virtual address space and a 3,600-second hang guard.
- Cargo uses the shared lock and four jobs/threads; required workspace and preland gates precede completion.

## Review focus

- Mixed matching sites preserve unchanged material within a changed word.
- Material choices differ independently across sites, including class feature combinations.
- Literal, cross-subrule and cross-rule feeding work in both directions.
- Empty cascades are identity; unsupported successors leave no successful partial net or reports.
- Defaults, word anchors and morpheme boundary targets retain all valid outputs.

## Tasks

- [x] Fix review P2s independently, verify environment effects and routing, commit as `6169e54d`.
- [x] Freeze clean-parent differential binaries and provenance before relation changes.
- [x] Observe direct relation regressions and record pinned C# verdicts for a fully specified staged fixture.
- [x] Implement per-site optional widening and shared subrule replacement batches in the existing owners.
- [x] Consolidate complete cascade APIs and move the two lab callers onto the typed proposing contract.
- [x] Finish focused verification and inspect the complete diff (30 relation tests pass).
- [x] Measure the same fixture/real-grammar cohort before and after; retain every refusal and incomplete row (28 real-grammar attempts, no shared fixture-cell regressions).
- Required before READY: managed check, the required workspace command and exact preland; retain the final receipts in the external report.
- [x] Commit Stage 1 locally with the required trailer and write the external report with receipts, as explicitly authorized while waiting for the separate rewrite fix.

## Integration of the rewrite lane

Before rebase, managed check and preland passed, while the full workspace gate was red only on the staged `uyi` fixture: pinned C# returned `SUBRULE|uyi`, while HC-Rust returned no analysis. The owner assigned that gap to `pg-rewrite-feeding` and authorized a local Stage 1 commit while waiting. This lane adds no `pg-rules` changes or divergence/allowlist entry.

The rewrite fix landed in main as `efbffef1` (#195). Rebase the local P2FIX/Stage 1 stack onto it, dropping the already-landed Stage 0 patch. Main's `per-site-alpha-rewrite` grammar/words are identical to this lane's; keep main's LTR and RTL fixture copies. Final workspace/preland verification and readiness are recorded in `/home/johnm/work/fst-production-20261103/fst-strategies-report.md`.

## Review P1: expansion inclusion

The independent `a -> ai` regression fails on the reviewed native `(->)` construction,
despite the obligatory relation and pinned C# retaining `ai`. Classify this as
correctness/representability; change no resource bound or admission policy.
Use explicit replacement-or-unchanged material alternatives in an obligatory batch.
For insertion, represent unchanged sites with a context-transparent temporary symbol
and erase it from the output after construction.

The direct suite now passes 34 replacement tests, including 112 independently
specified operand-shape/mode/direction combinations and the reviewer's named
regression. The staged `permissive-multisegment-output` fixture has seven C#-verified
positive/negative words covering single, repeated and mixed sites. Preserve the
separate P2FIX commit and amend Stage 1 after fresh check, workspace and preland
verification. The separate HC-Rust ambiguous-material discrepancy remains outside
this lane, with no local fix or divergence entry.

## RTL context expansion re-review

The re-review's literal `au -> aau` and final-anchor `a -> aa` witnesses fail
on `0627ce48`; the extended nonempty-focus matrix reproduces the RTL processed-right
context loss. Both staged reviewer fixtures pass a fresh pinned C# replay.
Mirror all operands and swap/reverse environments for the entire RTL iterative
batch, compile with LTR processed-side semantics, then reverse both tapes of the
result. The reversal argument is in the construction-strategies documentation.
It preserves processed-side feeding; it does not change contexts on the original
operands. Expand the matrix through literal, anchored and alpha environments,
with directional feeding/bleeding and atomic-refusal controls, then run both
required gates on the final squashed commit. Record receipts externally.

The extended sweep also found unproven simultaneous multi-token focus mappings.
Refuse that construction explicitly at the lowered-focus owner with
`SimultaneousMultiTokenFocusUnavailable`, propagating the typed error through the
whole cascade. This is correctness/representability containment, not a readiness
or resource limit. Do not alter pg-rules or add a divergence entry. Matrix rows
for that shape assert the refusal, while all supported rows retain their forward
expectations; the bounded search inventories 1,200 compiled and 600 refused cases.

## Intermediate representation review correction

The third P1 is correctness/representability: attested-token projection loses
unspelled feature intermediates. Replace the proposing path's two redundant
pattern scopes with one per-cascade bundle closure in the lowering owner.
Reuse the shared RHS pin/full-lane computation. Preserve intermediate tokens
through composition and spell only at the end. Fixed closure containment is
4,096 rows / 1 MiB conservative charge, with typed atomic refusal, no cache.
The construction document states the inclusion invariant and refuses every
uncovered construct, including iterative overlapping multi-token focus.
Stage the two C#-verified review fixtures; leave the iiiaiii port bug unstaged.
Rebase on landed #196 (5d500a23), measure combined scoreboard by name, squash
to one commit, then repeat the required full workspace and preland gates.

## Review af9a4b58: iterative insertion scope

Correctness/representability: refuse all iterative empty focus atomically with
`IterativeFedInsertionUnavailable`. Do not model fed insertion or claim a
nonfeeding exception. Retain simultaneous original-gap insertion and its tests.
Stage and replay bounded-iterative-epenthesis; compare every named scoreboard
cell against main 5d500a23. Measure the existing real-cascade closure refusal
limits without changing them, then rerun workspace and preland.
