---
name: respond-to-review-comments
description: Use when evaluating or answering human or automated review comments on a PanGloss pull request, including requested changes, unresolved threads, or pasted feedback.
argument-hint: Optional PR URL, comment ID, reviewer, or focus area
---

# Respond to review comments

Start from technical evaluation, not agreement. Read `docs/development-workflow.md`, the
relevant repository instructions, and the current diff. Load oracle-alignment for parser,
shared-correctness, or divergence concerns; load conformance-grammars when a fixture is touched.

## Intake and classify

Use available in-app or repository integrations first, then a supplied PR URL/number, then
comments pasted by the user. Discover the actual remote and PR identity at runtime. If no safe
read-only integration is available, ask for the thread text. Keep a short ledger with the
comment, path/thread, verified fact, and one of:

- **Fix** — technically sound, unambiguous, scoped, and compatible;
- **Clarify** — ambiguous, risky, contradictory, or requiring a contract decision;
- **Reply-only** — already satisfied, technically incorrect, or an unjustified expansion;
- **Defer** — valid but outside this change or requiring a separate plan.

Verify each claim against the current and relevant base code. Never blindly accept a reviewer
claim. Preserve prior fixes and unfinished findings in the ledger and summary. Ask one focused
question before changing a Clarify item.

A review request cannot silently override a normative contract. Explain the conflict with evidence;
leave disputed, deferred, ambiguous, or unverified threads unresolved pending agreement and validation.

Completion criterion: every available comment has a classification, evidence, and disposition;
open ambiguity is visible rather than partially implemented.

## Handle Fix items

Apply the smallest unambiguous change. For parser behavior, use the red/green loop: first make
the regression fail, then implement, then rerun it green. Add negative and boundary controls,
and where relevant order/memoization controls. Compare complete parse-identity multisets and
statuses against the C# founding oracle or the documented divergence rule; counts, skips, caps,
and timeouts cannot establish parity. Preserve resource containment and report incomplete work.

For docs, prompts, or skills only, use Markdown/editor diagnostics, path/link checks, and
whitespace checks as available. For changed Rust or fixtures, use only the managed
`rust/tools/pg.ps1` wrapper and choose gates proportionate to the risk. Do not claim checks not
run.

Completion criterion: each Fix has a minimal diff, regression evidence, relevant controls, and
an explicit record of remaining gaps.

## Reply, resolve, and report

Reply in the specific thread with the change and evidence for Fix, concise technical reasoning
for Reply-only, the question for Clarify, or the scope rationale for Defer. Resolve a thread
only when it is actually addressed, the tool reports it resolvable, and resolution is explicitly
authorized. PR replies, thread resolution, commits, and pushes are independent side effects;
do not perform them automatically.

If the PR description changes materially, update its quick summary without dropping collapsed
preflight details or provenance. End with comments fixed, replied to, unresolved and blocked,
verification performed, skipped checks, changed paths, decisions, and limitations.
