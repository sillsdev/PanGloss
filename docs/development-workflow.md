# Development workflow: PR and Jira guidance

This document is the shared policy for the PR and Jira skills. It adapts durable review
practice from FieldWorks to PanGloss; it is not a FieldWorks workflow and does not authorize
external writes.

Read `CLAUDE.md` and `docs/code-review-standards.md` as the governing repository instructions.
These workflow skills supplement them; they do not replace managed verification or linear-main rules.

## PanGloss correctness contract

The C# HermitCrab implementation is the founding oracle. Rust is a port under test. For
parse claims, compare complete parse-identity multisets and row statuses, not counts alone.
Use positive, negative, boundary, order-sensitive, and memoization-sensitive controls when
the change can affect those dimensions. A timeout, cap, skip, missing row, or error is
incomplete evidence, never an expected rejection or parity pass.

The oracle and the Machine conformance grammars are the expected-identity authority, subject
to the documented independently justified divergence rules. Read `.claude/skills/oracle-alignment`
before changing parser semantics, porting an optimization, auditing the divergence ledger, or
reporting a shared correctness concern. Read `.claude/skills/conformance-grammars` before
changing a conformance fixture. A fixture's presence, an engine run, a grammar mutant, and a
fix-removed regression are different kinds of evidence.

Review risks explicitly include:

- completeness and soundness: valid analyses must not disappear and invalid analyses must not
  be admitted;
- correlation and identity: per-feature or per-path evidence must not be mistaken for a
  joint parse-identity result;
- oracle evidence: record the C# and Rust revisions, exact grammar/word paths, and whether
  the observation was run, derived, or unavailable;
- resource bounds: caps and timeouts contain work but make the result incomplete; they do not
  certify correctness or production readiness.

A concern shared with Machine needs a matching issue, exact evidence and provenance, and a
reference from the relevant divergence-ledger entry. Label it as reproduced, suspected, a
coverage gap, or an efficiency difference. Do not close that issue for a partial slice.

Separate a normative contract from an implementation detail. The implementation is evidence
to inspect, not an automatic replacement for a documented contract; a disagreement must be
made explicit and resolved with the relevant ADR, ledger entry, test, or upstream report.
Do not accept a reviewer claim merely because it sounds plausible, and do not assume the code
is right merely because it is current.

## Discovery and authority

Resolve the repository and integration surface at runtime:

1. Inspect `git remote -v`, the current branch, configured upstreams, and available remote
   base branches. Pin the chosen base, `HEAD`, and the exact diff in review notes. A default
   such as `origin/main` is usable only after it is verified to exist.
2. Discover the issue tracker, project/key, tenant, schema, and installed read-only tooling
   from the environment or supplied task context. Do not hardcode a Jira tenant, project,
   Data Center/cloud field shape, account-field name, or local client path. If no safe
   read-only integration is available, use ticket details pasted by the user.
3. Use only approved, least-privilege tooling whose issue visibility is authorized for agent use.
   Do not browse private Jira URLs directly or substitute personal/admin tokens. Automated
   integrations use approved service credentials in managed secrets and HTTPS, never credentials
   in prompts, logs, or repository files. If approved access is unavailable, request agent-shareable
   pasted details; do not configure a new integration merely to read a ticket.

Separate `git diff --cached`, `git diff`, and the committed base-to-head diff. Record included and
excluded paths and the precise review scope; HEAD alone does not identify uncommitted content.
Fetch the selected base when available, or label it potentially stale. Refresh evidence after
scope/head changes, retaining previously resolved findings. Verify `.review/` is ignored before use.

Commits, pushes, PR creation or editing, Jira assignment/transition/commenting, and PR thread
resolution are side effects. Perform them only when the task explicitly authorizes that
operation. An explicit commit-to-main request is a supported workflow; otherwise normal PR
work uses an isolated feature branch. Never use broad staging, discard or stash user files,
force-push, or delete durable documentation automatically.

Existing task authorization counts; do not ask again for an already approved action. Named staging
alone does not exclude unrelated files already staged: use a clean isolated worktree or an explicitly
scoped commit, and verify the complete commit path list. Never clear someone else's index.
Integrate onto main with the repository's linear-history workflow, preserving unrelated working files.
Before an authorized merge, inspect actual checks, review requirements, and unresolved findings.
Do not copy FieldWorks release branches or merge settings.

Preserve verified Jira keys exactly in development links; local plan IDs are not Jira keys by default.
Link issues with neutral references for partial fixes, not automatic closing keywords. For concerns
shared with Machine, follow oracle-alignment: search/reuse its actual report, link fixture and ledger
evidence, and distinguish a posted comment from a fix PR. Keep the issue open while acceptance gaps remain.

Use `rust/tools/pg.ps1` for Rust checks and runs. Skills must not teach bare Cargo commands.
For documentation-only changes, use markdown/editor diagnostics, link and path checks, and
whitespace checks as available; do not claim parser, conformance, or build validation. For
changed Rust or fixture behavior, select the narrowest appropriate managed `pg.ps1` gate first
and run the authoritative broader gate required by the risk. Report skipped checks and why.

## Durable records and provenance

Preserve ADRs, design decisions, the divergence ledger, conformance evidence, previously
recorded fixes, and unfinished findings. A working note may be removed only when the exact
scope is approved and its durable conclusion and provenance have first been safely published
in the PR description or a repository document. A PR pitch may summarize evidence in collapsed
sections, but it must not evict durable records or leave a dangling reference.

The source adapted here is immutable and auditable:

| Source | Revision | Paths read |
| --- | --- | --- |
| `sillsdev/FieldWorks` (`https://github.com/sillsdev/FieldWorks`) | `caeeeb148517ac5aa77de7ce0d21f7db1cc10c47` | `.claude/skills/pr-preflight/SKILL.md`; `.claude/skills/pr-pitch/SKILL.md`; `.claude/skills/respond-to-review-comments/SKILL.md`; `.claude/skills/jira-bugfix/SKILL.md`; `.github/instructions/review-analyzer.instructions.md` |

The source was read with `git -c safe.directory=... -C ... show <sha>:<path>`. The adaptation
keeps outcome-first PR writing, verified severity/risk review, thread-specific response
classification, and red/green TDD with negative and boundary controls. It removes FieldWorks
eviction requirements, COM/.NET Framework/WiX assumptions, ticket-prefix/Jira tenant assumptions, and
FieldWorks source-path assumptions. PanGloss-specific rules above supply the replacement
oracle, conformance, identity, and resource-bound evidence model.

Additional source material reviewed at the same revision: `Docs/workflows/ai-pr-workflow.md`,
`Docs/workflows/pull-request-workflow.md`, `.github/copilot-jira-setup.md`, and
`.github/pull_request_template.md`. Their scoped issue intake, agent-safe access, review lifecycle,
and concise PR-template advice are adapted here; FieldWorks integration secrets and release mechanics
are deliberately not installed.

This documentation adaptation itself adds no parser fix, fixture, or divergence-ledger entry.

## Workflow ownership

- `pr-preflight` establishes the branch/base/head/diff, verifies findings against the tree,
  records severity and evidence, and identifies required checks before a pitch.
- `pr-pitch` writes a concise outcome-first body with collapsed evidence, decisions, known
  gaps, and provenance. It does not silently delete working or durable records.
- `respond-to-review-comments` evaluates each thread as Fix, Clarify, Reply-only, or Defer;
  it changes code only for verified, unambiguous fixes and preserves unresolved findings.
- `jira-bugfix` provides issue intake and a red/green implementation loop. Tracker mutation,
  branch mutation, commit, push, PR, and ticket updates remain separately authorized.

Every workflow ends with changed paths, decisions, checks actually performed, skipped checks,
and limitations. No finite suite justifies a universal correctness claim.
