# Agent build loop

Use a fresh PowerShell process for every managed Rust command:

```powershell
pwsh -NoProfile -File rust/tools/pg.ps1 -Mode check -Package <package>
```

Do not call `pg.ps1` with `&` from a reusable PowerShell host. The managed script sets build
environment variables for its child processes; process isolation prevents one worktree's target
directory or cache experiment from reaching the next invocation.

Run one PowerShell test file in a fresh process, or use the aggregator that already gives every
file its own process:

```powershell
pwsh -NoProfile -File rust/tools/tests/<name>.tests.ps1
pwsh -NoProfile -File rust/tools/tests/run-all.ps1
```

Running several `*.tests.ps1` scripts with `&` in one host contaminates later fixtures with global
functions and environment from earlier files. A combined-host failure is not a suite result.

## Classify before retrying

`helper_unknown_error: setup refresh had errors` occurs before PowerShell starts. It is a workspace
launcher failure, not a Cargo, test, or repository failure. Retry the exact command once with the
execution tool's `require_escalated` sandbox permission. Give delegated agents that recovery in
their task. If the escalated retry also fails, report the launcher as blocked; do not change
repository code, start another build, or count the event against a code hypothesis.

When a command returns a running session or times out after launch, inspect its Cargo, rustc,
procgov, and slot descendants before retrying. A timeout never authorizes a duplicate build.

## Shared build state

Measure physical-memory and commit headroom, CPU load, and active Cargo/procgov trees before a
build-heavy handoff. Use `pg.ps1` for all Rust work and let its shared slots govern concurrency.

The sccache daemon is machine-wide. Before a stop, restart, configuration experiment, or cache
counter reset, verify that no managed build or run slot can be using it. If another slot is active,
wait or use an isolated server; do not disrupt the shared daemon. Preserve cache objects unless the
experiment explicitly requires eviction, and distinguish a warm Cargo target (zero compiler calls)
from an sccache hit.

Record the exact command, target directory, Cargo time, wrapper time, and relevant cache counters.
A mechanism is accepted only by its observed effect.
