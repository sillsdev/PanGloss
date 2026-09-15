---
name: pr-pitch
description: Use after PR preflight, or to revise an existing PanGloss PR description, when a concise outcome-first pitch with collapsed evidence and provenance is needed.
argument-hint: Optional PR number, URL, or branch purpose
---

# PR pitch

This is a write-up skill, not the entrypoint for a fresh PR request. Use `pr-preflight` first
unless an existing PR description is being revised. Read `docs/development-workflow.md` and
the pinned preflight evidence before composing anything.

## Triage the record

List changed Markdown and documentation paths against the verified base. Classify each as
durable, research, not-taken, process, or stale. Durable ADRs, design decisions, ledger
entries, conformance evidence, and unfinished findings stay in the repository. A working
record may leave the tree only after exact scope is approved and its durable conclusion and
provenance are published in the body. Never silently delete files, comments, or issue history.

Verify names, paths, counts, contracts, and claims against the current tree. Where a normative
contract and implementation differ, name the disagreement and evidence; do not soften either
side into an uncheckable statement.

Completion criterion: every changed documentation artifact has a recorded destination and no
durable reasoning or provenance is scheduled for unapproved removal.

## Compose the body

The uncollapsed pitch is at most about 400 words, shorter for small changes, and answers, in order:

1. what changed and what users/callers can now rely on;
2. the reviewer’s main unknown and the risk worth reviewing;
3. at most five risk-and-pin bullets naming the relevant gate, test, invariant, or evidence;
4. deliberate non-goals, parity gaps, deferred work, and unfinished findings;
5. stack/base information and verification status, including what was not run.

Lead with the outcome. Keep detailed proof, decisions, alternatives, reversals, surprising
findings, limits of authorization, and paths not taken below a horizontal rule in closed
`<details>` sections. Synthesize; do not paste research. Include the pinned base/head, source
revisions, exact fixture or report paths, and whether evidence is run, derived, unavailable,
or incomplete. State full parse-identity comparison rather than a count when parsing is at risk.

Completion criterion: the pitch is within the word budget, every `<details>` is closed, the
body is within the hosting limit, and every named claim resolves to current evidence.

## Apply only with authority

Write the body to a local file before publishing. Publishing or editing a PR requires explicit
task authority and the verified runtime repository; do not hardcode an owner, remote, or PR
number. Do not evict durable docs or create a commit as a side effect of writing the pitch.
Any removal of working notes requires the approved scope and a final check for dangling links.

When editing an existing PR body, preserve useful author-written content and replace stale evidence
instead of appending duplicate summaries. Partial fixes use neutral issue links, not closing keywords.

End with changed paths, decisions, checks done, skipped checks, and limitations. A documented
finite check is not a claim of universal correctness or production readiness.
