---
name: code-comments
description: >-
  Use when writing, reviewing, or cleaning up comments and doc comments in this repo's source
  code — module headers, `//!`/`///` docs, inline `//` notes, and commit-adjacent prose that has
  leaked into code. Enforces minimal comments that explain WHY, and forbids project state in
  source: no plan/spec/task/openspec references, no step-numbers, no dates, no wiring status, no
  changelog narrative. Caps IMPLEMENTATION comments (`//`, and `///` on private items) at ONE line
  while API docstrings on public items may run long form as appropriate, and forbids bare
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
| **Cited** | durable external knowledge: a paper, an algorithm, an upstream issue | a link to `docs/research/*.md` or a URL | `docs-link-broken`; the target does not move on its own |
| **Named** | any claim about a *different* entity: "X refuses Y", "the only caller is Z" | just backtick it — `` `crate::lower::lower_span` ``. **Do not add an intra-doc link.** | nothing, deliberately — see below |
| **Durable external** | a paper, an algorithm name, a DTD element, an upstream issue, foma's `.#.` semantics | keep it, one to three lines | nothing needed — it does not rot |
| **Project state** | plans, dates, wiring status, history | delete | the hygiene ratchet |

## Do not write code-to-code intra-doc links

**Default to a backtick, not `[`a link`]`.** This reverses an earlier version of this file, and the
reversal is evidence-driven: turning on `broken_intra_doc_links` for the first time found **551 broken
links**, and nobody had noticed any of them. A navigation aid that can rot that far unobserved was not
being navigated with.

The argument for links, stated fairly, because there is exactly one: rust-analyzer resolves intra-doc
links, so `[`Foo`]` is go-to-definition-able from a comment while `` `Foo` `` is inert text. That is
real. It is also worth roughly one keystroke over a workspace-symbol search, and it buys nothing at all
for the reader this repo optimizes for — an agent greps, and a link and a backtick are identical to it.
Against that: every link is a permanent coupling to another item's exact path, and it validates only
that the path resolves, never that the sentence is true. `[`slot_candidates`]` kept resolving
throughout the eight days its surrounding paragraph was false.

The case that WOULD justify links is a published crate whose readers browse rendered HTML on docs.rs
with no editor. That is not this workspace. Revisit this rule if any crate here is ever published.

So: **a link to research is worth keeping; a link to code is redundant with the LSP.** Delete the
brackets, keep the name in backticks. Deleting a link cannot break the build and permanently removes a
rot surface — which is why the correct response to a broken code link is almost always to remove it,
not to repair it.

Two corollaries:
- **Brackets inside plain `//` comments are already meaningless.** Rustdoc reads only `///` and `//!`,
  so `[`Foo`]` in an ordinary comment renders as literal brackets and is never checked. Pure noise.
- **A link no longer buys comment length** (see below). It never should have: rewarding links was an
  incentive to add exactly the thing that rots.

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

**The cap depends on which KIND of comment it is, and that distinction is standard, not local.**
[Ousterhout](https://web.stanford.edu/~ouster/cgi-bin/cs190-spring16/lecture.php?topic=comments)
separates *interface documentation* ("what someone needs to know to use the class or method") from
*implementation documentation* ("how the method works internally"), and his central rule is: **do not
describe the implementation in the interface documentation.** Java's conventions draw the same line
syntactically — [documentation comments](https://www.oracle.com/java/technologies/javase/codeconventions-comments.html)
"describe the specification of the code, from an implementation-free perspective", such that a reader
"should be able to use the class and its methods without having to read any source code", while
*implementation comments* merely "clarify how a particular piece of code operates".

Rust has the same split in syntax: `///` and `//!` are documentation comments; `//` is an
implementation comment.

| Kind | Cap | Why |
|---|---|---|
| **API docstring** — `///` / `//!` on a `pub` (or `pub(crate)`) item | **long form as appropriate** | This is the abstraction. Without it the caller must read the body, and there is no interface — Ousterhout's point, and the one place length genuinely pays. |
| **Implementation comment** — any `//`, and `///` on a private item | **one line** | It explains code the reader is already looking at. If one line cannot carry it, the knowledge belongs in the interface doc or in a test, not here. |

**One line. Not three.** The earlier three-line allowance was itself a compromise, and it produced
3,269 blocks — about 80 per module, which is not a plausible count of things you can get catastrophically
wrong. If an implementation comment wants a paragraph, that is a signal, and it has exactly two honest
destinations: **lift it to the API docstring** if a caller needs it, or **make it a test** if it is a
claim that could stop being true. Prose in the body is the option that rots.

**A reference document REPLACES a long comment; it does not license one.** If the knowledge needs a
paragraph, write it in `docs/research/` and let the comment be the single line that points there. That
is the whole answer for rejected alternatives, port divergences, algorithm derivations and measured
rationale — all of which used to be reasons to write twelve lines in the body.

**Exactly one class buys extra lines: `SAFETY:`** — an unsafe block's proof obligation, up to three
lines, and past that it needs an anchor like anything else.

It is the only one because it is the only obligation with **no external home**: an FFI precondition is a
proof about *this* call site (what the caller must guarantee about a pointer's validity and lifetime),
so there is nothing to point at. It is also Rust's own convention and what clippy's
`undocumented_unsafe_blocks` expects. Measured: 209 unsafe sites here, concentrated at the FFI boundary,
with 14 of 19 crates forbidding unsafe entirely.

**`TRAP:`, `INVARIANT:` and `WHY-NOT:` were tried and removed.** Each states something one line can
carry — *"apply_up cost lives in abandoned branches, so a path count is a floor, not a bound"* is a
complete trap in one line — and where it genuinely cannot, the argument is a document. Keeping them
would have offered three ready-made ways to buy length.

**`PORT-CORRESPONDENCE:` / `PORT-DIVERGENCE:` survive as review vocabulary, not as length licenses.**
Prefix a one-line comment with one when it makes a claim about the C# oracle. The split forces you to
say which, and that is what makes it checkable: a reviewer reads the cited C# and asks either "does ours
match?" or "is the stated difference real and intended?". A single vague tag lets a comment avoid
answering, and vague is the state in which a claim quietly stops being true.

**There is no accepted count.** The checker is zero-tolerance: every violation is reported and every one
is meant to go. It is a warning locally and fatal in CI. The ratchet it replaced was right against an
inherited backlog and wrong afterwards — a baseline records the current count as *acceptable*, so 4,330
violations printed as "passing", and re-baselining after a rule change quietly relabels old debt as the
new normal.

**Deciding whether a long API docstring earns its length:** it must tell a caller something they cannot
get from the signature — a precondition, a trap, an invariant they must preserve, or a rejected
alternative that looks better than it is. If it narrates what the body does, it is implementation
documentation in the wrong place; delete it or move it down to one line.

**An intra-doc link is NOT an anchor**, though it used to be. Two reasons it was removed: it only ever
proved a path resolved, so it licensed length without licensing truth; and making it an anchor created
an incentive to add links at exactly the moment we established that code links should be deleted. The
anchors that remain — a doctest, a ``pinned by `<test_name>`` citation, an existing `docs/research/`
path — all survive semantic drift, which a link never did.

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
| Validate an entity reference resolves | `#![deny(rustdoc::broken_intra_doc_links)]` | enabled; only `pg.ps1 -Mode doc` actually runs rustdoc, so that is what checks it |
| Long prose in markdown, still rendered as docs | `#[doc = include_str!("…")]` (note: error locations report against the `.rs` file) | unused |
| Validate external URLs | [lychee](https://lychee.cli.rs/) | unused |
| Validate `docs/…md` paths in comments | `comment-hygiene.ps1`'s `docs-link-broken` | built |
| Cap comment block length | `comment-hygiene.ps1`'s `comment-block-too-long` | built |
| Prove a comment sweep touched no code | `verify-comment-only.ps1` | built — see below |

## Verify that a comment-only edit was comment-only

**Run `rust\tools\verify-comment-only.ps1` after every file you edit, and honour the exit code.**
It diffs against HEAD and requires every line the change adds *and every line it removes* to be a
comment or blank.

The symmetry is the whole point. On 2026-08-06 one Edit removed 204 lines from
`orthogonal_basis_group_a.rs` and added 2: the first ~102 removed lines were the module doc it meant
to shorten, and the rest were the `use` block, two type definitions and four `const` items. The file
stopped parsing and took the entire pg-foma integration suite with it. The ad-hoc check in use asked
only "is everything you *wrote* a comment?" — which a pure deletion answers trivially and correctly.
Deleting code is invisible to that question.

The cause was an `old_string` whose end was anchored past the end of the comment block, so the Edit
matched further than intended. **Anchor the end of `old_string` on the last comment line, never on
the code that follows it** — and if a file does trip the verifier, `git checkout -- <file>` and redo
it in smaller pieces rather than patching the wreckage.

Two things it cannot do, so do not read a green result as more than it is. It is a diff-shape check,
not a semantic one: it cannot tell a good comment from a bad one (that is `comment-hygiene.ps1`), and
it cannot tell that a deleted comment *should* have been kept. What it does tell you, with no
compiler and in under a second, is that the code is untouched — which is the property a sweep is
actually claiming.

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
