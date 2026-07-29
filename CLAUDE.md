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
