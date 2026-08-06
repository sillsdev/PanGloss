# comment-hygiene.ps1: why each rule is shaped the way it is

Arguments moved out of `rust/tools/comment-hygiene.ps1`'s implementation comments so the source can
carry a one-line summary and a pointer. The rules themselves are stated in
`.claude/skills/code-comments/SKILL.md`; this file is the reasoning behind the *checker*, including
the several times it was wrong.

## Zero tolerance, not a ratchet

A ratchet fails only when a category grows. It was the right instrument against an inherited backlog
and the wrong one afterwards, for a specific reason: a baseline records the CURRENT count as
acceptable, so 4,330 violations printed as "passing". Re-baselining after a rule change quietly
relabels old debt as the new normal, which happened twice in one session.

Locally the checker is a WARNING — `pg.ps1` prints it on every managed build and never fails on it,
because a documentation finding that blocks every build is the gate shape this repo has already
watched get switched off and then protect nobody. In CI it is fatal: invoke it directly and honour
the exit code.

## Why the categories are scored separately

One total would let 50 new plan references hide behind 50 deleted dates. Each category is counted on
its own so one kind of cleanup cannot silently pay for another kind of regression.

## `step-marker`: case-sensitivity is deliberate

PowerShell's `-match` ignores case, and this codebase uses "stage 1"/"stage 2" for real algorithm
structure (propose, then confirm). A blanket match flags correct domain vocabulary and pressures a
cleanup pass into rewriting it. So `Phase`/`Stage` are case-SENSITIVE via `(?-i:...)` while
everything else stays case-insensitive — scoped that way rather than making the whole line
case-sensitive, because a capitalised task number must still be caught.

## `cross-reference-claim`: present tense, word-boundaried

The category catches a behavioural assertion about a NAMED entity other than the line below it. It
requires both a claim verb and a backticked identifier, so ordinary prose about the local statement
is not caught.

PRESENT-TENSE ACTIVE ONLY, and that is the whole precision of it. "`x` refuses `y`" is a claim about
what another entity does right now — the thing that silently stops being true. Past tense ("`load`
rejected the grammar") almost always documents what an error variant MEANS rather than a live
cross-reference, and including it measured ~25% false positives on this tree. `guaranteed` is out for
the same reason: "never guaranteed SMALL" is prose about a value, not a claim about a callee.

Word boundaries matter, and `unreachable` excludes the macro form. Substring matching made
identifiers collide with claims: `unreachable!()` and a local `const UNREACHABLE_KIND` both tripped
it, and two agents reworded correct technical vocabulary to satisfy the regex. That is the same
failure recorded above for `stage 1` — a gate that pressures a cleanup pass into damaging accurate
prose is worse than no gate.

## Citations, and the time the checker punished its own fix

A `pinned by <test>` citation is the strongest anchor, because a test is the only falsifier that
survives semantic drift. Two forms are recognised: the phrase form (`pinned by`, `pins`, `asserted
by`, `witnessed by`, `proved by`, `checked by` followed by a backticked name) and the
`path/to/file.rs::test_name` form this repo already uses for curated evidence citations. Only a
citation whose file is one of ours is judged; a name we cannot resolve in a file we do not own is not
evidence of anything.

Two hard-won details:

- **A citation wrapped across two comment lines is captured truncated**, and the break is not always
  at an underscore — `..._on_subrule` plus `_finding` on the next line reads as a complete name. So a
  captured name that is a PREFIX of a real one counts as live. That trades a little strictness to
  remove a whole false-positive class: a gate that cries wolf on correct citations gets ignored, and
  the real finding is ignored with it.
- **The checker once punished its own fix.** An earlier version counted a citation as a violation
  rather than as an anchor, so adding the very thing the rules asked for made the number go up.

## Anchors versus references

An anchor licenses a whole multi-line argument. A reference licenses only a SECOND line. They are
deliberately different sets.

Anchors: a ` ``` ` doctest (the only comment form that cannot silently go stale, because cargo
executes it), `include_str!`, a `docs/research/*.md` path that exists on disk, or a verified test
citation.

An intra-doc link is deliberately NOT an anchor any more. It only ever proved that a path resolved,
so it licensed length without licensing truth — and treating it as an anchor rewarded adding links at
exactly the time the repo concluded code-to-code links should be deleted (an LSP already navigates;
551 broken ones had gone unnoticed).

References are looser, because they only buy one extra line: any `docs/*.md` that resolves, or an
external URL. A URL cannot be validated offline, but a paper or an upstream issue is durable in the
way this rule cares about.

## Why two lines, and exactly two

The pointer gets its own line so the other can say WHY you would follow it. A bare
`see docs/research/<topic>.md` is close to useless — the reader cannot tell whether the detour is
worth it — and forcing summary and pointer onto one line yields neither. Three would reintroduce room
for an argument, and the argument is the thing that belongs in the document.

## Exception tags: one class survived the cull

`SAFETY:` is the only tag that buys extra lines. It marks an unsafe block's proof obligation, which
is a genuine obligation the language itself imposes and a reviewer must check.

Every other proposed tag was cut, because a marker you can type becomes universal and then means
nothing — this repo's documented failure mode for any gate that taxes ordinary work. Anchored long
blocks are counted and reported separately, never gated, so the escape hatch cannot quietly become
the norm without showing up as a number.

`PORT:` was considered on the ground that a divergence from the C# original is durable external
knowledge like a paper or an upstream issue. It was split into `port-divergence:` and
`port-correspondence:` precisely so an author must choose one and a reviewer can check which is true.

## API versus implementation, per language

The split is Ousterhout's interface-versus-implementation distinction, and Java's doc-versus-
implementation comments draw the same line. An interface comment may be as long as the contract
needs; capping it destroys the abstraction. An implementation comment is held to one line.

**Rust.** `//!` documents the module it sits in, so the module's own visibility decides. `///`
documents the NEXT item, and attributes and blank lines sit between the two, so the classifier skips
forward rather than reading one line. `pub(crate)` counts as API: it is a real interface for every
other module in the crate, and its callers are exactly as unable to see the body as an external
caller. But `pub` inside a PRIVATE module reaches nobody, so the item is effectively private and its
doc is implementation documentation — requiring both is what makes "is this an interface?" mean
reachability rather than spelling.

Two kinds of item inherit visibility from the enclosing declaration and can never carry the keyword
themselves: enum variants and trait items. Struct fields are NOT in that set — a field needs its own
`pub` — so the classifier walks OUT to the nearest enclosing declaration at a smaller indent and uses
its visibility. An earlier version guessed from the shape of the item's own line, which both missed
trait items and wrongly promoted every private field.

`tests/` and `examples/` are their own crate roots, but nobody consumes their docs; they are not
public interfaces and are classified accordingly.

**PowerShell.** Comment-based help is the analogue, and it requires TWO conditions: the block sits at
the top of a script or at a function's head, AND it carries a help keyword (`.SYNOPSIS`,
`.DESCRIPTION`, …). Position alone was the first version of the rule and it was wrong — measured
across this tree, 67 delimited blocks sat in a help position, 0 carried a keyword, and `Get-Help`
returned nothing for a single one of them. They were block comments wearing help's clothes. Position
alone is also a typeable marker: wrap any comment in a delimited block, put it at a function's head,
and the cap is gone.

**Python.** No keyword is required, and demanding one would import a PowerShell accident. A `"""`
block at module top or directly under a `def`/`class` is not a comment at all — the string is bound
to `__doc__` and `help()` prints it — so there, position really is the mechanism.

## Delimited bodies, shebangs, and directives

A delimited block's continuation lines carry no marker, so a line-start pattern reads them as code
and the whole body escapes every rule. Until this was fixed, 387 body lines — every script header in
the tree — were scored by nothing, hiding five dates and a plan reference.

Two things that merely look like comments are excluded, because neither can be shortened: PowerShell's
`#Requires` parser directive, and a first-line `#!` shebang. The shebang mattered concretely — glued
to the module docstring beneath it, it made one 36-line phantom block out of two unrelated things.

## Scope, and the checker exempting itself

The scan covers `rust/crates` (Rust), `rust/tools` (PowerShell) and `.claude/hooks` (Python). The
first version scanned only `rust/crates` and therefore missed every violation in the scripts that
enforce the rule — a checker exempt from its own check.

`$fnNames` is built from every `fn` in the tree so a citation can be checked rather than trusted, and
not only from `fn`: a curated citation legitimately names a fixture constant.

`ListLimit` defaults high enough to list a whole category. Truncating at 40 silently hid 56 of 96
hits while the summary reported the full number, which reads as "I have seen them all".

## Retired

`comment-block-too-long` applied one length to every comment in the tree regardless of whether it
documented an interface. It is replaced by the API/implementation split above.
