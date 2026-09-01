# Build resource governance (`rust/tools/_common.ps1`)

Consolidated design rationale for the mechanisms `rust/tools/_common.ps1` implements to keep a
managed build/test/run from starving this machine's interactive daemons (SSH, Chrome Remote
Desktop) or exhausting its memory. The measured incidents that motivated each mechanism are
recorded in the repo's own `CLAUDE.md` ("Keeping SSH / remote desktop alive during builds" and
"Running parallel agents without starving the machine"); this document states the mechanisms
themselves, for comments that need to cite the reasoning without repeating it inline.

## Target-dir placement (SSD vs HDD)

Two cache roots, two different access patterns. The SSD root (`C:`, NVMe) is preferred for the
*active* target-dir: compiling and linking hammer many small `.rlib`/`.rmeta`/object files with
scattered random I/O (worse under `lto=fat, codegen-units=1`), which is exactly what an HDD's seek
time punishes and NVMe doesn't have. The HDD root (`G:`) is used once the SSD's free space drops
below its reserve, and is preferred for sccache's cache regardless: a cache hit is one blob read,
not scattered small-file churn, so the HDD's capacity matters more than its seek time there, and
keeping the shared cache off the SSD stops it contributing to an SSD space crisis.

The SSD reserve exists because unbounded worktree growth already drove free space on `C:` down to
1.3GB once; it is a hard "do not add another worktree's target dir here" line, not just "is there
any space at all."

## CPU thread reserve

Cargo defaults to one compiler job per logical core, and the build-slot mutex (below) permits
multiple concurrent invocations, so an unbounded job width can massively oversubscribe the machine
and starve the daemons the box is administered through (sshd, Chrome Remote Desktop's
`remoting_host` video encoder). The reserve is a fixed thread count, sized for those daemons plus
the shell/editor driving the build — not for a second full workload — and is subtracted from
logical core count before deriving each build's `-j`.

## Memory reserve: "available", not "free"

`Win32_OperatingSystem.FreePhysicalMemory` counts only the free list and omits the standby list —
cache pages Windows will hand to a new allocation on demand. On a machine that has been building
for a while, almost all reclaimable memory sits in standby, so gating on `FreePhysicalMemory` would
refuse builds on a machine with tens of GB genuinely available. `Win32_PerfRawData_PerfOS_Memory`'s
`AvailableBytes` is the counter Task Manager labels "Available" and is what this repo gates on
instead. Both CIM classes are used (rather than `Get-Counter`'s `\Memory\Available MBytes` path)
because their property names are not localized; the `Get-Counter` path is, and throws on a
non-English Windows.

An unqueryable reading returns `$null`, never `0`: a failed query must never read as "nothing is
available" and block every build on a machine where the CIM query itself fails. The same reasoning
applies to disk-space queries (`Test-DiskReserve`) and is why every threshold-comparing function in
this file takes a `[Nullable[double]]`, never a plain `[double]` — the plain type would silently
coerce a passed `$null` to `0.0`.

**Proportional, never a flat number of gigabytes.** A flat reserve cannot be right on two machine
sizes at once: a fixed figure that is a sane fraction of a large box is half the RAM of a small
developer machine, and a gate that blocks ordinary work on the small box gets disabled and then
protects nobody. The reserve is a fraction of installed RAM (`Get-InteractiveReserveGB`), clamped
to a floor and ceiling so it stays meaningful at both extremes; the same proportional approach
sizes the per-build job-object memory cap (`Get-JobMemoryCapGB`) and the CPU rate ceiling
(`Get-JobCpuRatePercent`).

**Commit charge is a distinct resource from available physical memory** (`Get-CommitChargeGB`).
`git`'s own child-process fork can fail with `MEM_COMMIT failed` while available physical memory
reads generously high, because the commit charge was near its limit even though RAM was free. Two
things this repo relies on are commit-denominated, not physical-memory-denominated: the
Resource-Exhaustion-Detector's event 2004 reports committed memory per process, and procgov's
`--maxjobmem` caps committed bytes. Reporting only available *physical* memory while enforcing on
*commit* is how "there is plenty of memory" and "allocation failed" can both be true at once.

## Per-job memory allowances

Three different weights, because the phases are not comparable:
- A **thin/no-LTO compile job** (the `pg-test-opt`/dev profiles) is the predictable case: one rustc
  compiling one crate.
- A **fat-LTO link job** (`[profile.release]`: `lto = "fat", codegen-units = 1`) is heavier per
  process, because whole-program optimization happens inside `rustc` itself — it holds an entire
  dependency graph's LLVM IR in one address space — and `link.exe` merely consumes the object rustc
  produced. Cargo has no lever for "how many crates may be in their LTO phase at once" separate from
  `-j`, so N concurrent jobs can mean N overlapping LTO peaks; this is why the allowance is per-job
  rather than a separate link-concurrency knob. This allowance is measured, and deliberately not
  padded far past the measurement: an earlier draft assumed a much heavier figure and cut every
  ordinary build's job width to a fraction of what the machine could actually sustain, for no real
  protection — a gate that taxes every ordinary build at rest gets turned off and then protects
  nothing under real pressure.
- A **test process** in this workspace can be an entire grammar compile and is bounded by nothing
  the tooling controls. This is the allowance with real evidence behind the *risk* (a `pangloss
  batch` probe reaching 30+ GB RSS, recorded in CLAUDE.md) and no measurement behind the *number*:
  it is a placeholder chosen to be heavier than a compile job, not a recorded peak. Its protection
  on the test path rests on the reserve and the spawn refusal rather than on this figure being
  exactly right.

## Job budget and concurrency resolution

`Get-CargoJobBudget` derives `-j` from
`(logical cores − thread reserve − run-pool allotment) / MaxConcurrent`: dividing by `MaxConcurrent`
rather than handing out the whole budget is required because the build-slot mutex is machine-wide —
if two worktrees can each hold a slot, each one's job count has to be sized for the case where both
do. The run pool's allotment (`RunSlots × RunThreadsPerSlot`) comes off the top for exactly the same
reason: it is a second machine-wide pool that can be fully occupied while both builds run, so a
build sized as though it did not exist oversubscribes the machine precisely when every slot is busy.
It floors at 2, not 1: a single-job cargo serializes codegen across the whole workspace, which in
practice gets the cap disabled entirely rather than tuned.

`Get-MemoryProcessBudget` performs the analogous derivation from available memory, and returns
`$null` (not a fabricated number) when memory is unqueryable, so a caller combining it with the CPU
budget can distinguish "memory says N" from "memory has no opinion." `Resolve-ConcurrencyBudget`
picks the lower of the two and records *which one bound the result*, so a preflight report can
state the real reason a run is narrower than the core count instead of only printing a number with
no derivation behind it. An explicit override is never narrowed by either budget — silently
overriding an operator's explicit `-Jobs`/`-TestThreads` would make the number printed beside it a
lie.

`Get-CargoJobBudget`'s result is exported as `CARGO_BUILD_JOBS` rather than appended as a `-j` flag,
for two reasons: it reaches cargo subcommands that don't take `-j` in the same position (nextest's
`--cargo-profile` form), and it beats `rust/.cargo/config.toml`'s static `jobs` floor in Cargo's own
precedence order (CLI > env > config) without overriding an explicit `-j` a caller put in extra args,
which still wins.

## Kernel-enforced ceilings (procgov)

A Windows job object, launched via [procgov](https://github.com/lowleveldesign/process-governor),
replaces three mechanisms that were tried and removed because each was worse and still had to be
maintained here:
- A polling loop sampling available memory and killing on a threshold has a sampling interval a
  spike can hide in; a job object enforces a commit limit at *allocation time*, so an over-limit
  process fails its own allocation instead of the whole machine going unreachable.
- A machine-wide memory reservation ledger existed to stop several waiting builds from all seeing
  "memory is free" and starting together; with a hard per-build job-object cap plus the build-slot
  mutex's fixed concurrency, the machine-wide worst case is bounded by construction, so the race
  stops mattering and the ledger's bookkeeping is unnecessary.
- `-j`-based CPU limiting cannot bound rustc's total thread count: `-j` caps codegen workers
  *within* one rustc instance, not threads across instances
  ([rust-lang/rust#81957](https://github.com/rust-lang/rust/issues/81957)). `--cpurate` is a
  kernel-enforced ceiling that does not care how many threads exist.

Cargo has no built-in answer to any of this
([rust-lang/cargo#12912](https://github.com/rust-lang/cargo/issues/12912),
[#9157](https://github.com/rust-lang/cargo/issues/9157),
[#11707](https://github.com/rust-lang/cargo/issues/11707),
[#9735](https://github.com/rust-lang/cargo/issues/9735)); no cargo plugin solves it, so the choice
is a job object or nothing. procgov is optional: an absent tool degrades the protection but never
breaks the build.

`-r` (recurse the job object onto every descendant) is required, not optional: without it the
limits apply to the launched process alone, and every `rustc`/`link.exe` it spawns — where all the
resource use actually is — escapes the job entirely. It also makes procgov wait for the whole tree,
so orphaned compilers cannot outlive the run it belongs to.

**Options beyond the two default ceilings**, both opt-in because they are unmeasured against the
default:
- `-CpuCores` claims a fixed *count* of logical processors instead of throttling a *rate* across
  all of them. On a hybrid CPU this can leave whole physical cores genuinely uncontended for a
  latency-sensitive daemon, which in principle beats a rate limit (a rate-limited job still
  schedules threads onto every core between throttle windows) — but whether it actually helps
  daemon latency in practice is unmeasured, so it stays opt-in rather than becoming the default on
  a hunch. It is mutually exclusive with `--cpurate`, not additive: procgov applies a rate only to
  the selected cores when both are set, compounding into a harsher cap than either flag states.
- `-EfficiencyMode` (procgov `--efficiency-mode`, i.e. Windows' EcoQoS /
  `PROCESS_POWER_THROTTLING_EXECUTION_SPEED`) pushes compiler work off P-cores onto E-cores on a
  hybrid CPU. It costs build wall-clock, which is why it is opt-in.

## Process priority and sccache

Dropping the launched process to `BelowNormal` alone does not cover the actual compiler fan-out
when `RUSTC_WRAPPER=sccache` is set: cargo does not exec `rustc` directly, it invokes a short-lived
sccache *client*, which hands the compile to the long-lived sccache *server* daemon, and the server
spawns `rustc`. Windows priority inheritance gives those `rustc` processes the *daemon's* priority
class, not cargo's — so the daemon must be re-primed to `BelowNormal` on every run, after
`Test-SccacheHealth` has ensured the server is actually up (its `--show-stats` call is what starts
it) and before cargo itself starts (priority is fixed at spawn time; an already-running `rustc`
keeps the class it was born with).

`BelowNormal` on the launched process's whole tree, rather than each descendant individually,
because Windows propagates it for free: `CreateProcess` gives a child `NORMAL_PRIORITY_CLASS` by
default *unless* the creating process is `IDLE` or `BELOW_NORMAL`, in which case the child inherits
the parent's class. Setting it once on the root process therefore reaches every descendant this
script never even sees.

`Set-SccacheServerPriority` is called unconditionally, deliberately not skipped when `-Priority` is
`Normal`: the sccache server is long-lived and shared across every build on the machine, so whatever
priority one run leaves on it persists into the next. A `Normal` early-out would let one
`-Priority Idle` run strand the daemon at Idle indefinitely, silently keeping a later
`-Priority Normal` run — someone explicitly asking for full speed — compiling at Idle too. Setting it
unconditionally makes the daemon track whichever priority was actually requested, in both directions.

## Resource slots: mutexes, not a counted semaphore

See `CLAUDE.md`'s "What is scoped to the PC, and what is scoped to the worktree" section for the
full incident history. The mechanism: N named Windows mutexes (`Global\PanGlossBuildSlot0..N-1` and
`Global\PanGlossRunSlot0..M-1`), not one counted semaphore. A semaphore's count is not restored when its holder dies, and in agent
workflows the holder dies constantly (tool timeouts, agent stop/resume, detached invocations whose
parent conversation has gone) — any critical section between acquire and release will eventually be
interrupted. A mutex cannot leak that way because the kernel owns the cleanup: a holder that exits
without releasing leaves the mutex *abandoned*, and the next waiter is granted ownership
(`AbandonedMutexException`, carrying the abandoned index). Catching it and continuing *is* the
recovery — no ledger, no sweep, no hand-repair procedure.

A mutex also fixes a semaphore wart that was measured failing: a semaphore's maximum is frozen by
whichever process creates it first and cannot be queried or changed afterward. A mutex's "slot
count" is simply how many names a caller waits on, so a caller asking for 1 slot genuinely cannot
take a second one, regardless of what any other caller believes the limit to be.

The slot-holder ledger (`Write-SlotHolder`/`Get-SlotHolders`, one directory per pool) is diagnostic
only. It never decides whether a slot is free — the mutexes are the exclusion — and exists only so a
waiter or `doctor` can name who holds each slot instead of blocking anonymously; a stale entry from a
killed holder is expected and reported as NOT ALIVE rather than trusted. The pools keep separate
ledger directories so a `run` is never reported as occupying a build slot, which is the confusion the
split exists to end.

Sizing `Get-JobMemoryCapGB` for `MaxConcurrent + 1`, not `MaxConcurrent`, is a correctness fix, not
padding: it keeps the per-build memory bound true even through one slot of over-admission from a
caller passing a different `-MaxConcurrent` than the mutex names actually in use (a semaphore-shaped
failure mode that briefly recurred even after the mutex migration, since nothing stops two callers
disagreeing on how many names to wait on).

## Two pools: builds and runs queue separately

`Enter-ResourceSlot -Pool build|run` replaces the single `Enter-BuildSlot` queue (which survives as a
front end onto the build pool). One queue for both was measured costing real time for no resource
reason: a 0.3s `pangloss parse` waited behind two multi-minute builds in another worktree, and six
such parses queued six separate times.

The split is justified by what each side is actually bounded by. A build is bounded by disk (a
target dir) and by memory; a run writes no target dir, and — for the light shape this pool is sized
for — barely moves memory either. What a run can genuinely exhaust is CPU. So the two pools share
**one** machine-wide core budget and are otherwise independent.

### The CPU budget is decomposed, not duplicated

`Get-JobCpuRatePercent -Threads N` sizes procgov's `--cpurate` from *one slot's own width* instead of
the whole machine's usable width. Without `-Threads`, every concurrent job requested the entire
machine-wide figure, so two builds could request 140% between them — a live bug that predated the run
pool, and the highest-value part of this change. With it the shares sum back to the same
machine-wide figure they were each individually claiming:

| | width | `--cpurate` | slots | total |
|---|---|---|---|---|
| build | `Get-CargoJobBudget` | ⌊width/logical × 100⌋ | 2 | — |
| light run | 1 core | ⌊1/logical × 100⌋ | 4 | — |
| | | | | ≤ machine-wide ceiling |

The sum closing on the pre-existing global figure is the point: this is a decomposition of a number
already in the design, not a new policy. `rust/tools/tests/memory-reserve.tests.ps1` pins the
one-sided bound (per-slot rounding can only lose percent, never gain it).

### The light-run memory cap is flat, and that is measured

`Get-RunJobMemoryCapGB` returns a flat 2GB (`PANGLOSS_RUN_MEM_GB`), deliberately *not*
`Get-JobMemoryCapGB`'s machine-proportional derivation. A runaway is recognizable by absolute size; a
share of the box would make the same binary legal at 8GB on one machine and refused at 2GB on
another.

The number rests on a measurement of the HermitCrab engine over the Sena grammar
(`--engine=default --threads 1`), rather than on the intuition that the search is a churn machine:

| corpus | `--memo=on` peak | `--memo=off` peak | wall (on / off) |
|---|---|---|---|
| 1,000 words | 189 MB | 184 MB | 12s / 21s |
| 3,000 words | 454 MB | 458 MB | 33s / 75s |
| 6,146 words (all) | 454 MB | 458 MB | 62s / 132s |

Three things this settles. **Peak does not grow with corpus size** — 3,000 → 6,146 words adds
nothing, because peak is set by the hardest single word and the 3,000-word slice already contains it.
So a flat cap is the right shape and not a corpus-size limit in disguise. **The memo table is not the
driver**: disabling it moves memory by <1% while doubling the runtime, so `--memo=on` buys a 2.1×
speedup on the full corpus for no measurable memory. And **peak committed ≈ peak working set** (190
vs 186 MB on the 1,000-word run), so the general hazard that `--maxjobmem` bounds commit rather than
live set — a Rust `Drop` returns pages to the allocator, not necessarily to the OS — does not bite
here; this allocator is not hoarding decommitted pages. 2GB is ~4.5× the measured peak: headroom for
an unmeasured grammar, tight enough that hitting it is a signal worth chasing with the
`dead-end-census` skill.

A run that legitimately needs more is not a light run. `-Heavy` puts it in the **build** pool with a
build's ceilings — the right home for a `predict_census`-shaped probe — and `-RunMemoryGB` overrides
the cap for one invocation without touching `PANGLOSS_JOB_MEM_GB`, which would change every ordinary
build's cap for as long as it stayed set.

### Transition hazard

Same shape as the semaphore → mutex migration: while some worktrees still run the old code, their
`run` takes a *build* slot while a new-code worktree's takes a *run* slot, so the two do not exclude
each other for runs. Tolerable only because the per-job memory and CPU ceilings above are sized for
the full six-slot worst case regardless.

## Orphan process reaping

Two families, with different tolerances, because the two failure directions are not symmetric. A
build in *another* worktree, running on the *same* machine, must be indistinguishable from
untouchable — so liveness is decided by `Test-ParentAlive`, never by process name, age, or CPU
burned.

`Test-ParentAlive` guards two distinct false-positive shapes:
1. **PID reuse.** A dead parent's PID can be recycled by an unrelated new process; a bare
   "does this PID exist" check would then report the orphan as parented and skip it. A candidate
   parent is only accepted when it was created *before* the child — a process that started later
   cannot be the thing that spawned it.
2. **Access-denied misread as dead.** `Get-Process -Id` reports failure both for "gone" and for
   "exists but I can't see it" (a different session, different elevation). "I could not look" must
   never read as "it is dead" — that is exactly the false positive that would reap a healthy build
   in another worktree. A CIM process snapshot answers existence uniformly.

Compilers (`rustc`/`cargo`/`link`/`cc1`) and scanners (`find`/`rg`/`grep`/`findstr`) are reaped by
separate functions with different thresholds, on purpose: an orphaned `rustc` has at least produced
object files on disk, so there is real, if incomplete, output to weigh against killing it. An
orphaned scanner has produced nothing but a closed pipe whose reader has already exited — there is
no salvageable output at all, so a scanner can be reaped on CPU-and-age thresholds (both must be
crossed) that would be too aggressive to apply to a compiler. The scanner name list is a single
named constant precisely because the entire safety argument for reaping scanners rests on no Rust
build binary ever appearing in it.

## Split-ExtraArgsSpec: the PowerShell `-File` binder trap

`pwsh -File script.ps1 -Mode test -- --nocapture` fails at parameter-binding time ("the parameter
name '' is ambiguous"): under `-File`, the bare `--` reaches the parameter binder, which reads it as
a parameter with an empty name, rather than being consumed by PowerShell's own parser the way it is
when a script is invoked via the call operator (`& .\script.ps1 ... -- ...`). Dropping the separator
is not a safe workaround: a single-dash argument meant for the wrapped tool that happens to
prefix-match a script parameter binds to that parameter silently instead of reaching the tool — this
repo measured `-p foo` intended for cargo binding to a script's own `-Package` instead. An
environment-variable channel (`PANGLOSS_EXTRA_ARGS`, tokenized by `Split-ExtraArgsSpec`) is immune
because it never passes through the parameter binder at all.

## What `doctor` gates on, versus what it only reports

`doctor`'s unsafe/exit-code decision folds in disk, memory, worktree-base, sccache health, and the
conformance submodule -- all five describe the environment RIGHT NOW. It deliberately does NOT fold
in Resource-Exhaustion-Detector history: that describes something that already happened and the
machine already recovered from on its own, so failing doctor on a week-old incident would block
every managed build for the whole lookback window for no actionable reason. History is reported
prominently (loud enough that it isn't scrollback) but never gates the exit code -- the same
distinction this document draws elsewhere between "something is wrong right now" and "something bad
happened once."

## Resource-exhaustion event log

Windows already diagnoses a low-commit condition and logs it:
`Microsoft-Windows-Resource-Exhaustion-Detector` fires event ID 2004 into the System log naming the
top few processes by committed bytes. `Get-ExhaustionConsumersFromMessage` parses that message text
best-effort only — Microsoft publishes no stable grammar for it, so a message shape this repo has
not seen must degrade to an empty parse, never a thrown error; the caller keeps the raw message text
for a human regardless of whether this parses it.

`Get-WinEvent` throws, rather than returning an empty collection, both for "genuinely nothing in
this window" (the normal, good-news case) and for "could not query at all" (provider absent, access
denied). Those are not the same fact and must not be reported identically — the first is fine, the
second is "I don't know" and must never be silently upgraded to "fine." The only signal available to
tell them apart is `Get-WinEvent`'s own exception message text, which is why the distinction is made
by matching on it rather than on a dedicated exception type (there isn't one).

## Conformance submodule: sparse, path-scoped auto-init

`machine` (`sillsdev/machine`, `conformance-framework` branch) is a git submodule this repo's
default, non-`#[ignore]`d test suite reads fixtures from, but only from `machine/conformance`
(under 1MB) — never the rest of a full checkout (415MB, mostly `machine/src` and `machine/tests`).
A worktree many builds and worktrees deep on this machine pays that 415MB per worktree for data
nothing ever reads, so `Initialize-ConformanceSubmodule` materializes only the `conformance/`
subtree via a cone-mode sparse checkout, using a `--separate-git-dir` clone (into the same
worktree-scoped `modules/` location `git submodule update` itself already uses per linked
worktree, so two worktrees never contend for one submodule gitdir) followed by
`sparse-checkout init --cone` / `sparse-checkout set conformance` / `checkout <pinned SHA>`. The
pinned SHA is read from the superproject's own tree (`git ls-tree HEAD -- machine`), never from
`.gitmodules`' branch name or a live remote — the branch can move, and the tree entry is the exact
commit this checkout is pinned to regardless of where the branch has since drifted.

`git submodule update --init --no-checkout` is not valid syntax for this — `--no-checkout` is not
a recognized flag of `submodule update`, only of `clone` — which is why the recipe above builds the
clone and sparse-checkout steps by hand rather than through `submodule update`'s own flags.

A fast idempotent sentinel check (`machine/conformance/constructs.txt` present) runs before any git
invocation at all, so the common case — already initialized — costs exactly one `Test-Path` call. If
the sparse path fails on some git version or environment, the design falls back to a full checkout
rather than leaving the submodule half-initialized: a working 415MB checkout beats a broken clever
one. See `CLAUDE.md`'s own section on this submodule for the original incident (every fresh worktree
failing a conformance gate until someone ran `git submodule update` by hand) and the full command
sequence.

## Direct-binary invocation shares the same ceiling

`Invoke-ProcessInJobObject` is the procgov-wrapping core shared by every managed cargo invocation
*and* `pg.ps1 -Mode run` (an arbitrary already-built binary, or `cargo run --example`/`--bin`). This
closes a real gap: every incident that took the machine to a frozen, unreachable state was a single
PanGloss binary invoked *directly*, never through cargo, so nothing that only wrapped cargo could
ever have bounded it. `run` still takes a slot deliberately: the safety property the rest of this
design relies on — at most `MaxConcurrent + RunSlots` operations share the machine's headroom at
once — only holds if every one of them, including a long-running probe, counts against a slot. The
alternative (`run` counting against nothing) would make it an unbounded extra consumer on top of
`MaxConcurrent` full-cap builds — precisely the "several things assume they have the whole machine's
headroom simultaneously" shape that caused the incidents in the first place.

What changed is *which* slot: a light run takes one of the run pool's, not one of the two build
slots, because the resource it competes for is CPU and that is now budgeted across both pools (see
"Two pools" above). The cost this removes is a `run` occupying a *build* slot for hours; the cost it
keeps is that a long `-Heavy` probe still can, so a build queued behind one can hit
`-BuildSlotTimeoutSeconds` and exit needing a retry — a known, loud, recoverable cost, weighed
deliberately against an unbounded machine-wide worst case.
