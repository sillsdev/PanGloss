---
name: pr-preflight
description: Use when preparing or reviewing a PanGloss branch before writing, opening, or updating a pull request; establishes verified risk, provenance, and validation evidence.
argument-hint: Optional branch purpose, PR URL, or base ref
user-invocable: true
---

# PR preflight

Use this as the entrypoint for PR preparation, branch review, review-summary generation, or
validation evidence. Read `docs/development-workflow.md` first. It owns discovery and review;
`pr-pitch` owns the final body after readiness is explicit.

## Establish the review boundary

1. Inspect `git remote -v`, `git branch --show-current`, `git status --short`, and verified
   remote/upstream branches. Resolve the base from the task or configured repository state;
   do not assume a tenant, remote, or `main` ref exists.
2. Pin the selected base, `HEAD`, merge base, file list, and diff. Record pre-existing changes
   separately. Keep the normal PR path on an isolated feature branch. An explicit commit-to-main
   request may use main, but it does not authorize push, PR, or tracker writes by itself.
3. If an issue is supplied, discover the available read-only tracker integration and its actual
   project/key format. If it is unavailable, ask for or use pasted ticket details; never guess
   Jira fields or client paths.

Completion criterion: the review notes name the verified base and head, exact diff, issue-source
status, and any pre-existing worktree changes.

## Review the change

Verify every finding against the current tree and, where useful, the base version. Classify
Critical, Important, or Minor only after confirming impact and evidence. Review these PanGloss
hot spots:

- parse completeness and soundness, including invalid and boundary neighbors;
- correlation and full parse identity, not feature-wise or count-only agreement;
- C# oracle provenance, Machine conformance paths, exact revisions, and unfinished divergence
  evidence;
- resource bounds, caps, timeouts, partial outcomes, and production-readiness claims;
- compatibility of docs, schemas, CLI/report contracts, ADRs, ledger entries, and fixtures.

Distinguish a normative contract from an implementation detail. Do not blindly accept reviewer
claims or silently treat current code as the contract. Preserve previously recorded fixes and
unfinished findings; a new pass may refine their status but must not erase them.

Run four compact review passes: contracts and compatibility; parser/oracle/conformance
correctness; resource bounds and managed build/test risk; and documentation, provenance, and
scope completeness. Use only the passes relevant to the changed files, but record why a pass
was not applicable.

For parser, conformance, shared-correctness, or ledger work, load the existing
`.claude/skills/oracle-alignment/SKILL.md`; for fixture work also load
`.claude/skills/conformance-grammars/SKILL.md`.

Completion criterion: each finding has a file/path, observed fact, risk, severity, evidence
status, and a narrowly scoped required action or explicit reason it is not actionable.

## Record and hand off

Write local `.review/summary.md` only as scratch, using the ignored path. Include:

- purpose and pinned review boundary;
- contract/API changes, findings by severity, positive observations;
- checks run, checks still required, manual checks not performed;
- author explanations, dismissed findings with reasons, open questions, and unfinished work;
- suggested review focus and limitations.

Do not commit or publish the summary unless separately authorized. For documentation-only work,
use editor/Markdown diagnostics, repository path/link checks, and whitespace checks as available;
do not claim a Rust build, parser suite, or conformance run. Changed-code validation must use
the managed `rust/tools/pg.ps1` wrapper and must report incomplete outcomes honestly.

After recording readiness or explicit draft limitations, hand off to `pr-pitch`; drafting does not
need separate publication approval. Commit, push,
PR mutation, Jira mutation, and thread resolution each require explicit task authority. Finish
by reporting changed paths, decisions, checks done, and limitations.
