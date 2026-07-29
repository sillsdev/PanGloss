# Categorical build and worktree hardening

## Status

Approved by the owner on 2026-07-29. Implementation must occur on branch
`categorical-build-hardening`, created from local `main` commit
`051ba2af1d1f61c3a57baa6f1e7e7e22cca49dac`. The primary checkout remains untouched while another
agent works there.

## Problem

PanGloss has useful build wrappers, but they are optional. Direct `cargo` commands bypass target
redirection, the shared compiler cache, disk reserves, concurrency limits, process cleanup, and
stale-cache collection. An observed agent build invoked Cargo directly, wrote to its worktree-local
`rust/target`, and called `rustc` without `sccache`.

Corpus-backed tests create a second false-success path. The real language inputs under
`samples/data/` are intentionally untracked. Many ignored tests return success when those files
are absent. A worktree can therefore claim that it ran the corpus suite while testing no corpus
data.

Agent-created worktrees create a third ambiguity. A request to inspect or build current local
`main` may materialize from an older session or remote snapshot. Written instructions tell agents
to check the base after creation, but tooling neither records the requested commit nor rejects a
mismatch before a build.

Parallel builds amplify these defects. A full Rust workspace test build emits hundreds of test
executables and Windows symbol files. The measured release tree contained 932 `.exe` files
(2.10 GB), 965 `.pdb` files (2.43 GB), and 1.76 GB of libraries and metadata. Debug information,
incremental state, profile and feature variants, and multiple target triples can grow a mature
debug tree to tens of gigabytes. Repeating that tree per worktree exhausted the system disk.

## Goals

The repository must make the safe path the normal path and make unsafe states fail visibly.

1. Every managed build uses a shared content-addressed compiler cache and a private, bounded
   target directory.
2. Corpus-required test modes fail before compilation when their declared inputs are absent.
3. Every managed worktree records and verifies its requested base commit before building.
4. Disk and concurrency budgets prevent parallel builds from exhausting the system drive.
5. Cleanup removes regenerable artifacts without deleting preserved release deliverables.
6. Build output reports the worktree, commit, target directory, cache state, corpus state, disk
   state, and acquired build slot.
7. CI and local verification test these controls without requiring private corpus data in CI.

## Non-goals

This change does not commit private real-language corpora, place active Cargo targets in one
shared directory, alter product behavior, or guarantee that an arbitrary user cannot invoke the
Cargo executable manually. It makes repository and agent workflows categorical: documented
commands, agent instructions, CI, and verification use the managed entry point, and the entry
point rejects unsafe state.

## Architecture

### One managed entry point

Replace the separate policy embedded in `rust/tools/build.ps1` and `rust/tools/test.ps1` with one
shared command model in `_common.ps1`. Keep the two user-facing scripts as thin build and test
front ends. Add explicit modes instead of inferring intent from arbitrary Cargo arguments:

- `build`: compile a package or workspace with the development build profile;
- `test`: run the fast fixture-independent suite;
- `corpus-test`: run named corpus gates and require their declared files;
- `release`: create an optimized deliverable or run an explicit performance gate;
- `doctor`: print configuration and fail on an unsafe or incomplete environment;
- `gc`: report or remove stale managed targets under strict ownership checks.

The scripts print a preflight record before starting Cargo. The record includes repository root,
worktree root and slug, `HEAD`, expected base, dirty state, Cargo profile, target triple, selected
target directory, free space, `sccache` path and health, corpus manifest result, and semaphore
slot.

`CLAUDE.md` and the applicable agent skill instruct agents to use these commands for all PanGloss
builds. They prohibit bare `cargo build`, `cargo test`, `cargo check`, and `cargo run` in agent
workflows. A repository verification test scans maintained agent instructions and build
documentation for the managed command contract so later edits cannot silently restore the old
guidance.

### Private targets, shared compiler results

Each worktree receives its own target directory. The directory key combines the repository ID and
a stable worktree identity, avoiding collisions between branches with similar leaf names. Cargo
may lock and mutate that directory without interference from another commit.

All worktrees share `sccache` at `G:\cargo-build-cache\sccache` by default. The cache stores
content-addressed compiler results, which are safe to reuse across worktrees. The managed command
sets `RUSTC_WRAPPER`, `SCCACHE_DIR`, and a canonical base directory before invoking Cargo. It runs
an `sccache` health check and prints statistics after the build. If `sccache` is installed but
cannot start or cannot write its cache, the command fails unless the caller explicitly chooses a
documented no-cache emergency mode.

The active target prefers the NVMe cache root while the system drive remains above its reserve.
It falls back to the capacious G: root when the reserve would be crossed. A preflight estimate uses
the requested mode and existing target size to reject a new build that cannot preserve the
reserve. This estimate is a guardrail, not a promise about final size.

The design does not share one live target directory. Although Cargo locks a shared target, builds
from different commits would serialize, invalidate fingerprints, and race over named final
binaries. Private targets preserve correctness; `sccache` supplies safe sharing.

### Profiles sized for their purpose

Broad tests must not use the final-deliverable release profile by default. Add a dedicated
optimized test profile derived from release but using thin or disabled LTO, multiple codegen
units, and stripped or reduced debug information. Keep fat LTO and one codegen unit for explicit
release deliverables and measurements that require production-equivalent code generation.

The fast suite builds only the targets it runs. It must not pre-build `--workspace --all-targets`
and then build hundreds of overlapping test binaries again. Package-scoped work uses
`-p <package>` unless a workspace gate is required.

### Corpus manifest and fail-closed gates

Add a committed corpus manifest that names each private fixture by logical corpus, relative path,
purpose, and tests that require it. The manifest contains no corpus content.

`corpus-test` resolves the repository's canonical corpus source. In the primary checkout that is
normally `samples/data/`. In a linked worktree the command may use a configured external corpus
root or stage files into the worktree with copy-on-demand. It validates every requested file
before Cargo starts and prints file names, sizes, and stable digests for reproducibility.

Corpus test helpers gain a required mode controlled by the managed command. When required mode is
active, a missing fixture panics with a precise message instead of returning success. Existing
self-skip behavior remains available only for ordinary fixture-independent CI and default tests.
Each corpus run must emit a machine-readable count of executed corpus cases. The front end rejects
a successful Cargo exit when the expected count is zero or incomplete.

CI tests the fail-closed behavior with a synthetic manifest and intentionally missing files. CI
does not need or receive private corpus data.

### Exact-base worktree contract

Add a worktree bootstrap command that accepts an explicit base revision. It resolves the revision
to a full object ID before creation, creates the worktree and branch from that object ID, and
writes a gitignored metadata file inside the worktree. The metadata records:

- repository identity;
- requested revision and resolved object ID;
- creation time;
- worktree path and branch;
- corpus source policy;
- managed target identity.

Every managed build compares `HEAD` and the recorded base before doing expensive work. A strict
base mode requires equality for read-only assessment tasks. A development mode allows descendant
commits but requires the recorded base to remain an ancestor. A mismatch fails with the expected
and actual IDs. The tool never checks out or rebases automatically because either action can
discard context or invalidate a useful build cache.

### Concurrency, ownership, and cleanup

A machine-wide semaphore limits expensive Rust builds across worktrees. The command acquires the
slot before disk-intensive work and releases it in `finally`. The configured limit is stable for
the machine session; callers cannot accidentally create incompatible semaphore capacities.

Every managed target contains an ownership marker with repository ID, worktree path, creation
time, last successful use, and preservation status. Garbage collection only touches directories
under configured cache roots whose markers match this repository. It resolves and validates each
absolute target before deletion.

Cleanup classifies artifacts:

- disposable: debug/test targets, stale worktree targets, incremental state, and orphaned build
  processes;
- preserved: explicitly registered release executables, packages, reports, and their provenance;
- unknown: unmarked directories, which `gc` reports but never deletes.

Before deleting a stale target, `gc` checks the current Git worktree registry and live Cargo,
`rustc`, linker, and `sccache` processes. Dry-run is the default. Destructive cleanup requires an
explicit apply switch and prints what it removed and whether it was regenerable.

## Error handling

Preflight failures occur before Cargo starts and use distinct exit codes for wrong base, missing
corpus, low disk, unavailable cache, invalid target ownership, and build-slot timeout. Error text
includes the failed condition and the safe recovery command.

Interrupted builds terminate their Cargo process tree and release the semaphore. The target
remains owned and eligible for later reuse or garbage collection. A failed build never registers
a release deliverable.

The no-cache emergency mode remains explicit, noisy, and incompatible with parallel managed
builds. A caller may use it when G: or `sccache` is unavailable, but disk and private-target
checks still apply.

## Testing

Implementation follows red-green-refactor.

PowerShell unit-style tests cover target identity, SSD/HDD selection, free-space rejection,
semaphore configuration, worktree metadata validation, corpus-manifest validation, ownership
checks, dry-run cleanup, and preserved deliverables. Tests use temporary directories and mocked
drive/process data; they never delete real caches.

Integration tests create temporary Git repositories and linked worktrees to prove:

1. exact-base creation records the resolved local commit;
2. strict mode rejects a different `HEAD`;
3. development mode accepts descendants and rejects unrelated history;
4. two worktrees select distinct targets and the same `sccache` directory;
5. a missing corpus fails before Cargo;
6. a complete synthetic corpus records a nonzero executed-case count;
7. stale marked targets are collectible while live, preserved, and unmarked directories survive.

A command-level smoke test uses a tiny temporary Rust workspace and a temporary `sccache`
directory. The second isolated worktree build must produce cache requests and at least one cache
hit or demonstrate reuse through a stable cache-stat delta supported by the installed `sccache`
version.

Repository verification runs formatting, affected PowerShell tests, the doctor command, a
package-scoped Rust smoke test, the fast workspace suite, a synthetic corpus gate, and a dry-run
garbage-collection report. Full private corpus gates run locally only when the manifest validates.

## Migration

The implementation first adds tests and the managed preflight without deleting existing wrappers.
It then converts `build.ps1` and `test.ps1` into thin front ends, adds worktree bootstrap and
doctor commands, and updates repository instructions. Finally, it migrates corpus helpers to
required-mode accounting and removes duplicated policy.

Existing local `rust/target`, `C:\cargo-targets`, and `G:\cargo-build-cache` directories remain
untouched during migration. The first `gc` run is dry-run only. The owner reviews its report before
any cache deletion.

## Definition of done

- A direct agent-workflow Cargo command is absent from maintained PanGloss instructions.
- Managed builds in two worktrees use distinct targets and the same healthy `sccache`.
- A requested corpus suite cannot exit successfully with missing inputs or zero executed cases.
- A worktree based on the wrong commit fails before compilation.
- Disk reserve and concurrency gates have deterministic automated tests.
- Garbage collection cannot escape configured roots or delete unmarked, live, or preserved data.
- The optimized test profile avoids final-release fat LTO while release deliverables retain it.
- Existing fast Rust tests pass.
- The design's synthetic end-to-end worktree, cache, corpus, and cleanup tests pass.
