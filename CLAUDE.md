# Repo instructions

## Managed build commands (required for agent workflows)

All PanGloss Rust builds and tests in agent workflows MUST go through the managed entry point
`rust/tools/pg.ps1` (or its thin front ends `rust/tools/build.ps1` / `rust/tools/test.ps1`) —
never bare Cargo. Bare `cargo build`, `cargo test`, `cargo check`, and `cargo run` are PROHIBITED
in agent workflows: they bypass target-dir redirection, the shared `sccache` compiler cache, the
disk-reserve gate, the cross-worktree build-concurrency limit, process-tree cleanup on
interruption, and — for corpus-backed suites — the fail-closed corpus-required gate that stops a
worktree from reporting a corpus run as green while its declared inputs are absent. See
`docs/superpowers/specs/2026-07-29-categorical-build-hardening-design.md` for the full design.

Use:
- `rust/tools/pg.ps1 -Mode build` (or `build.ps1`) / `-Mode test` (or `test.ps1`) for ordinary work.
- `rust/tools/pg.ps1 -Mode corpus-test` for anything gated on `samples/data/` — it refuses before
  Cargo starts if a required corpus file is missing, and fails a run that records zero executed
  corpus cases.
- `rust/tools/pg.ps1 -Mode release` for optimized deliverables (keeps `[profile.release]`'s fat
  LTO — `test`/`corpus-test` use the lighter `pg-test-opt` profile instead).
- `rust/tools/pg.ps1 -Mode doctor` to check the environment (worktree base, disk, cache, corpus)
  before either, with no Cargo invocation at all.
- `rust/tools/pg.ps1 -Mode gc` to report (dry run, the default) or `-Apply` to remove stale managed
  target directories this repository owns; it never deletes an unmarked, preserved, or still-live
  directory.

Enforcement is a `PreToolUse` hook (`.claude/hooks/block-bare-cargo.py`), not just this rule. It
refuses `cargo build|test|check|run` and `cargo nextest run`; `cargo fmt`/`clean`/`metadata` pass.
The escape hatch is `PANGLOSS_ALLOW_BARE_CARGO=1`, deliberately an env var — needing it means the
managed path is broken and should be fixed, not routed around.

## Running parallel agents without starving the machine

A fleet of six agents in one checkout took C: from 46 GB to 7 GB free, left 26 stray compiler
processes running, and wedged `git` itself. None of it was the agents *working* — it was agents
outliving their usefulness and bypassing the gates. Rules that follow from that, in order of how
much they actually bought:

1. **Cap build-heavy agents at 2–3 concurrent**, matching `Enter-BuildSlot`'s own max of 2. Six was
   over-subscribed threefold; the semaphore only binds callers who go through `pg.ps1` anyway.
2. **Never let an agent poll a background job it spawned.** Tell it to block in the foreground with a
   long tool timeout. Every agent that stalled did so around a self-spawned monitor, and one kept
   spawning poll loops for two hours *after* its work was committed and verified.
3. **Reap on report.** When an agent finishes, kill stray `cargo`/`rustc`/`link`/`pangloss` before
   dispatching the next. Doing this once at the end recovered 11 GB → 63 GB free.
4. **Probe pathological grammars single-threaded.** `pangloss batch`'s thread default fans words out
   concurrently and multiplies their memory: one probe reached 30+ GB RSS and never finished, where
   `--threads 1` plus `--word-timeout-ms` completed the same work in ~2 minutes. See
   `docs/fst-plan/corpus-word-list-hazards.md`.
5. **Assume agents self-verify badly.** In one fleet, two shipped regression gates that passed with
   their own fix reverted, and one reported a feature implemented while its guard sat behind
   `if false &&`. Re-run their gates with the fix bypassed before believing any of it.

## Merging worktree/agent branches into main

Keep `main`'s history linear — no merge commits.

Before merging any worktree/agent branch into `main`:
1. Rebase the branch onto current `main` first (resolve any conflicts there).
2. Merge with `git merge --ff-only <branch>` — this should always be a clean fast-forward
   once step 1 is done. If it isn't a fast-forward, the rebase didn't actually happen against
   the current tip; redo step 1.

Never use `git merge --no-ff`. If a rebase turns out to be non-trivial (real conflicts,
not just staleness), prefer re-running the underlying change fresh against current `main`
over hand-resolving a large/messy conflict set — see the `pg-rename` case for an example
where rebasing was the wrong tool entirely.
