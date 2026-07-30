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

## Keeping SSH / remote desktop alive during builds

This machine is administered remotely, and builds used to freeze SSH and Chrome Remote Desktop
sessions outright. The cause was not disk and not memory: Cargo defaults to one job per *logical*
core (20 here), `Enter-BuildSlot` permits 2 concurrent builds, and every resulting `rustc` ran at
`Normal` priority — the same priority as `sshd` and Chrome Remote Desktop's `remoting_host` video
encoder. ~40 compiler processes over 20 threads, with nothing left for the daemons the machine is
reached through. `pg.ps1` now handles this automatically; both knobs are printed in the preflight
record so a "why is this slower than I expected" question is answerable from the build log:

- **Job cap.** `Get-CargoJobBudget` (`rust/tools/_common.ps1`) sets `CARGO_BUILD_JOBS` to
  `(logical cores − 6) / MaxConcurrent` — 7 per build here. The reserve is
  `$script:InteractiveReserveThreads`, overridable with `PANGLOSS_INTERACTIVE_RESERVE`.
  `.cargo/config.toml` at the **repo root** carries a static `jobs = 8` floor for everything that
  bypasses `pg.ps1` (rust-analyzer's background `cargo check`, IDE tasks). It is at the repo root,
  not `rust/`, because `rust/.cargo/config.toml` is gitignored for personal target redirects and so
  would not exist in a fresh worktree; Cargo merges config from every ancestor directory, deepest
  winning, so a personal `rust/` override still takes precedence.
- **Test-execution cap.** `CARGO_BUILD_JOBS` bounds *compilation only*. Once cargo finishes
  building, nextest and libtest fan out test processes at their own default of one per logical
  core — 20 here, since this repo has no `nextest.toml`. So a capped build was followed straight
  away by an uncapped 20-wide test run, and that is the heavier half: these suites spawn real
  processes (`pangloss.exe`, `worker_test_child.exe`, and a full C **and** C++ toolchain for
  `pg-ffi::header_abi`), and corpus/foma cases can each reach many GB of RSS. Twenty at once is a
  memory storm as much as a CPU one, and memory pressure freezes a remote session faster than CPU
  load. `pg.ps1` now passes `--test-threads` (nextest) / `-- --test-threads` (libtest) from the
  same budget. Override with `-TestThreads N`.
- **Priority.** Cargo is launched `BelowNormal`, which Windows propagates to child processes, so
  `rustc`/`link.exe` inherit it and any interactive daemon preempts compiler work instantly.
  **`Set-SccacheServerPriority` is load-bearing here**: with `RUSTC_WRAPPER=sccache`, `rustc` is
  spawned by the long-lived sccache *server*, not by cargo, so it inherits the *daemon's* priority.
  Measured before that call existed: 7 concurrent `rustc`, only 2 of them `BelowNormal`. If you
  ever add another compiler-spawning daemon, it needs the same treatment.

Override per-run with `-Jobs N` / `-TestThreads N` / `-Priority Normal` (on `pg.ps1`, `build.ps1`,
or `test.ps1`) when you're at the console and there's no remote session to protect.

Two things this deliberately does **not** cover, so don't assume the machine is protected by
`pg.ps1` alone. Bare Cargo in another worktree still runs at `Normal` — the repo-root
`.cargo/config.toml` job floor reaches it (Cargo merges config from ancestor directories, and every
worktree under `.claude/worktrees/` has this repo root as an ancestor), but nothing can set a
process priority from a config file; that's what the `block-bare-cargo.py` hook is for. And
rust-analyzer's background `cargo check` gets the job floor but likewise runs at `Normal`.

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
6. **Never scan from the filesystem root.** Measured: one orphaned
   `find / -iname rewrite.rs -path *foma*` ran 35 minutes at `Normal` priority and burned 2110
   CPU-seconds — a saturated core plus continuous random I/O — writing to a pipe whose reader had
   already exited, so none of it could ever be read. It froze remote sessions on its own, and it
   sits entirely outside `pg.ps1`'s priority and concurrency controls, which only govern Cargo and
   what Cargo spawns. Use `rg --files`, a scoped `Glob`, or `git ls-files`; all answer in under a
   second. Unscoped `find` is also slow enough to trip tool timeouts (`find . -name nextest.toml`
   took >120s just walking `.claude/worktrees/`), and a timeout is exactly what orphans the process.

`pg.ps1 -Mode gc` reaps dead-parent `cargo`/`rustc`/`link`/`cc1` and, separately, dead-parent
`find`/`rg`/`grep`/`findstr` that have burned >60s CPU and lived >2min. Dry-run by default;
`-Apply` to act.

## Playing nicely with other worktrees

Several worktrees build concurrently on this machine, so every machine-wide mechanism here is
built to fail in the conservative direction. If you touch any of it, keep that property:

- **The gc process sweeps are machine-wide** — they can see builds belonging to worktrees you know
  nothing about. Liveness is decided by `Test-ParentAlive` (PID-reuse-safe: a candidate parent
  created *after* its child is not the parent) and never by name, age, or CPU. The earlier version
  used `Get-Process -Id`, which also reports failure for access-denied, so "I could not look" read
  as "it is dead" — the exact false positive that kills a healthy build in another worktree.
- **Only scanners are reaped on thresholds**, never compilers. An orphaned `rustc` has at least
  produced object files; an orphaned `find` has produced a closed pipe. `rust/tools/tests/
  orphan-reaping.tests.ps1` asserts that no Rust build binary can be selected by the scan sweep at
  any age or CPU.
- **`gc` never deletes a target dir whose worktree still exists**, is unmarked, or is preserved.
- **The build-slot semaphore and job budget are machine-wide conventions**, not per-invocation
  guarantees — `Get-CargoJobBudget` divides by `MaxConcurrent` precisely so two worktrees building
  at once still leave the interactive reserve free.
- **`sccache`'s server is shared**, so `Set-SccacheServerPriority` changes the priority of *every*
  worktree's compilation, not just yours. That is why `BelowNormal` is the default and why
  `-Priority Normal` should be a deliberate, temporary choice.

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
