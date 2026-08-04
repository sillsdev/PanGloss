---
name: code-comments
description: >-
  Use when writing, reviewing, or cleaning up comments and doc comments in this repo's source
  code — module headers, `//!`/`///` docs, inline `//` notes, and commit-adjacent prose that has
  leaked into code. Enforces minimal comments that explain WHY, and forbids project state in
  source: no plan/spec/task/openspec references, no step-numbers, no dates, no wiring status, no
  changelog narrative. Trigger on "add a comment", "document this module", "fix the doc comment",
  "why is this commented", any doc/code mismatch cleanup, and whenever a plan reference is
  encountered in code — remove it on sight.
---

# Comments and doc comments

## The one rule

**A comment explains what the code cannot: why this, why not the obvious alternative, what breaks
if you change it. Everything else is noise or a lie waiting to happen.**

Code says what it does. Git says when and by whom. The plan says where the project is. A comment
that duplicates any of those three has no reason to exist and will eventually contradict its
source.

## Forbidden in source code, without exception

Delete these on sight, in any file you touch, whether or not you are otherwise working on it.
Do not "correct" them — remove them.

| Forbidden | Why | Instead |
|---|---|---|
| Plan / spec / task references — `openspec/changes/...`, `tasks.md §4`, `design.md D3`, `Step 1 of N`, `Phase B`, `P6`, `task 7.13` | A pointer into memory you do not own. Plans get archived, renumbered, superseded; the comment survives and misleads | State the constraint itself. If the reason is genuinely external, cite a **stable** source (a paper, an RFC, a standard), never a project artifact |
| Wiring/reachability status — "purely additive", "not wired into X yet", "reachable from no path", "a later change will…" | True the day it is written, false the day the next change lands, and nothing checks it. This is the single largest doc-rot source in this repo | Say what the module OWNS. Reachability is a fact about the call graph — let the reader grep, or let a test assert it |
| Dates — "measured 2026-07-30", "corrected 2026-08-04", "as of today" | Git has the date and is never wrong about it | Nothing. If a measurement matters, put the number in a test or an evidence doc |
| History / changelog narrative — "this used to read…", "previously we…", "renamed from…", "corrected in place" | Describes code that no longer exists. Readers cannot tell the live claim from the dead one | Nothing. `git log -p` and `git blame` answer this better |
| Ticket / commit / PR IDs | Same failure as plan references, plus they outlive the tracker | Nothing |
| Attribution and process notes — "added by the 7.8 agent", "two agents split this so…", "the reviewer asked for…" | Process shape leaking into code shape. This repo has a measured case: ~600 lines duplicated across three test files because two agents were forbidden to edit a shared file | Nothing. Fix the duplication instead |
| Restating the code — `// increment i`, `/// Returns the name` on `fn name()` | Pure maintenance liability | Nothing. Rename the thing if it is unclear |
| Commented-out code | Dead weight; nobody dares delete it | Delete it. Git has it |

## What a good comment looks like here

Keep these. Write more of these.

- **A non-obvious invariant.** *"The FST over-approximates by construction; confirm prunes. Never
  add a filter here that could reject a true analysis."*
- **A rejected alternative and the reason.** *"Matched by char-def identity rather than literal
  spelling: a char-def with several representations matches any of its own spellings."*
- **A trap.** *"`apply_up` cost lives in abandoned branches, so a path count is a floor, not a
  bound."*
- **A safety or ordering requirement.** *"Must run before the daemon starts; priority is inherited
  at spawn."*
- **A citation to something stable.** A paper, an RFC, a standard, an upstream issue.

## Module headers (`//!`)

One short paragraph: **what this module owns**, in the present tense, third person. Then the
non-obvious constraints a caller needs.

A module header is not a status board. It has no shelf life, so nothing with a shelf life belongs
in it. Per the Rust API guidelines' own C-HIDDEN, docs exclude implementation detail irrelevant to
the reader; project state is the extreme case of that.

```rust
//! Compiles rewrite rules into replace-calculus regexes.
//!
//! Matches by char-def identity, not literal spelling — a char-def with several representations
//! matches any of them, so callers must not pre-expand spellings before calling in.
```

Not:

```rust
//! P6 feasibility prototype (docs/fst-plan/foma-fst-plan.md §P6 item 1). Step 1 of
//! `openspec/changes/reify-compilation-plans`. Purely additive — NOT wired into the mainline
//! path yet; a later change (design.md D4) will flip a real seam to consult it.
```

Every clause in the second example was true when written. All of it is false now, and it made a
2,700-line production module read as an abandoned experiment.

## Length

Minimal. If a comment is longer than the code it describes, the reasoning belongs in a design doc
and the comment should be one line pointing at the idea, not the plan.

A long comment is usually one of three things wearing a disguise: a design doc, a changelog, or an
apology for code that should be clearer. Only the first is worth keeping, and it should not be
here.

## When you find a violation

Remove it in the same commit as whatever you were already doing. Do not open a task, do not
annotate it, do not leave a marker. These are cheap to delete and expensive to schedule.

The one exception: if deleting a claim would lose a **genuine invariant** tangled up with the plan
reference, keep the invariant and drop the reference.

Before: `// Task 7.13: this matches the candidate's own LoweringAdapter -- the same value
ExecutableCandidate seals -- instead of a second enum kept in correspondence by hand.`

After: `// Keyed on the candidate's own adapter: a parallel enum would have to be kept in
correspondence by hand.`

## Why this rule is strict rather than advisory

Doc rot compounds — a few percent drift per change becomes a majority mismatch within a dozen. And
the failure is asymmetric: a missing comment costs one reader one minute, while a confidently wrong
one sends them to the wrong conclusion and is *believed*, because it looks maintained. Once a
reader is burned twice they stop trusting every comment in the tree, including the good ones.

That has already happened here. Four of this crate's most central modules described themselves as
unwired prototypes while being the capability gate, the rule compiler, the plan substrate, and the
health schema.

## Sources

- [Rust API Guidelines — Documentation](https://rust-lang.github.io/api-guidelines/documentation.html) (C-HIDDEN, C-LINK, C-CRATE-DOC)
- [RFC 505 — API comment conventions](https://rust-lang.github.io/rfcs/0505-api-comment-conventions.html) and [RFC 1574](https://rust-lang.github.io/rfcs/1574-more-api-documentation-conventions.html)
- [No ticket numbers in comments](https://sveljko.github.io/no_ticket_numbers_in_comments/)
- [Documentation rot](https://devonair.ai/blog/pain-points/documentation-rot)
- [Code smell: obsolete comments](https://dev.to/mcsee/code-smell-183-obsolete-comments-3mmo)
