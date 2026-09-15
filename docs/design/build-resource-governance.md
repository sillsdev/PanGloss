# Build resource governance

Why `rust/tools/pg.ps1` is shaped the way it is: the job/thread/memory budgets, the job objects,
the two slot pools, and what is scoped per-machine versus per-worktree. This is maintainer
documentation, extracted from CLAUDE.md because no agent can violate it during ordinary work — the
mechanisms enforce themselves. Read it when changing `pg.ps1`/`_common.ps1`, or when a build's
resource behaviour needs explaining.

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
  core — 20 here, because `rust/.config/nextest.toml` sets no thread count (it exists, but only to put
a 10-minute kill on a hung test; read it before adding a knob here). So a capped build was followed straight
  away by an uncapped 20-wide test run, and that is the heavier half: these suites spawn real
  processes (`pangloss.exe` and a full C **and** C++ toolchain for
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

  **`run` DOES take a slot** (`Enter-ResourceSlot`), weighed deliberately rather than assumed: the
  alternative — a `run` that counts against nothing — breaks the property the rest of this file
  relies on to avoid a reservation ledger, namely that at most `-MaxConcurrent` + `-MaxConcurrentRuns`
  operations share the machine's headroom at once, so each one's job-object cap is safe *by
  construction*. A `run` outside that count is an unaccounted-for extra consumer on top of up to
  `-MaxConcurrent` full-cap builds — the exact "several things assume they have the whole machine's
  headroom, simultaneously" shape that produced the table above.

  **But it takes a RUN slot, not a build slot** (4 wide by default, `PANGLOSS_RUN_SLOTS` /
  `-MaxConcurrentRuns`). One queue for both was measured costing real time for no resource reason: a
  0.3s `pangloss parse` waited behind two multi-minute builds in another worktree. A build is bounded
  by disk and memory; a light run writes no target dir and — measured — barely moves memory either.
  What it can exhaust is CPU, and CPU is now budgeted across both pools rather than per-pool (see the
  scoping table below). A light run gets one core, a `--cpurate` share sized from that one core, and a
  **flat 2GB** memory ceiling (`Get-RunJobMemoryCapGB`, `PANGLOSS_RUN_MEM_GB`) rather than a build's
  machine-proportional cap — flat because a runaway is recognizable by absolute size, and 2GB because
  the full 6,146-word Sena corpus through the HermitCrab engine peaks at **454MB**, a peak set by the
  hardest single word rather than by accumulation, so corpus size does not move it. Measurements and
  the `--memo` comparison are in `docs/research/build-resource-governance.md`.

  **`-Heavy` is the other direction:** it puts a genuinely build-sized probe (`predict_census` and
  friends) back in the **build** pool with a build's ceilings. The cost that remains is that such a
  probe can occupy a build slot for hours, so a build queued behind it can hit
  `-BuildSlotTimeoutSeconds`'s 30-minute wait and exit needing a retry. That is a known, recoverable,
  loudly-reported cost; an unbounded machine-wide worst case is what this whole file exists to rule
  out, so a slot is taken unconditionally. If procgov is absent, `run` degrades exactly like a
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


## What is scoped to the PC, and what is scoped to the worktree

Several worktrees run here, sometimes with more than one agent inside a single worktree. Every
resource control below has to be classified correctly or it protects nothing: a per-worktree cap on
a machine-wide resource just multiplies by the number of worktrees. The rule is what the resource
*is*, not who is asking for it.

| Concern | Scope | Mechanism |
|---|---|---|
| CPU cores | **per PC** | `Get-CargoJobBudget` (cores − reserve − run pool ÷ build slots), `-TestThreads`, `BelowNormal` priority, and `procgov --cpurate` — now sized from **one slot's own width**, so the per-job ceilings sum to the machine-wide one instead of each requesting all of it |
| Memory | **per PC** | spawn gate (machine-wide available memory) + `procgov --maxjobmem` per build; a light run gets a flat 2GB instead (`Get-RunJobMemoryCapGB`) |
| Taking your turn | **per PC** | `Enter-ResourceSlot` — two independent named-**mutex** pools: `Global\PanGlossBuildSlot0..N-1` (default 2) and `Global\PanGlossRunSlot0..M-1` (default 4) |
| Killing old processes | **per worktree** | `gc`'s orphan sweeps: liveness by dead *parent*, never by name/age |
| Disk / target dirs | **per worktree** | ownership markers; `gc` never deletes another worktree's target |

The build slot is the one people ask for by name — "don't start a third build if two are going" is
already exactly what `Enter-BuildSlot` does, and it binds across worktrees *and* across agents
inside one worktree, because a Windows named semaphore is per-machine. A third `pg.ps1` waits, then
exits 15 after 30 minutes rather than hanging forever.

**Two pools, one core budget.** `pg.ps1 -Mode run` queues in the *run* pool, not the build pool,
because a build is bounded by disk and memory while a run is bounded by CPU — and a 0.3s
`pangloss parse` waiting behind a three-minute build served no resource purpose. What the two pools
share is the **one** machine-wide core budget: the run pool's allotment (slots × 1 core) comes off
the top of `Get-CargoJobBudget` before the remainder is divided among build slots, so builds pay for
the run pool's existence rather than the machine being oversubscribed when every slot is busy. Same
transition hazard as the semaphore→mutex migration below: while some worktrees still run the old
code, their `run` takes a *build* slot while a new-code worktree's takes a *run* slot, so the two do
not exclude each other for runs — tolerable only because the per-job CPU and memory ceilings are
sized for the full six-slot worst case regardless. A run-slot timeout reuses exit **15**: the
recovery (wait and retry) is identical, and this file's rule for a distinct code is that the recovery
differs. The message names which pool timed out.

**It is N mutexes, not a counted semaphore, and that was a bug fix — do not "simplify" it back.**
The semaphore this replaced **deadlocked every worktree on this machine on 2026-07-31**: 4+ worktrees
sat at "waiting for a build slot" for 20+ minutes with *zero* cargo/rustc/link processes alive
machine-wide, recoverable only by hand-releasing the semaphore until it threw. A counted semaphore
never restores its count when the holder dies, and in agent workflows the holder dies constantly —
a tool timeout, an agent stop/resume, or a detached invocation whose parent conversation has gone
all kill `pg.ps1` between acquire and release. Assume any critical section between the two *will*
be interrupted.

A mutex cannot leak that way because the **kernel** owns the cleanup: a holder that dies leaves the
mutex ABANDONED and the next waiter is granted ownership (`AbandonedMutexException`, carrying the
index). Catching it and continuing *is* the recovery — no ledger to reconcile, no sweep to schedule,
no hand-repair procedure. Same reasoning that replaced the hand-rolled memory watchdog with a job
object: prefer the primitive whose cleanup the OS already guarantees.
`rust/tools/tests/build-slot.tests.ps1` pins this by killing a real holder and requiring the slot to
be reacquirable.

It also fixes a wart that was **measured failing**: a semaphore's maximum is frozen by whichever
process creates it first and cannot be queried, and on 2026-07-31 three procgov-wrapped builds ran
concurrently under a nominal limit of 2 (orphaning and a `Global\`/`Local\` namespace split were both
ruled out). With mutexes the slot count is simply how many names a caller waits on, so
`-MaxConcurrent 1` genuinely cannot take a second slot. `Get-JobMemoryCapGB` still sizes for
`MaxConcurrent + 1` as belt-and-braces, so the memory bound survives one slot of over-admission.

Two things it still does **not** fix. It only binds callers who go through `pg.ps1` — bare cargo
takes no slot, which is what `block-bare-cargo.py` is for. And the queue is **unfair**: Windows makes
no ordering guarantee, so a waiter can starve (measured: one timed out after the full 30 minutes
while *newer* arrivals were granted slots), and the timeout is arithmetically unreachable for a deep
queue — N builds two-at-a-time take ≈ N/2 × T, so at 10 worktrees and T = 10 min the last waiter
needs 40+ minutes against a 30-minute limit and always exits 15 regardless of load. That wastes time
and confuses agents but cannot exhaust the machine, since the job objects bound that. A fair FIFO
ticket lock would fix it and has deliberately not been built. What *is* built is visibility: a waiter
and `doctor` both print who holds each slot (pid, mode, worktree, since when, and whether that pid is
still alive), because a 20-minute anonymous wait is indistinguishable from a deadlock and that
ambiguity is what actually burned the time during the incident.

**Transition hazard:** a worktree still on the old semaphore code and one on the mutex code share no
mutual exclusion at all. Every worktree must pick this up, or real concurrency becomes
(old-code builds) + (new-code builds).

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

