# Code review standards

What a reviewer — human or agent — checks in this repo, beyond "does it work". `/code-review` reads
this file as the documented standard.

Each rule below exists because it failed here, not because it is generally good advice. The measured
cost is stated so a future reader can judge whether the rule still earns its place.

## 1. Comments

**Authoritative rules: `.claude/skills/code-comments/SKILL.md`. Enforced by
`rust/tools/comment-hygiene.ps1` and `pg.ps1 -Mode doc`.** The review-time summary:

| Check | Standard |
|---|---|
| Implementation comment (`//`, or `///` on a private item) | **one line** |
| API docstring (`///` / `//!` on a `pub` / `pub(crate)` item) | long form as appropriate |
| Code-to-code doc links (`` [`Foo`] ``) | **banned** — plain backticks; the LSP navigates |
| Links to research (`docs/research`, papers, upstream issues) | keep; they are checked |
| Project state — plans, dates, wiring status, history prose | **banned** |
| A claim about another entity's behaviour | cite the pinning test, or reword |

The kind distinction is the standard one, not a local invention:
[Ousterhout](https://web.stanford.edu/~ouster/cgi-bin/cs190-spring16/lecture.php?topic=comments)
separates interface documentation ("what someone needs to know to use it") from implementation
documentation ("how it works internally"), and rules that the second must not appear in the first;
[Java's conventions](https://www.oracle.com/java/technologies/javase/codeconventions-comments.html)
draw the same line syntactically.

**Why one line for implementation comments.** A three-line allowance produced 3,269 over-long blocks —
roughly 80 per module, which is not a plausible count of things a reader could get catastrophically
wrong. When an implementation comment wants a paragraph it has two honest destinations: lift it to the
API docstring if a caller needs it, or make it a test if it is a claim that could stop being true.
Prose in the body is the option that rots.

**Reviewer's question for any long API docstring:** does it tell a caller something the signature
cannot — a precondition, a trap, an invariant to preserve, a rejected alternative that looks better
than it is? If it narrates the body, it is implementation documentation in the wrong place.

## 2. Evidence for claims

- **A comment asserting another entity's behaviour must cite the test that pins it** —
  ``pinned by `<test_name>` ``, and the name is machine-checked against the tree. A citation to a test
  that does not exist has happened three times here, each asserting the *opposite* of what the live
  test asserts.
- **A gate must be falsified, not asserted.** Show it failing when the thing it guards is broken.
  Measured cost of skipping this: a regression pin that named `FailClosed` + `RefusalWitness` while
  asserting `ConfigPredicate` + `Dedicated`, and fed itself a fully-covered fixture set where its doc
  claimed an empty one — it would have passed with its own fix reverted.
- **"I could not look" must never read as "everything is fine."** An empty result set, a command that
  produced no output, an absent baseline: each must fail loudly rather than pass quietly. This has
  produced a false green three separate times, twice from a script that failed to parse and exited
  silently.

## 3. Deletion

- **Delete before polish.** Cleaning code that is about to be removed is wasted twice — once writing
  it, once reviewing it. Both happened in a single session here.
- **Prefer deleting dead code to keeping it.** Git has it. Dead-but-maintained code is worse than
  dead code: one task spent ~108 lines updating a module with no production consumer.
- **A symbol with zero production callers is a finding, not a curiosity.** Say so in review.

## 4. Scope

- Comment-only changes must be **verifiably** comment-only: zero non-comment lines in the diff.
- A change that alters a serialized schema, a golden file, or a CLI contract needs its version bumped
  or an explicit note that it was not.
- Structure and comment cleanup belong in the **same** pass over a module: they need the same context,
  and an over-long comment is usually the signal for the structural fix.

## 5. What not to flag

Reviews here have wasted time on these; they are deliberate:

- `parse_word_core`'s traced/untraced duplication — one body parameterized by a sink so the two paths
  cannot drift.
- `Enter-BuildSlot`'s N mutexes rather than a counted semaphore — the kernel releases a mutex when its
  holder dies; a semaphore leaks its count, and did, deadlocking every worktree.
- Machine-proportional thresholds in `rust/tools/` rather than flat gigabyte figures.
- Long API docstrings on public items that genuinely carry the contract. The cap is on implementation
  comments, not on interfaces.
