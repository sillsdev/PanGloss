# Repo instructions

## What this repo is

PanGloss is a Rust reimplementation of HermitCrab, the morphological parser in
`sillsdev/machine`'s `SIL.Machine.Morphology.HermitCrab` (C#). Almost everything below follows from
that one fact. The C# checkout lives at `C:\Users\johnm\Documents\repos\machine`.

Shell commands go through the PowerShell tool, never Bash. Read/Write/Edit/Glob/Grep are not shells
and are fine.

## The oracle hierarchy — the rule with the widest blast radius

C# `hc.dll` is the **founding oracle**. Observed fixture expectations come from it;
a fixture may instead record independently justified forward-synthesis expectations, explicitly
marked and reported upstream, even when C# currently fails them. `pg_parse::Morpher` (HC-Rust)
is a **port under test**.

**HC-Rust must never produce a different parse than C#.** It may be faster or leaner; every such
efficiency needs an argument for why it cannot change a parse. If you believe the algorithm itself
should change, open or reuse a Machine issue with the evidence, then propose a fix PR when ready —
never keep a behavioural improvement unreported in Rust alone. Every divergence and every optimization gets an entry under
`docs/divergences/`.

The one escape hatch: an independently justified divergence documented in the ledger, pinned by a
fixture, and reported upstream. An issue is not a fix PR and neither alone proves correctness.

A fixture authored against HC-Rust instead of the oracle records HC-Rust's behaviour, not
correctness, and must say so in its `words.yaml` (`# oracle-provenance:`) — silence reads as
"verified against hc.dll" and is the bug. *Scar: HC-Rust once accepted `xpitz`/`muat`, which the
oracle rejects; only an oracle-diffed fixture caught it.*

Before changing analysis/synthesis semantics, porting optimizations, auditing the ledger, or reporting
shared correctness issues, load `.claude/skills/oracle-alignment/SKILL.md`.

## Rules with teeth

Each is enforced. The enforcement explains itself when it fires, so this is a table, not an essay.

| Rule | Enforced by |
|---|---|
| Bare `cargo build`, `cargo test`, `cargo check`, `cargo run` and `cargo nextest run` are PROHIBITED — use `rust/tools/pg.ps1` | `.claude/hooks/block-bare-cargo.py` |
| A managed build runs in the foreground, never `run_in_background` | `.claude/hooks/block-backgrounded-build.py` |
| Never scan from a filesystem root | `.claude/hooks/block-root-find.py` |
| `-Mode conformance-test` must claim `-Scope local\|all`; no default | exit 20, and `pg_conformance_fixtures::discover` panics on an unset `PANGLOSS_CONFORMANCE_SCOPE` |
| Divergence entry ids are unique and indexed | `pg-cli --test divergence_catalogue_gate` |
| Skills never instruct a command the hooks refuse | `pg-cli --test skills_never_instruct_bare_cargo` |
| Every repo path named in CLAUDE.md and the skills resolves on disk | `pg-cli --test agent_docs_resolve_gate` |

Each hook has a deliberate env-var escape hatch, named in its own refusal message. Needing one means
the managed path is broken and should be fixed, not routed around.

*Scars: bare Cargo once took this machine from 46GB to 7GB free with 26 stray compiler processes. An
orphaned root `find` burned 2110 CPU-seconds writing to a pipe whose reader had exited. Four agents
in one session backgrounded a build, waited on it, and reported "waiting for the background run to
finish" as their result — the last of them on a prompt that said "do not be the fourth".*

## Rules without teeth

Nothing enforces these. They are here because they change what a careful agent does.

- **Assume agents self-verify badly.** Re-run their gates with the fix reverted before believing any
  of it. *Scar: two agents shipped regression gates that passed with their own fix removed; one
  reported a feature implemented while its guard sat behind `if false &&`.*
- **Reap on report.** Kill stray `cargo`/`rustc`/`link`/`pangloss` when an agent finishes.
  `pg.ps1 -Mode gc` does it; nothing calls it for you.
- **Probe pathological grammars single-threaded.** `pangloss batch --threads 1` plus
  `--word-timeout-ms`. *Scar: one probe reached 30+GB RSS and never finished; the same work took ~2
  minutes single-threaded.*
- **Cap build-heavy agents at 2-3 concurrent.** `Enter-BuildSlot` caps *builds* at 2 machine-wide,
  but nothing caps agents; extras just queue and then exit 15.
- **A long command that is NOT a managed build should be a background job**, so the harness notifies
  on completion. *Scar: a 7,121-word corpus batch run in the foreground truncated silently at ~1,663
  words and was read as "the corpus".*
- **Conformance grammars use synthetic data only** — invented lexemes, never real-language data, and
  no language named outside a comment.

## Managed builds, in one place

`rust/tools/pg.ps1` (or `build.ps1`/`test.ps1`). Reach for `check` first, `test` last: the cost of a
round trip is compiling and linking ~105 integration targets in pg-foma, not running tests.

`-Mode check` type-checks everything including test code. `-Mode quick` adds unit tests.
`-Mode test` / `-Mode conformance-test` are authoritative — a green `quick` is not a green suite.
Also: `corpus-test` (refuses before Cargo if a declared corpus is missing), `release`, `doc` (the
only thing enforcing `broken_intra_doc_links`), `doctor`, `gc`, `run`, `new-worktree`.

`-Package`/`-TestTarget` narrow compilation; `-Filter` narrows execution only and still links
everything. `--no-fail-fast` is the default. *Scar: one trailing-newline mismatch once stopped 838
tests from running, and a ledger recorded that run's failure count as fact — wrong by 18x.*

**A feature-gated test is compiled out, not skipped, and reports as nothing at all.** Anything
touching a `developer-tools`-gated flag must be verified twice: rerun with
`$env:PANGLOSS_EXTRA_ARGS = '--features developer-tools'` and read the printed cargo line back.

Never run `git submodule update` by hand; `pg.ps1` initializes `machine/conformance` sparsely on its
own.

## Merging into main

Keep history linear. Rebase the branch onto current `main`, then `git merge --ff-only`. If it is not
a fast-forward, the rebase did not happen against the current tip; redo it. Never `--no-ff`. If a
rebase turns out to involve real conflicts rather than staleness, prefer re-running the change fresh
against `main`.

## Where to look

**Skills** (load by task, not by subsystem):
`oracle-alignment` — changing parse semantics, porting an upstream change, any C# divergence.
`conformance-grammars` — authoring, staging or graduating a fixture.
`fst-limits` — changing an FST threshold, refusal, retry or budget.
`module-seams` — adding a check another module already makes; building the measurement first.
`fix-a-grammar` — a slow, refused, oversized or incomplete grammar.
`dead-end-census` — the standing first lever for "language X is too slow".
`code-comments` — comment and doc-comment policy.

**Design docs** (`docs/design/`), for changing the mechanism rather than obeying it:
`build-resource-governance.md` — job/thread/memory budgets, procgov job objects, slot pools, what is
scoped per-machine versus per-worktree.
`conformance-submodule.md` — why the submodule auto-initializes sparsely, and the exact git recipe.
`controls-that-cannot-act.md` — four incidents behind the one rule below.
`agent-doc-gates.md` — what the three gates on this file and the skills check, and why they skip
what they skip.
`fixture-pins.md` — when a test may name a conformance fixture rather than let `discover()` sweep
it, and why a named pin fails instead of skipping when its fixture is gone.

**`docs/divergences/`** — every known C#/Rust difference, its kind, status, and pinning test.

## The one rule behind most of the scars here

**A control that cannot act must say so.** Refuse, panic, or error, naming what you could not do.
`None`, `false`, "skipped", and an unused parameter all read as success to every caller and every
log. And **verify a mechanism by its effect, never by its message** — check the count deleted, the
bytes freed, the fire-count of the branch you think you took. *Scars in
`docs/design/controls-that-cannot-act.md`; the most recent is `gc -Apply`, which abstained whenever
any build was alive anywhere and so reclaimed nothing on a machine that always has one.*
