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

The standing instance: an **underdefined project** (`CONTEXT.md`) runs on provisional definitions
that C# lacks, by design (`docs/adr/0008-provisional-definitions.md`). That is not a parity bug,
but each behaviour it introduces is still its own ledger entry and gated fixture. A fully defined
project gets exact parity.

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
| Bare `cargo build`, `cargo test`, `cargo check`, `cargo clippy`, `cargo run` and `cargo nextest run` are PROHIBITED — use `rust/tools/pg.ps1` | `.claude/hooks/block-bare-cargo.py` |
| Every compile mode refuses a tree CI would refuse, before tests or a build start: rustfmt is applied, comment hygiene is fatal, clippy runs with `-D warnings` | `pg.ps1`: exit 41 (hygiene), exit 40 (clippy); `rust/tools/tests/lint-gate.tests.ps1` |
| No release from a commit whose `Rust CI` run is not green | `release preflight` waits for the exact tagged source commit in `.github/workflows/rust-gates.yml` |
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
- **Reap on report.** Kill verified orphan `cargo`/`rustc`/linker/`pangloss` processes when an
  agent finishes. `pg.ps1 -Mode gc` does it; Linux ownership checks live in `rust/tools/_linux_gc.ps1`.
- **Probe pathological grammars single-threaded.** `pangloss batch --threads 1` plus
  `--word-timeout-ms`. *Scar: one probe reached 30+GB RSS and never finished; the same work took ~2
  minutes single-threaded.*
- **Cap build-heavy agents at 2-3 concurrent.** `Enter-ResourceSlot -Pool build` caps *builds* at 2 machine-wide,
  but nothing caps agents; extras just queue and then exit 15.
- **A long command that is NOT a managed build should be a background job**, so the harness notifies
  on completion. *Scar: a 7,121-word corpus batch run in the foreground truncated silently at ~1,663
  words and was read as "the corpus".*
- **Conformance grammars use synthetic data only** — invented lexemes, never real-language data, and
  no language named outside a comment.

## Managed builds, in one place

`rust/tools/pg.ps1` (or `build.ps1`/`test.ps1`). Reach for `check` first, `test` last: the cost of a
round trip is compiling and linking test binaries (pg-foma has 11 integration targets and
pg-foma-backend has 9, plus examples), not running tests.

`-Mode check` is `cargo clippy --all-targets -- -D warnings` with CI's exact flags: it type-checks
everything including test code and lints it as CI does. `-Mode quick` adds unit tests.
`-Mode test` / `-Mode conformance-test` are authoritative — a green `quick` is not a green suite.

**Fail up front, not at release.** Every compile mode applies rustfmt, then refuses on a comment
hygiene violation (exit 41), then runs the same clippy (exit 40) before any test or build starts. So
a tree that passes a managed run passes CI's fmt, clippy and hygiene gates. Fix what they report;
never route around them with bare cargo or an `#[allow]` you cannot justify. Run `-Mode check`
before every commit, and commit the rustfmt reflow with the change. *Scar: v0.5.0 needed three
release runs — a clippy error that had kept `Rust CI` red since the commit that introduced it, then
a rustfmt failure in the fix, which was verified with bare `cargo clippy` instead of `pg.ps1`.*
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

## Releases start with a pushed version tag

Author the `CHANGELOG.md` section, then run `pwsh -NoProfile -File rust/tools/release.ps1
-Version x.y.z` from a clean, up-to-date `main`. The script stamps `[workspace.package].version`,
refreshes `Cargo.lock` through managed `pg.ps1 -Mode check`, commits `release: vX.Y.Z`, creates an
annotated `vX.Y.Z` tag, and prints `git push --atomic origin main vX.Y.Z`. It never pushes. `-DryRun` checks
the branch, tree, version, tag, and changelog without changing files.

Pushing main and the tag starts Rust CI and `.github/workflows/release.yml`. The tag must be
annotated, match the workspace version and changelog, and point to a commit reachable from
`origin/main`; the workflow verifies `Cargo.lock` with locked metadata and waits for Rust CI on that
commit. It then runs the shared Rust and release gates, builds and smoke-checks Windows x64, Linux
x64, macOS arm64, and macOS x64, verifies all ten assets and checksums, and publishes a GitHub
release on the existing tag. CI never commits to main or creates or moves tags.

`workflow_dispatch` takes a version and an explicit source ref and runs the same preflight, gates,
and builds without publishing. Prerelease tags are not supported. Coverage and the real-language
corpus exclusion are recorded in the GitHub release notes body.

For the pending 0.7.0 cut, `main` currently has workspace version 0.6.2. After this change merges,
run `pwsh -NoProfile -File rust/tools/release.ps1 -Version 0.7.0` on `main`, then run the exact
`git push --atomic origin main v0.7.0` command it prints. The tag workflow waits for Rust CI before building
and publishing. See `docs/development/releasing.md` for the complete procedure and rehearsal path.

**The contract gate is `machine/conformance`** -- engine-agnostic, all-synthetic, diffed against
committed ground truth, and available to CI through the submodule. The real-language corpora (Sena,
Amharic, Aweti, Indonesian) are gitignored local files: they exist for speed work and for harvesting
issues that then become conformance fixtures. They are the sampling tool, not the contract, and
their absence from a release gate is deliberate rather than a gap.

*Historical note: the former release process was tied to one workstation. One attempt refused six
roots of unrelated local scratch, and another found a drifted FieldWorks checkout. The tag flow now
checks this worktree's clean, current `main` and validates the pushed source commit in CI.*

## Merging into main

Keep history linear. Rebase the branch onto current `main`, then `git merge --ff-only`. If it is not
a fast-forward, the rebase did not happen against the current tip; redo it. Never `--no-ff`. If a
rebase turns out to involve real conflicts rather than staleness, prefer re-running the change fresh
against `main`.

When `pg.ps1` prints `rustfmt: applied`, commit that reflow with your change. Never revert it:
reverting leaves `main` unformatted, so every later build in every worktree redoes the same
reflow, and a build that follows a revert recompiles everything. *Scar: one release run rebuilt all
~105 test targets twice for this reason, 50 minutes instead of 11.* `release.ps1` refuses an
unformatted tree before it commits: its managed check runs after the stamp, and the script refuses
to commit if rustfmt or another check leaves changes outside `rust/Cargo.toml` and `rust/Cargo.lock`.

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
`build-resource-governance.md` — thread/memory budgets, direct process launches, slot pools, what is
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
