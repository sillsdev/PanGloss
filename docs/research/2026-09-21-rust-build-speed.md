# Rust build speed investigation

Worktree: `.worktrees/rust-build-speed`, branch `perf/rust-build-speed`.
Baseline commit: `e13e3a990b37241be5694b7892988552aca414c6`.
The main checkout's uncommitted work is excluded from this baseline.

## Objective and measurement contract

Reduce developer feedback latency without losing test coverage or weakening process
containment. Measure fresh-worktree check, representative linked test, and unchanged
repeat separately. Fresh-worktree output is not a globally cold compiler cache.
Record wall time, Cargo time, queue/preflight cost, profile, target storage, cache
state, concurrent activity, and test result. Compare one variable at a time.

All Rust work uses `rust/tools/pg.ps1`. Existing caches and other workers remain
intact. Initial admission permits one investigation build alongside one existing
managed build; existing memory and CPU limits remain in effect.

## Initial evidence

- 20 logical processors (Intel i7-12700); initial CPU reading 8%.
- Initial physical free memory about 44 GiB; committed memory 32.26 GB of
  72.72 GB, approximately 40.46 GB commit headroom.
- One other managed build was active: `rich-trace-json-min`, pg-cli test targets,
  `--maxjobmem=19G --cpurate=25`.
- C: free space about 28.6 GiB, below the 50 GiB SSD reserve in
  `rust/tools/_common.ps1`. Target selection therefore falls back to G:.
- The default test profile uses optimization level 3, ThinLTO, 16 codegen units,
  and line-table debug information. `build` defaults to the fat-LTO release profile.
- Worktree creation initialized only `machine/conformance`; actual directory
  inspection confirmed this, and the new checkout started clean.

## Hypotheses and discriminating experiments

| Question | Discriminating evidence | Status |
|---|---|---|
| Does per-binary ThinLTO dominate linked tests? | Same representative target and workload with LTO on/off; distinguish rebuild from unchanged repeat | Inconclusive: changed-profile rebuild is confounded |
| Is HDD placement limiting progress? | Cargo timing plus observed disk activity; controlled SSD comparison only with adequate space | Placement confirmed; effect unmeasured |
| How much work survives a new worktree? | Fresh output versus warm repeat, compiler cache statistics and cacheability | 297 dirty check units; shared-cache reuse not isolated |
| Do defaults compile much more than the edit requires? | Actual target inventory and dependency graph; narrow target timing | Target inventory confirmed; use narrow package/target compilation |
| How much latency occurs before Cargo? | End-to-end wall time minus reported Cargo time | Confirmed: 13.3 seconds hygiene in a 19.38-second warm repeat |

## Parallel research

Three Luna researchers at xhigh effort cover the codebase/dependency architecture,
fresh build measurements, and current primary-source online guidance. The primary
agent reviews their evidence and owns the final recommendations.

## Source-backed constraints

- `lto = false` can retain local ThinLTO; `lto = "off"` disables it completely.
  [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html).
- Integration targets are separate executables; consolidation must preserve the
  test inventory and supported runner behavior.
  [Cargo targets](https://doc.rust-lang.org/cargo/reference/cargo-targets.html).
- Nextest normally runs each test in its own process. Do not apply libtest's
  concurrency model to nextest; also verify the supported libtest fallback.
  [Nextest execution model](https://nexte.st/docs/design/how-it-works/).
- sccache cannot cache system-linker invocations or incremental Rust compilation;
  its supported emit modes require `link`. Check-only work and executables cannot
  be assumed cache hits.
  [sccache Rust caveats](https://github.com/mozilla/sccache/blob/main/docs/Rust.md).
- LLD supports Windows PDB output, but changing the final linker does not remove
  ThinLTO optimization work inside rustc.
  [LLD Windows support](https://lld.llvm.org/windows_support.html).

## Wrapper overhead to quantify

`Wait-ManagedProcessTree` in `rust/tools/_common.ps1` polls every ten seconds and
unconditionally sleeps after inspecting the process tree. This can delay observing
completion. Compare Cargo time with wrapper wall time before changing this safety
mechanism; preserve liveness checks and lingering-helper cleanup.

The follow-up experiment confirmed this hypothesis. With the same governed `pwsh`
payload sleeping for one second and a ten-second liveness interval, the original path
took 10.912 seconds while an exit-aware timed wait took 1.669 seconds, saving 9.243
seconds (84.7%) in this deliberately worst-phase probe. The replacement still wakes
at the configured interval when the process remains alive, so tree snapshots, idle-wedge
detection, helper reaping, job containment, and their thresholds are unchanged. The
25-test managed-process suite, including real procgov falsification, passes.
A subsequent warm representative pg-foma target still took 13.062 seconds (hygiene
2.7 seconds, Cargo 0.13 seconds, four tests passing). Pre-Cargo variability therefore
prevents attributing an end-to-end saving from that single run; the causal claim is
limited to the controlled wait-seam experiment above.

`pg.ps1` checks workspace-wide formatting even for narrow package checks. Test/build
also run comment hygiene; broad builds additionally run the backend card generator.
These phases belong in end-to-end measurements, not compiler/linker time.

The rustfmt follow-up measured 549 Rust files and 247,036 lines; no generated or foreign
Rust tree was found that justified an ignore rule. Directly parallelizing arbitrary files
was rejected because rustfmt can traverse out-of-line modules, and direct invocation does
not inherit each Cargo package's edition and style edition. The local managed path instead
establishes one full-workspace clean baseline per commit/tool identity, keys clean results
by the contents of all Rust sources, manifests, formatter/Cargo configs and toolchain files,
and formats only the owning Cargo packages when subsequent changes are exclusively `.rs`
files. Unknown ownership, changed manifests/config/toolchains, Git-query failure, or an absent
baseline falls back to the full workspace. Formatter failures and concurrent input changes
publish no clean result. Release and both CI workflows retain `cargo fmt --all -- --check`.

The unchanged managed `pg-comment-hygiene` check originally took 4.955 seconds. Five warm
cache-hit repeats had a 2.326-second median (2.205–2.687 seconds), with Cargo itself at
0.06–0.07 seconds. A deliberately misformatted source file was repaired through its one
owning package and the final hardened managed check completed in 3.293 seconds (rendered as
3.29 seconds); the normalized Git blob was identical to HEAD afterward. The final hardened
first full baseline-publication run took 5.148 seconds (rendered as 5.148 seconds), so this
optimization targets repeated local feedback, not first use or authoritative CI.

Primary behavior references: [cargo-fmt package/all strategy](https://github.com/rust-lang/rustfmt/blob/main/src/cargo-fmt/main.rs)
and [rustfmt invocation, module traversal, editions and check semantics](https://github.com/rust-lang/rustfmt).

## Reviewed baseline observations

The first Cargo HTML report recorded `pg-foma` all-target checking in 57.8 seconds,
297 dirty units, zero fresh units, five configured jobs, Rust 1.98.1 MSVC. Its largest
unit was `windows 0.57.0` at 23.4 seconds. The `pg-foma` library check was 5.9 seconds
and its test-library variant 6.3 seconds. Unit durations overlap; do not sum them or
equate one unit's duration with removable wall time.

Evidence: `G:/cargo-build-cache/rust-build-speed/cargo-timings/cargo-timing-20260921T235857439Z-b835b1c725425297.html`.

The representative target is `backend_selection_contract` (four tests). Wrapper
logs show comment hygiene taking 15.6 seconds on its first linked invocation and
13.6 seconds on its unchanged repeat. Those logs capture wrapper output; Cargo
child output is separate and must not be inferred from a missing log summary.

| Managed command / state | Cargo seconds | Wall seconds | Result |
|---|---:|---:|---|
| `check -Package pg-foma --timings`, fresh worktree output | 57.75 | 73.580 | Pass |
| Default representative linked test, first invocation | 83 | 113.371 | 4 passed |
| Default representative test, unchanged repeat | 0.20 | 42.138 | 4 passed |
| Process-local LTO off, different-profile rebuild | 255 | 284.251 | 4 passed |
| LTO off, unchanged repeat | 0.14 | 27.753 | 4 passed |
| Primary rerun, default profile unchanged | 0.13 | 19.380 | 4 passed |
| Native hygiene port, same default profile warm target | 0.13 | 6.979 | 4 passed |

The primary's baseline repeat independently confirmed test execution at 0.01 seconds and
hygiene at 13.3 seconds. The earlier 42.138 seconds is not a fixed wrapper floor.
Profile changes invalidate artifacts; the LTO-off row is not evidence that disabling
LTO slows compilation or speeds it up. No LTO defaults were changed. Shared machine
activity and variable preflight cost prevent causal conclusions from these single runs.

The user selected the native checker port as the next implementation slice; see
[its plan and verification](2026-09-21-comment-hygiene-port.md). The port still scans
all inputs on every call: only the executable is cached, not prior hygiene findings.

The completed port reduced the measured warm round trip from 19.380 to 6.979 seconds
(about 64%), with hygiene falling from 13.3 to 2.6 seconds. Cargo remained 0.13 seconds;
this improves preflight latency, not Rust compilation. Final identical-input full-tree
scans took 11.414 seconds in PowerShell versus 2.345 seconds natively, with no finding
or informational-count differences. A negative fixture also matched all 11 findings.
The native package, launcher/cache/classifier/verifier tests and independent review passed;
see the port report for exact coverage and first-use bootstrap caveats.

Filesystem inventory at the baseline has 117 top-level integration-test files in
`pg-foma`, 32 in `pg-parse`, 20 in `pg-rules`, and 13 in `pg-cli`. This is a source
inventory, not a claim that every command builds every target. Directory-based
targets and example test harnesses must also be counted before consolidation.

`sysinfo 0.32.1` enables component/disk/network/system/user/multithread by default.
PanGloss directly uses it in `pg-foma/src/backend_runtime.rs` and
`pg-cli/src/recipe_optimize.rs` for process sampling. A narrow dependency experiment
is to retain `system` and `multithread` while removing the other defaults, then
check both packages and verify sampling still returns usable measurements.
This has not yet been applied or claimed as a measured speedup.

## Candidate work, ordered by scope

1. **Use the narrow existing inner loop.** Type-check with `-Mode check -Package`;
   select an integration executable with `-Mode test -Package -TestTarget`.
   `-Filter` only selects executed tests. Keep full-suite validation as its own
   explicit gate. Existing `-Mode build` means fat-LTO release, not a cheap compile.
2. **Remove repeated wrapper work.** The exit-aware timed wait is implemented while
   retaining ten-second liveness inspections when work remains, stale-tree detection,
   helper cleanup, and kernel caps. Native full-tree hygiene is also implemented.
   Workspace rustfmt now uses an exact-content clean cache plus changed-package formatting
   after a proven full baseline. Separately measure repeated hygiene gates in release
   orchestration. Any future result reuse must
   invalidate on all scan inputs, including deleted linked documents.
3. **Prune unused dependency features.** Experiment with sysinfo's system and
   multithread features only. Verify actual process observations, both native
   consumers, and supported-platform builds before adopting it.
4. **Separate iteration optimization from release optimization.** Compare LTO off
   against ThinLTO on unchanged source and equivalent rebuilt target sets. Record
   test runtime as well as compilation; a slower runtime can erase build savings.
   Preserve production release settings and representative performance gates.
5. **Consolidate integration targets selectively.** Start with a small thematic
   group, keep source modules, record a mapping from old to new test identifiers,
   and prove the same tests run. Audit fixture paths, global state under libtest,
   per-binary nextest configuration, and direct binary consumers. Avoid a single
   giant test target that makes every small edit expensive again.
6. **Restore SSD target capacity.** The configured reserve currently forces HDD
   output. Measure a representative SSD comparison once space is available; do not
   lower the reserve or delete active/user data to manufacture a speed result.
7. **Measure alternative linker and incremental edits.** Try MSVC-compatible LLD
   only after determining final linking is material. Compare sccache reuse with
   local incremental compilation across real one-edit rebuilds. Do not clear the
   shared cache or interpret machine-wide cache counters as one build's results.
8. **Refactor crate boundaries only from invalidation evidence.** Identify which
   common edits rebuild pg-foma and its consumers. Extract a cohesive owner with a
   narrow interface only if that removes a measured dependency/rebuild path; moving
   Rust modules between files inside one crate does not create independent units.

No source/profile/default changes are implied by this list. Each candidate needs
its own acceptance evidence; the investigation retains current resource controls.

## Architecture findings and independent review

The codebase researcher found 220 direct integration-test source files workspace-wide.
The pg-foma example count is eight including `examples/lab`; prior work already
consolidated 38 examples into eight. The earlier report-module move changed warm
CLI build measurements from 77 to 75 seconds and did not demonstrate a linking win.
See [the existing measurements](compiler-interface-measurements-2026-09.md).
This rules out presenting either change as a new, untried improvement.

The two pg-foma check contexts both enable developer-tools/test-support; the
ordinary library and cfg(test) library are different artifacts. Their presence
alone does not prove accidental feature duplication. A shared dev-only test helper
library could remove repeated helper compilation across targets; another common.rs
included independently by many test binaries would not.

Consolidation hazards were checked in source: the fixture-wide no-panic test and
candidate-filter tests mutate the process-wide panic hook. The f0 roundtrip test
uses a fixed temporary filename, which needs a concurrency audit. In-package
consolidation preserves CARGO_MANIFEST_DIR; extracting tests into another package
changes it. pg-cli tests use CARGO_BIN_EXE_pangloss, so moving those helpers into a
library requires passing the executable path explicitly.

The independent Sol/xhigh review approved this investigation sequence, not changing
defaults. Its adjustments are adopted:

- Prioritize measured hygiene overhead and completion observation latency.
- Preserve sysinfo multithread behavior while pruning unused feature groups.
- Require matched rebuilt states (A/B/A), not a cold/warm comparison, before claiming
  an LTO speedup. Four contract tests cannot establish workspace runtime performance.
- Preserve ignored tests, corpus coverage, nextest filters and focused-target usage
  when consolidating; verify libtest with one and multiple threads.
- Do not silently change build's backward-compatible release behavior. First factor
  profile/command/label/resource-weight decisions into one owner, then design an
  explicit iteration mode or a documented migration.
- Restore SSD capacity as an operational task and measure it; HDD placement alone
  is not proof of an I/O bottleneck. A single in-flight observation showed no disk
  queue, which likewise does not exclude intermittent I/O stalls.
