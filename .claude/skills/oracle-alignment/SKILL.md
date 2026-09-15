---
name: oracle-alignment
description: >-
  Use before changing HC-Rust analysis/synthesis semantics, porting Machine optimizations,
  investigating C#/Rust parse differences, auditing the divergence ledger, reporting shared
  correctness concerns in Machine issues or PRs, or claiming parse-path optimization safety.
---

# Oracle alignment

Keep HC-Rust aligned with Machine's C# founding oracle while making known oracle defects explicit.
Read CLAUDE.md's oracle hierarchy, then `docs/divergences/README.md` for the authoritative ledger.
For fixture authoring or graduation, also read `.claude/skills/conformance-grammars/SKILL.md` and
the full `machine/conformance/PROTOCOL.md`.

## 1. Establish current evidence

Record immutable C# and Rust commits and the exact fixture/test paths. Inspect current upstream
heads, not just old review comments. Distinguish an implementation, a unit test, an exported
conformance grammar, and a demonstrated failing-before/passing-after result.

Classify each concern as a reproduced bug, a suspected bug, a coverage gap, or an efficiency
difference. State which engine is affected. An unstable C# oracle can affect PanGloss comparisons
without being a Rust implementation bug. Do not infer absence from a search miss: inspect the
relevant branch tree before claiming a fixture or fix does not exist.

## 2. Separate correctness from efficiency

HC-Rust may be faster or leaner without changing observable parses. Every optimization needs an
argument covering both lost valid parses and admitted invalid parses, plus discriminating evidence.
Fewer rule attempts, confirmation of surviving candidates, or matching finite samples alone cannot
prove complete recall. Per-feature widening must account for correlations between alternatives.

For an implementation change:
- Compare before/after release binaries built in separate worktrees, using `rust/tools/pg.ps1`.
- Run binaries through `pg.ps1 -Mode run`; compare batch TSVs with
  `python rust/tools/parse_compare.py before.tsv after.tsv`.
- Require complete word-row coverage, matching statuses, and identical parse-identity multisets.
  `IDENTICAL` and `MULTISET_EQUAL` can pass. `SET_EQUAL` alone cannot: it hides multiplicity
  differences. Missing rows, caps, skips, errors and timeouts are incomplete evidence, not equality.
- Record deterministic counters and actual cache/prune hits. A branch that never executes is not
  validated by its test passing.
- Cover the five reference grammars and their worst-word lists, with both divergence directions.
  Report any unfinished comparison explicitly; do not label it full parity.
- Run the managed focused tests and conformance gates; use `-Scope all` for parse-path changes.
  For documentation-only cleanup, run the relevant documentation gates instead of claiming a parser
  suite was rerun.

## 3. Establish expected parses independently

Run the founding oracle before transcribing outputs. Prefer `rust/tools/oracle-conformance.ps1`;
it distinguishes unavailable oracle (exit 25) from a genuine signature divergence (exit 26).
A zero exit reports no NEW divergence, so inspect any printed baselined divergences as well.
Use a raw harness invocation only when the wrapper cannot perform the targeted experiment.

When C# is believed wrong, preserve its observed output and commit in the evidence, but do not
encode that wrong output as the required semantic answer merely to make a test green.
The conformance skill permits a `forward-synthesis` fixture deliberately red against C# when
a derivation independently proves the expected parse. Include negative and non-vacuity controls,
a derivation trace, explicit provenance, and a linked upstream report. Do not silently mix observed
and derived expectations: identify every derived row. A second fixture freezing a known defect is
not mandatory.

An oracle-unverified fixture must say which engine supplied its observations and why C# was
unavailable. Rust output alone is not correctness evidence. Never convert a timeout or skipped
case into an expected rejection.

For each claimed fix, demonstrate its regression fails when the fix is removed. Keep this separate
from grammar-counterfactual checks: changing a grammar attribute proves the fixture sees that
attribute, not that a particular engine implementation is necessary.

## 4. Open or reuse the shared Machine report

Search existing Machine issues and related PR discussions first. For a concern shared by the HC
semantic contract, both implementations, or PanGloss's oracle dependency, open or reuse an issue in
`sillsdev/machine`. A Rust-only implementation bug remains local unless evidence also implicates C#.
Respect the user's write authorization; a read-only review may propose an issue but not post it.

The report must contain:
- Reproduction and exact revisions, with expected versus observed complete identities.
- Independent justification for expected parses, or explicit research questions if not yet known.
- Related PRs, existing fixtures and historical experiments, so the work is not recreated.
- Acceptance criteria covering soundness and recall; separate any performance claim.
- The remaining evidence gaps, without presenting a hypothesis or coverage gap as a confirmed bug.

An issue records the concern; open a fix PR when there is a reviewable implementation. A posted PR
comment is evidence of reporting, not a fix PR. An unposted draft is not an upstream report.
A documented, independently justified, tested divergence with an upstream issue may remain open
during research; the issue does not itself prove the Rust behavior correct.

Use a Machine worktree for edits, never switch branches in the shared main Machine checkout:
`C:\Users\johnm\Documents\repos\machine\.worktrees\<slug>`.
Check branch and PR state before building: a reused path is not an immutable revision.

## 5. Reconcile the ledger

Every divergence and every optimization gets an entry under `docs/divergences/`; reference extra
conformance grammars by name. Keep entry numbers stable and update `by-module.md`.
For each entry record independently:
- Current Rust implementation and commit.
- Current C# implementation and commit.
- Machine issue, fix PR and/or posted comment links.
- Exact conformance fixture versus unit-test coverage.
- Actual verification, fix-removed sensitivity, and remaining gaps.

A PR being merged does not close an entry if Rust implements a different fix or parity is unverified.
Mark superseded algorithms as historical and link their replacement. Do not turn an old timestamp
into a current claim. Keep live status in the ledger rather than duplicating it in this skill.
Entry 002 is the Exact-inversion case; entries 034/035 track its merge interaction.

## Completion check

- [ ] Both lost and invalid parses considered; evidence type and engine status explicit.
- [ ] Shared concern has a Machine issue (or posting is explicitly blocked by authorization).
- [ ] Ledger and module index link the actual issue, PR/comment and fixture locations.
- [ ] Expected parses independently justified; unavailable or incomplete runs not called passes.
- [ ] Claimed mechanism exercised; fix-removed versus grammar-mutant evidence kept separate.
- [ ] Relevant managed gates rerun; no broader completion claim than their results support.
