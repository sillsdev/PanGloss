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

- **Memory headroom.** Threads were capped and *bytes were not*, and the machine was taken to zero
  memory twice on 2026-07-30 with every CPU control above already in place. A daemon blocked on a
  page fault stalls a remote session exactly as hard as one starved of CPU, and `BelowNormal` buys
  nothing there — it is not waiting for the scheduler. So `pg.ps1` now also **refuses to spawn**
  when available memory is under `Get-SpawnFloorGB`,
  exiting **17** — distinct from low-disk's 12, because the recovery is completely different. It
  prints the largest working sets so the refusal is actionable, and re-checks *after* the
  build-slot wait, since a 30-minute queue is exactly how an approved reading goes stale. `doctor`
  reports the same state; `gc` is exempt, because it is the recovery action.
  Available memory then narrows `-Jobs`/`-TestThreads` the same way cores do, and the preflight
  record names which of the two actually bound the number.

  **Every threshold here is proportional to installed RAM, never a fixed number of gigabytes.** A
  flat figure cannot be right on two machines at once, and the failure is asymmetric: too low on a
  big box risks the machine, too high on a small box blocks ordinary work — and a gate that blocks
  ordinary work gets set to 0, protecting nobody. An 8GB reserve is 12% of a 64GB box and **50% of a
  16GB developer machine**. So the reserve is 10% of installed RAM clamped to [1.5, 6]GB, the spawn
  floor is that plus ~2GB of room for the build itself, and the job-object cap is
  `(installed − reserve) / slots`:

  | Installed | Reserve | Spawn floor | Job cap (of 2 slots) |
  |---|---|---|---|
  | 16GB | 1.6GB | 3.6GB (22%) | 7GB |
  | 32GB | 3.2GB | 5.2GB (16%) | 14GB |
  | 64GB | 6GB | 8GB (12%) | 29GB |

  Note the 64GB row lands on the flat 8GB it replaced — which is exactly why that number looked
  right on the box it was picked on. Overrides: `PANGLOSS_MEM_RESERVE_FRACTION`,
  `PANGLOSS_MIN_FREE_MEM_GB` (absolute), `PANGLOSS_MIN_BUILD_ROOM_GB`, `PANGLOSS_JOB_MEM_GB`.
  Caveat at the small end: below ~12GB installed, two concurrent builds cannot both fit under the
  reserve (the job cap floors at 4GB to keep linking working), so such a machine should also run
  `-MaxConcurrent 1`. Nothing enforces that yet.

- **Kernel-enforced ceilings (`procgov`).** The pre-spawn gate cannot bound a peak that develops ten
  minutes into a build, so every managed build runs inside a **Windows job object** via
  [procgov](https://github.com/lowleveldesign/process-governor) — `--maxjobmem` (committed memory
  for the whole tree), `--cpurate` (hard CPU ceiling), `-r` (bind every rustc/link.exe, not just
  cargo). Install: `winget install LowLevelDesign.ProcessGovernor`. It is **optional**: without it
  builds still run, with every pre-spawn gate intact and a loud warning.

  This is prefabricated on purpose. A hand-rolled polling watchdog plus a machine-wide memory
  reservation ledger were written first and then deleted — the kernel enforces at allocation time
  with no sampling interval to lose a spike in, and a job memory cap makes a runaway fail *its own
  allocation* rather than taking the machine down. With `Enter-BuildSlot` capping builds at 2 and
  each one capped by a job object, the machine-wide worst case is bounded by construction, which is
  why no reservation ledger is needed to stop several waiting builds from starting together.

  Cargo has no equivalent: [cargo#12912](https://github.com/rust-lang/cargo/issues/12912) (limit
  parallelism automatically) is open and `S-needs-design`, [#9157](https://github.com/rust-lang/cargo/issues/9157)
  (restrict parallel linker invocations) likewise, and [#11707](https://github.com/rust-lang/cargo/issues/11707)
  / [#9735](https://github.com/rust-lang/cargo/issues/9735) describe this exact workspace shape
  (OOM linking many binaries). No cargo plugin solves it. Don't re-invent this locally.

  **Measured 2026-07-30 — read this before blaming the build for the next exhaustion.** A full
  `-Mode test` build (711 samples, 313 processes) peaked at **1.08GB** for the largest single rustc
  and **4.03GB across the entire fan-out**, never dropping below 50.4GB free. A forced fat-LTO
  relink of the `pangloss` binary peaked at 0.71GB. **Compiling and linking are not where this
  machine's memory goes.** What the same run *did* show is **446 threads on 20 logical cores** — a
  22x oversubscription, because `-j` caps codegen workers *within* one rustc but not threads across
  instances ([rust#81957](https://github.com/rust-lang/rust/issues/81957)). `--cpurate` is the only
  thing that actually bounds that; `jobs = 8` cannot.

  On the "it got faster, so it crashed" theory: the mechanism is real — peak memory is (jobs
  simultaneously in their heavy phase) x per-job peak, and anything that raises throughput, including
  the Windows Defender exclusions for the Rust toolchain, means less time blocked on I/O and so more
  rustc processes compute-resident at once. But it cannot account for exhausting 64GB *while
  building*: the measurement above was taken with those exclusions already in place and still peaked
  at 4.03GB, so the theory would need ~16x the observed peak. What the exclusions plausibly did
  worsen is the CPU side (446 threads, 100% CPU), and a box at 100% CPU with no priority headroom is
  indistinguishable from a crashed one over SSH or remote desktop. If a "crash" during a *build*
  needs explaining, suspect CPU starvation before memory.

  So the memory exhaustion is by elimination in test *execution*, not the build:
  `$script:MemoryPerTestProcessGB` (2.5GB) remains an **unmeasured placeholder**, a corpus/foma case
  can be a whole grammar compile, and one `pangloss batch` probe reached 30+ GB RSS. Measuring a
  corpus-test *run* is the outstanding calibration. At rest none of the per-process numbers bind —
  an idle 63.7GB box still gets all 7 jobs, deliberately: a gate that taxes every ordinary build
  gets switched off and then protects nothing.

- **Direct binary invocation (`pg.ps1 -Mode run`).** Every mechanism above wraps CARGO ONLY —
  `Enter-BuildSlot`, the job-budget derivation, and (until 2026-07-31) the procgov job object all
  live inside `Invoke-CargoWithReaper`, which nothing but a `cargo build/test` call ever reached. A
  hand-run `examples\predict_census.exe` or a bare `pangloss batch` was covered by NONE of it. The
  Windows event log shows exactly what that gap cost, all three a single PanGloss binary invoked
  **directly**, never through cargo (Microsoft-Windows-Resource-Exhaustion-Detector, event ID
  2004 — see below):

  | Date | Binary | Committed memory |
  |---|---|---|
  | 2026-07-04 | `hc-rs.exe` | 97 GB |
  | 2026-07-26 | `pangloss.exe` | 90 GB |
  | 2026-07-30 | `predict_census.exe` | 118 GB (climbed over ~45 minutes) |

  For contrast, the measured full managed `-Mode test` build above peaks at 4.03GB. The hardened
  path was never the problem; the unhardened path used 118GB. `-Mode run` closes this by giving an
  arbitrary binary the SAME kernel-enforced ceiling a build gets: `Invoke-CargoWithReaper`'s
  procgov-wrapping body was extracted into a reusable `Invoke-ProcessInJobObject`
  (`rust/tools/_common.ps1`), and `Invoke-CargoWithReaper` is now a thin, behavior-preserving front
  end onto it. Three invocation shapes:
    - `pg.ps1 -Mode run -Example <name> -- <args>` — `cargo run --example <name>` (builds first,
      then runs the result as a job-object CHILD of cargo; procgov's `-r` recurses the ceiling onto
      it exactly like it already does for rustc/link.exe).
    - `pg.ps1 -Mode run -Bin <name> -- <args>` — same, for a workspace `[[bin]]` target.
    - `pg.ps1 -Mode run -Exe <path> -- <args>` — runs an already-built executable directly, no
      cargo involved.
  The job-object memory cap defaults to the SAME machine-proportional figure a build gets
  (`Get-JobMemoryCapGB`, divided across `-MaxConcurrent` slots) and is overridable per-run with
  `-RunMemoryGB` — e.g. a deliberate 40GB experiment — without touching `PANGLOSS_JOB_MEM_GB`,
  which would also change every ordinary build's cap for as long as the env var stayed set.

  **`run` DOES take a build slot** (`Enter-BuildSlot`), weighed deliberately rather than assumed:
  the alternative — a `run` that doesn't count against the semaphore — breaks the property the rest
  of this file relies on to avoid a reservation ledger, namely that at most `-MaxConcurrent` heavy
  operations share the machine's headroom at once, so each one's job-object cap is safe *by
  construction*. A `run` outside that count is an unaccounted-for extra consumer on top of up to
  `-MaxConcurrent` full-cap builds — the exact "several things assume they have the whole machine's
  headroom, simultaneously" shape that produced the table above. The cost of taking the slot is
  that a probe can occupy it for hours (that is the whole point of `run` — a `predict_census`-shaped
  binary is not a five-minute build), so a build queued behind a long `run` can hit
  `-BuildSlotTimeoutSeconds`'s 30-minute wait and exit needing a retry. That is a known, recoverable,
  loudly-reported cost; an unbounded machine-wide worst case is what this whole file exists to rule
  out, so the slot is taken unconditionally. If procgov is absent, `run` degrades exactly like a
  build does: a loud warning, but it still runs — an absent tool must never block the workflow.

- **Reading the exhaustion log (`pg.ps1 -Mode doctor`).** Windows already diagnoses the low-memory
  condition above and logs it — the table's three figures all came from
  `Microsoft-Windows-Resource-Exhaustion-Detector` (event ID 2004) in the System log — and nobody
  was reading it before 2026-07-31. `Get-ResourceExhaustionEvents` (`rust/tools/_common.ps1`) reads
  the last 7 days of these events via `Get-WinEvent` and `doctor` now reports them: event count,
  most recent timestamp, and (best-effort) the top consumer names/bytes parsed out of the message
  text. Message-text parsing is split into its own pure function
  (`Get-ExhaustionConsumersFromMessage`) precisely because it IS fragile — Microsoft publishes no
  stable grammar for it — so a parse failure degrades to the raw message text, never a thrown error
  or a silently dropped event. This history is reported prominently but **never fails doctor**: the
  four checks that DO gate doctor's exit code (disk/memory/base/sccache) all describe the
  environment *right now*, whereas an exhaustion event describes something that already happened
  and the machine already recovered from on its own — failing doctor on old history would block
  every managed build for the whole 7-day window for no actionable reason. This is the same rule
  this file states elsewhere for a different failure mode: "I could not look" must never read as
  "everything is fine" — and, symmetrically, "something bad happened once" must never read as
  "something is wrong right now." Get-WinEvent throws (rather than returning empty) both when there
  is genuinely nothing in the window and when it cannot query at all (provider absent, access
  denied); those two are NOT the same fact, so `Get-ResourceExhaustionEvents` distinguishes them by
  matching on Get-WinEvent's own exception text (there is no separate exception type) rather than
  collapsing both to "no data".

Override per-run with `-Jobs N` / `-TestThreads N` / `-Priority Normal` (on `pg.ps1`, `build.ps1`,
or `test.ps1`) when you're at the console and there's no remote session to protect. `-Jobs` and
`-TestThreads` are never narrowed by the memory budget — an explicit number stays the number.

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

## What is scoped to the PC, and what is scoped to the worktree

Several worktrees run here, sometimes with more than one agent inside a single worktree. Every
resource control below has to be classified correctly or it protects nothing: a per-worktree cap on
a machine-wide resource just multiplies by the number of worktrees. The rule is what the resource
*is*, not who is asking for it.

| Concern | Scope | Mechanism |
|---|---|---|
| CPU cores | **per PC** | `Get-CargoJobBudget` (cores − reserve ÷ slots), `-TestThreads`, `BelowNormal` priority, and `procgov --cpurate` as the only hard ceiling |
| Memory | **per PC** | spawn gate (machine-wide available memory) + `procgov --maxjobmem` per build |
| Taking your turn | **per PC** | `Enter-BuildSlot` — `Global\PanGlossCargoBuild`, a named semaphore, max 2 |
| Killing old processes | **per worktree** | `gc`'s orphan sweeps: liveness by dead *parent*, never by name/age |
| Disk / target dirs | **per worktree** | ownership markers; `gc` never deletes another worktree's target |

The build slot is the one people ask for by name — "don't start a third build if two are going" is
already exactly what `Enter-BuildSlot` does, and it binds across worktrees *and* across agents
inside one worktree, because a Windows named semaphore is per-machine. A third `pg.ps1` waits, then
exits 15 after 30 minutes rather than hanging forever.

Two honest limits on that. The semaphore's maximum is fixed by whichever process creates it first
and cannot be changed while it lives, so `-MaxConcurrent` is a *convention* everyone must pass the
same value for, not a per-invocation guarantee. And it only binds callers who go through `pg.ps1` —
bare cargo in another worktree takes no slot, which is what `block-bare-cargo.py` exists for.

The width knobs are weaker still: `Get-CargoJobBudget` always divides by `MaxConcurrent = 2`
whether or not a second build exists, so a solo build takes 7 jobs where 14 would be safe, and two
builds take 7 each whether or not the other one is there. It assumes the worst case permanently
rather than measuring. Memory is the counter-example worth copying — it is derived from a live
machine-wide reading, so it sees other worktrees (and bare cargo, and anything else) for free.

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
