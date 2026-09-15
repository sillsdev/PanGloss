---
name: jira-bugfix
description: Use when a PanGloss defect is identified by a Jira or other issue-tracker ticket and the user wants an evidence-backed bugfix workflow.
argument-hint: Optional issue key, URL, or pasted ticket details
---

# Issue-tracker bugfix workflow

Read `docs/development-workflow.md` first. This skill adapts the FieldWorks Jira flow without
assuming a Jira tenant, project, schema, client path, or mutation permission. It supports an
explicit commit-to-main request; normal PR work uses an isolated feature branch.

## 1. Establish the issue and branch

1. If the task explicitly names a local plan item, use that plan without requiring Jira. Ask which
   tracker only when ambiguous. Otherwise discover the tracker and available read-only tooling. Fetch the issue only
   through that safe integration, or use details pasted by the user. Record summary, type,
   status, priority, acceptance criteria, reproduction, comments, and unknowns using the
   tracker’s actual fields. If it is not a defect, confirm that this workflow still applies.
2. Inspect remote, branch, upstream, and worktree state. Do not switch branches, discard, or
   stash user files. Keep unrelated changes separate. Assignment, transition, branch creation,
   commit, push, PR, and ticket updates each require explicit authority.
3. Preserve previous fixes, unfinished findings, ADRs, divergence entries, and conformance
   evidence. Do not close an issue for a partial slice or claim universal correctness from a
   finite suite.

Completion criterion: the issue source, exact requirement/reproduction, verified branch boundary,
and pre-existing work are recorded, with missing information called out.

## 2. Reproduce red before implementation

Understand the contract and root cause from the issue and code. For a parser defect, read
oracle-alignment and use the C# founding oracle plus exact Machine conformance evidence as
applicable. Write the smallest failing regression before the fix. The red case must assert the
observable contract: complete parse identities and statuses, not only analysis counts. Add a
negative or boundary neighbor and order/memoization controls where those could regress.

Run the regression through `rust/tools/pg.ps1` before implementation and record the actual failure.
Confirm it exposes the stated defect, not a build/setup problem. For a claimed mechanism fix,
distinguish fix-removed sensitivity from changing the grammar itself.

If an automated test is impossible, name the concrete constraint and propose an alternative
check; do not silently substitute a passing smoke test. A timeout or resource cap is an
incomplete outcome, not a red expected rejection.

Completion criterion: the regression fails for the stated reason, and its positive, negative,
boundary, identity, oracle, and resource evidence requirements are explicit.

## 3. Implement green and review the scope

Make the smallest fix that satisfies the verified contract. Keep docs, implementation, and
oracle evidence distinct; if the implementation conflicts with a normative contract, record
the decision rather than silently declaring the code authoritative. Re-run the red case green,
then assess adjacent callers, compatibility, soundness/completeness, correlation/identity,
resource bounds, and negative/boundary controls.

For changed Rust or conformance behavior use the managed `rust/tools/pg.ps1` wrapper. For
documentation-only changes use Markdown/editor diagnostics, path/link checks, and whitespace
checks as available. Report every skipped check and every incomplete result.

Completion criterion: the fix passes the targeted regression, meaningful controls cover likely
adjacent failures, and a skeptical review has no unresolved correctness or scope question.

## 4. Handoff and external updates

Prepare a concise PR pitch through `pr-preflight` and `pr-pitch`, preserving unfinished findings
and provenance. Do not automatically assign, transition, comment on, resolve, commit, push,
or open a PR. Perform any such operation only after explicit task authority and runtime
discovery of the target repository/tracker. Never use broad staging, force-push, or automatic
durable-document deletion.

End with changed paths, decisions, issue disposition, checks done, skipped checks, and
limitations. Include commit/PR/ticket identifiers only when the authorized operation actually
occurred.
