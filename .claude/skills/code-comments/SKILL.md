---
name: code-comments
description: >-
  Use when writing, reviewing, or cleaning up comments and doc comments in this repo's source
  code — module headers, `//!`/`///` docs, inline `//` notes, and commit-adjacent prose that has
  leaked into code. Enforces minimal comments that explain WHY, and forbids project state in
  source: no plan/spec/task/openspec references, no step-numbers, no dates, no wiring status, no
  changelog narrative. Also caps comment blocks at three lines unless they carry a machine-checked
  anchor (intra-doc link, doctest, or an existing `docs/research/` path), and forbids bare
  behavioral claims about other code entities ("X refuses Y", "the only caller is Z",
  "unreachable") — those become a link or a test. Trigger on "add a comment", "document this
  module", "fix the doc comment", "why is this commented", "this comment is too long", any
  doc/code mismatch cleanup, and whenever a plan reference is encountered in code — remove it on
  sight.
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

## Generalise the incident to the class

The most common thing worth SAVING is buried in the most common thing worth deleting: a comment
that justifies a guard by retelling the incident that produced it. Keep the mechanism, drop the
war story.

An incident is a date, a machine, a process name, a number measured once. The class is what the
guard actually defends against, and it is the only part still true next year on a different box.

| Incident-shaped (delete the narrative) | Class-shaped (keep) |
|---|---|
| "Measured 2026-07-30: `predict_census.exe` climbed to 118GB over ~45 minutes and took the machine to zero." | "Bounds committed memory; an unbounded run can otherwise exhaust the machine." |
| "Got wrong seven times in one session — five file names passed to `-Filter`, plus two subagents that concluded the flag was unreachable." | "`-Filter` matches test names; `-TestTarget` selects the binary. Only the second reduces build time." |
| "The semaphore deadlocked every worktree on 2026-07-31; recoverable only by hand-releasing it until it threw." | "A mutex is used because the kernel releases it when a holder dies; a counted semaphore leaks its count." |
| "Sena's 8 non-multi-app CompoundingRules turned 7 null-shaped allomorphs into 56 self-looping lexc lines." | "Null-shaped allomorphs can form epsilon cycles when unrolled per level." |

The test: **would a reader on a different machine, a year from now, act differently knowing the
date and the process name?** If no, it is decoration on a real reason — keep the reason.

Two things this does not license. Do not generalise away a *number the code depends on* — a
threshold's actual value and units stay. And if the incident is genuinely the only evidence for a
surprising claim, put the evidence in a test or an evidence doc and let the comment state the
claim; a measurement that only exists inside a comment is unverifiable anyway.

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

## Sort every claim by what a machine can falsify

This is the rule that catches what the forbidden-list above cannot. The forbidden list finds project
state. It does not find a comment that is simply **false about behavior** — and that is the class
that cost this repo eight days of a capability widening nobody could see, because six comments said a
function stayed on one lowering scope while the code passed another. None of the six was a date, a
plan reference, a step marker, a wiring-status phrase, or history prose.

**Length is not why such a comment rots.** Compress the worst of those six to one line —
`// slot_candidates refuses Anchor anyway.` — and it is exactly as false. What rots is an
**unverifiable claim about another code entity**. So classify by falsifiability, and prefer the tier
above wherever you can reach it:

| Tier | Looks like | What to do | Who checks it |
|---|---|---|---|
| **Executable** | "this composes to `ConfirmOnly`", "`qp` becomes `pq` at a final boundary" | a doctest, or **`pinned by `<test_name>``** | `cargo test`; the citation's *name* is verified against every item in the tree by `comment-hygiene.ps1` |
| **Linked** | any claim about a *different* entity: "X refuses Y", "the only caller is Z", "unreachable in practice" | write the entity as an intra-doc link — ``[`crate::lower::lower_span`]`` | `rustdoc::broken_intra_doc_links` via `pg.ps1 -Mode doc` |
| **Durable external** | a paper, an algorithm name, a DTD element, an upstream issue, foma's `.#.` semantics | keep it, one to three lines | nothing needed — it does not rot |
| **Project state** | plans, dates, wiring status, history | delete | the hygiene ratchet |

**Two hard coverage limits, because a gate you misjudge is worse than one you know is partial.**

1. **In `tests/*.rs`, a link anchor is checked by nobody.** `cargo doc`'s target selection is
   lib/bins/examples only — there is no `--tests`, and `--all-targets` is rejected. So inside a test
   file, prefer **`pinned by `<test_name>``**: `comment-hygiene.ps1` verifies citation names against
   every item in the tree regardless of target kind, so that anchor is checked everywhere the link
   anchor is not.
2. **You cannot link *through* a private module from outside its parent.** `--document-private-items`
   makes private items *documented*, not *nameable*. `crate::compile::environment::validate_environment`
   is unresolvable from `crate::segment` because `mod environment` is private to `compile`; the same
   name links fine from inside `compile` as `super::environment::…`. Rust visibility (private = the
   defining module **and its descendants**) governs link resolution too. From outside, use a code span.

Know the limit of tier 2 rather than trusting it: a link proves the **path resolves**, not that the
sentence about it is true. Renames and deletions are caught; semantic drift is not. That is exactly
how the metathesis comments survived — every identifier in them still existed. **Only a test closes
that gap**, which is why tier 1 does the real work and tier 2 is a floor, not a ceiling.

The practical consequence, and it is the most useful sentence in this file: **when a comment's
argument is load-bearing, the argument belongs in a test and the comment belongs in one line.** The
metathesis guard's *stated reason* was false while the guard itself was fine. Being right for a wrong
reason is what no gate catches, and a paragraph of prose reasoning is how you get there.

## Length: three lines, and longer costs you an anchor

**A comment block of three lines or fewer needs no justification. Over three lines, it must carry an
anchor a machine can check** — an intra-doc link, a ``` doctest, or a path under `docs/research/`
that exists. `rust\tools\comment-hygiene.ps1` enforces this as `comment-block-too-long`.

Three, not one, and the reason is specific: **one claim plus the falsifier that keeps it honest.**
One line holds a claim and nothing else, and a claim with no named falsifier is precisely what went
stale. Three lines hold both:

```rust
// MUST stay in lockstep with `capability::metathesis_swap_construction_attempted` -- a
// disagreement admits a different rule set than this compiles. Admits a word-edge `Anchor`;
// `metathesis_anchor_pattern_compiles_as_confirm_only_swap_superset` pins the result.
```

That replaced seven lines whose safety argument was false. It is shorter *and* it names the thing
that would fail if someone moved the scope back.

**Why an anchor rather than a marker.** A marker you can type becomes universal and then means
nothing — this repo's documented failure mode for any gate that taxes ordinary work. An anchor cannot
be applied thoughtlessly because it has to name something real. And so the escape hatch cannot
quietly become the norm, the checker reports `long-blocks-anchored` as a separate, ungated number: if
it climbs while `comment-block-too-long` falls, the rule is being satisfied rather than followed.

**`docs/` is for research, not for behavior — and this is a hard line.** Moving a long internal
explanation into `docs/` and linking it *relocates the rot and removes the only checker*: a page
describing what `slot_candidates` refuses goes stale exactly as the comment did, and nothing compiles
it. So an anchor under `docs/research/` is for durable external knowledge — a paper, an algorithm,
upstream behavior — and an internal behavioral claim must be a test, never a doc page. The checker
enforces the directory, and flags any `docs/…md` path in a comment that does not resolve
(`docs-link-broken`, currently 0 — every `docs/` pointer in the tree is live).

A caution earned by the checker's own first run, which is worth more than the count it produced. It
reported nine broken links; **all nine were the checker's bug** — the pages live under `rust/docs/`
and comments cite them relative to the crate root, so resolving only from the repo root called four
live files missing. The near-miss was writing "nine broken links" into this file as a measured fact.
So: a *link* check is only as trustworthy as its notion of where the target could be, and the failure
direction matters — "I looked in the wrong place" reads as "the file is broken" exactly as easily as
"I could not look" reads as "everything is fine."

## When you find a violation

Remove it in the same commit as whatever you were already doing. Do not open a task, do not
annotate it, do not leave a marker. These are cheap to delete and expensive to schedule.

The one exception: if deleting a claim would lose a **genuine invariant** tangled up with the plan
reference, keep the invariant and drop the reference.

Before: `// Task 7.13: this matches the candidate's own LoweringAdapter -- the same value
ExecutableCandidate seals -- instead of a second enum kept in correspondence by hand.`

After: `// Keyed on the candidate's own adapter: a parallel enum would have to be kept in
correspondence by hand.`

## The counterweight — read this before deleting an interface comment

This file is one-sided on purpose, and applied without judgment it will damage the codebase. The
strongest argument against it is Ousterhout's, and it is correct: **without an interface comment there
is no abstraction.** The reader must open the body, so all of its complexity is exposed. "Let the
reader read the function" is a real cost, not a free win.

Two things follow, and they bound every rule above.

**Reading a function tells you what it does. It never tells you what it must never do.** Negative
constraints — *refuses interior anchors*, *must stay in lockstep with the predicate*, *never add a
filter that could reject a true analysis* — have no representation in Rust except a test. Deleting the
comment does not make the constraint discoverable; it makes it invisible. Convert it (tier 1) or keep
it (three lines). Never just drop it.

**So the target is claims, not words.** A four-line struct doc explaining what a field means, with no
assertion about another entity in it, is doing its job — anchor it or tighten it, do not gut it. The
`comment-block-too-long` count is a *ratchet*, not a mandate to reach zero: unlike the project-state
categories, a nonzero number here is not automatically a defect. The two categories worth driving to
zero are `cross-reference-claim` and `docs-link-broken`, because those are pure defect.

## Why this rule is strict rather than advisory

Doc rot compounds — a few percent drift per change becomes a majority mismatch within a dozen. And
the failure is asymmetric: a missing comment costs one reader one minute, while a confidently wrong
one sends them to the wrong conclusion and is *believed*, because it looks maintained. Once a
reader is burned twice they stop trusting every comment in the tree, including the good ones.

That has already happened here. Four of this crate's most central modules described themselves as
unwired prototypes while being the capability gate, the rule compiler, the plan substrate, and the
health schema.

There is now measured evidence for a second reader this repo did not use to have. **Misleading natural
language in code degrades LLM code reasoning by roughly 23% on average, and reasoning models show a
"reasoning collapse" failure mode** ([CodeCrash](https://arxiv.org/pdf/2504.14119)): models trust the
prose over the executable logic. Separately, across 1.3 billion AST-level changes in 1,500 systems,
**changes that leave a comment inconsistent are about 1.5x more likely to be bug-introducing**
([Wen et al.](https://www.inf.usi.ch/lanza/Downloads/Wen2019a.pdf)). A confidently wrong comment is
not untidy; it is a measurable defect for both kinds of reader.

## Do not rebuild what already exists

Prefer the mechanism whose validation someone else maintains:

| Want | Use | Status here |
|---|---|---|
| A comment that cannot silently go stale | a **doctest** — `cargo test` executes it | available, underused |
| Validate an entity reference resolves | `#![deny(rustdoc::broken_intra_doc_links)]` | **not enabled** — the tier-2 anchor is unchecked until it is |
| Long prose in markdown, still rendered as docs | `#[doc = include_str!("…")]` (note: error locations report against the `.rs` file) | unused |
| Validate external URLs | [lychee](https://lychee.cli.rs/) | unused |
| Validate `docs/…md` paths in comments | `comment-hygiene.ps1`'s `docs-link-broken` | built |
| Cap comment block length | `comment-hygiene.ps1`'s `comment-block-too-long` | built |

No mainstream linter caps comment *block* length — ESLint's `max-len` and `eslint-plugin-comment-length`
cap line **width**, and [the block-length request](https://github.com/eslint/eslint/issues/4665) has
sat open for years. So the block rules here are genuinely local invention; treat their thresholds as
this repo's judgment rather than industry consensus.

## Sources

- [CodeCrash: LLM fragility to misleading natural language in code](https://arxiv.org/pdf/2504.14119) — ~23% reasoning degradation; models trust prose over logic
- [Wen et al., A large-scale empirical study on code-comment inconsistencies](https://www.inf.usi.ch/lanza/Downloads/Wen2019a.pdf) — inconsistent changes ~1.5x more bug-introducing
- [Ousterhout, A Philosophy of Software Design](https://www.goodreads.com/work/quotes/61938796-a-philosophy-of-software-design) — the counterweight: without an interface comment there is no abstraction
- [Rustdoc lints](https://doc.rust-lang.org/rustdoc/lints.html) — `broken_intra_doc_links`, the check the tier-2 anchor depends on
- [Rust API Guidelines — Documentation](https://rust-lang.github.io/api-guidelines/documentation.html) (C-HIDDEN, C-LINK, C-CRATE-DOC)
- [RFC 505 — API comment conventions](https://rust-lang.github.io/rfcs/0505-api-comment-conventions.html) and [RFC 1574](https://rust-lang.github.io/rfcs/1574-more-api-documentation-conventions.html)
- [No ticket numbers in comments](https://sveljko.github.io/no_ticket_numbers_in_comments/)
- [Documentation rot](https://devonair.ai/blog/pain-points/documentation-rot)
- [Code smell: obsolete comments](https://dev.to/mcsee/code-smell-183-obsolete-comments-3mmo)
