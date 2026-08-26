# Linux delegated-root containment correction implementation plan

> **For agentic workers:** Execute this plan task by task with a separate test-author commit,
> implementation commit, and review fixes. Do not revive the rejected current-cgroup-parent design.

**Goal:** Prove Linux aggregate worker-tree containment beneath an explicit empty delegated root,
including complete failure cleanup, and provide an honest repository-approved Linux test path.

**Architecture:** The host supplies `PANGLOSS_CGROUP_DELEGATED_ROOT` as an absolute cgroup hierarchy
path. The supervisor must already occupy a strict child leaf, while each worker is created as a
sibling directly below the empty root and enters it atomically through
`clone3(CLONE_INTO_CGROUP | CLONE_PIDFD)`. Linux `pg.ps1` runs Cargo only when the host has already
placed the build in a finite cgroup; it does not duplicate the worker launcher or offer bare-Cargo
fallback.

**Tech stack:** Rust 2024, libc Linux syscalls, cgroup v2 kernel files, PowerShell 7, GitHub Actions,
the existing `pg-worker-containment` safe API, and `rust/tools/pg.ps1` as the sole Rust entry point.

## Fixed decisions and deletion boundary

- Delete the assumption that `/proc/self/cgroup` identifies writable worker-parent authority.
- Do not search upward, strip a specially named leaf, enable controllers, use process groups as
  containment, move an already-running child, or add a spawn fallback.
- Do not add a named execution envelope. The delegated-root variable is host authority only; the
  existing configurable payload, memory, and wall limits remain the only build limits.
- Tests change before implementation. A red test that encodes the ratified contract is evidence to
  repair production code, not a reason to restore the rejected behavior.
- Protect the accepted Windows adapter and public safe API. This slice does not yet route
  `pg-foma::run_compile_worker`; that deletion remains Task 5 of the parent plan.
- Do not merge the rejected Linux branch wholesale. Reuse only source that survives line-by-line
  inspection against this plan.

## Task 1: Rewrite the Linux contract around explicit delegation

**Files:**

- Modify: `rust/crates/pg-worker-containment/tests/linux_containment.rs`
- Read only: `rust/crates/pg-worker-containment/src/bin/containment_test_child.rs`

1. Replace `optional_cgroup_relative_path`, first-mount selection, and
   `child_starts_in_current_unified_cgroup_on_its_first_action` with test helpers that read the
   configured hierarchy root and map it through the most-specific matching visible cgroup2 mount.
   The central assertion must have this shape:

   ```rust
   let root = configured_delegated_root()?;
   let current = unified_membership("/proc/self/cgroup")?;
   assert!(current.starts_with(&(root.trim_end_matches('/').to_owned() + "/")));
   assert!(read_to_string(mapped_root.join("cgroup.procs"))?.trim().is_empty());
   assert_eq!(worker_parent(child_membership), root);
   assert_ne!(child_membership, current);
   ```

   Canonical path validation must reject an empty value, a relative value, `.` or `..` components,
   duplicate separators, and a trailing slash except for `/`.

2. Add serialized environment-contract tests under `ENVIRONMENT_LOCK`:

   - absent `PANGLOSS_CGROUP_DELEGATED_ROOT` returns `ContainmentError::Unavailable`;
   - malformed roots return `Unavailable` without creating `.pangloss-worker-*` residue;
   - a configured root that is not an ancestor of current membership returns `Unavailable`;
   - a populated configured root returns `Unavailable`;
   - non-required hosts may skip only after a truthful `Unavailable`; required mode must panic.

   Restore the original environment exactly with a small guard even when an assertion unwinds.

3. Strengthen `missing_executable_returns_typed_spawn_failure_without_fallback`. Snapshot direct
   `.pangloss-worker-*` children of the mapped delegated root before launch and assert the same set
   afterward. Keep the initiating failure typed, and require any cleanup failure to be the returned
   top-level error while retaining the exec diagnostic in its detail.

4. Keep the existing success, argv/env/cwd/stdio, descendant kill, aggregate memory, direct-child
   crash, fork race, signal exit, and final removal tests. Change only their setup so every capable
   run proves the explicit-root topology.

5. Format and compile the test contract, without editing production files:

   ```powershell
   rust/tools/pg.ps1 -Mode test -Package pg-worker-containment -TestTarget linux_containment -NoNextest -MaxConcurrent 1 -Jobs 2 -TestThreads 1
   ```

   The wrapper runs comment hygiene and rustfmt before Cargo. On Windows, the target-specific test
   must compile cleanly and run zero Linux tests. On the designated Linux runner, record the
   intended red failures. Commit only the test rewrite.

## Task 2: Resolve and validate the delegated root

**Files:**

- Add: `rust/crates/pg-worker-containment/src/linux.rs`
- Modify: `rust/crates/pg-worker-containment/src/lib.rs`
- Modify if dependency features require it: `rust/crates/pg-worker-containment/Cargo.toml`
- Modify if dependency resolution changes it: `rust/Cargo.lock`

1. Add `#[cfg(target_os = "linux")] mod linux;`, restrict `unsupported` to neither Windows nor
   Linux, and preserve the existing safe exported API. Update Linux documentation only where the
   Windows-only wording is now false.

2. Implement `DelegatedRoot::resolve()` around the explicit variable:

   ```rust
   struct DelegatedRoot {
       hierarchy_path: PathBuf,
       directory: OwnedFd,
   }

   impl DelegatedRoot {
       fn resolve() -> Result<Self, ContainmentError> {
           let configured = parse_absolute_canonical_hierarchy_path(
               std::env::var_os("PANGLOSS_CGROUP_DELEGATED_ROOT")
                   .ok_or_else(|| unavailable("PANGLOSS_CGROUP_DELEGATED_ROOT is required"))?,
           )?;
           let current = read_unified_membership("/proc/self/cgroup")?;
           require_strict_descendant(&current, &configured)?;
           let mount = most_specific_covering_cgroup2_mount(&configured)?;
           let root = open_mapped_root_without_symlink_following(&mount, &configured)?;
           require_empty_root_and_memory_delegation(root.as_raw_fd())?;
           Ok(Self { hierarchy_path: configured, directory: root })
       }
   }
   ```

3. Parse every cgroup2 mountinfo record, including escaped fields and mount roots. Select the
   longest mount root that component-wise contains the configured hierarchy path. Reject zero
   matches, tied ambiguous mappings, traversal, symlink substitution, and current membership that
   is equal to rather than below the configured root.

4. Require the root's `cgroup.procs` to be empty and `memory` to be present in
   `cgroup.subtree_control`. Open authority relative to the validated root directory descriptor;
   never rediscover or ascend from the current leaf.

5. Create a generated `.pangloss-worker-<pid>-<counter>` directly beneath that root. Require and
   configure the worker surfaces named in the design. Read back `memory.max`, accept page rounding
   only when the effective value is positive and no greater than requested, and treat missing
   `memory.swap.max` as the one documented optional surface.

6. Make `read_events` require exactly parseable `max` and `oom_kill` keys. Missing, duplicate,
   malformed, or overflowing values are failures; never silently substitute zero.

7. Preserve the audited race-free mechanics from the rejected branch only after inspection:
   prebuild argv/environment/cwd and pipes, call `clone3` with both flags, perform only raw
   no-allocation operations in the child, preserve Unix environment case sensitivity, and report
   signal termination distinctly.

8. Run the Windows-safe compile/gate before committing:

   ```powershell
   rust/tools/pg.ps1 -Mode test -Package pg-worker-containment -NoNextest -MaxConcurrent 1 -Jobs 2 -TestThreads 1
   ```

## Task 3: Make every Linux exit path perform complete bounded cleanup

**Files:**

- Modify: `rust/crates/pg-worker-containment/src/linux.rs`
- Modify tests first if a newly found edge lacks a Task 1 assertion:
  `rust/crates/pg-worker-containment/tests/linux_containment.rs`

1. Introduce one cleanup state machine used by launch failure, explicit lifecycle calls, and
   `Drop`. It must continue after individual failures:

   ```rust
   fn cleanup(&mut self, deadline: Instant) -> Result<CleanupEvidence, ContainmentError> {
       let mut failures = Vec::new();
       record(&mut failures, self.kill_tree());
       record(&mut failures, self.wait_tree_empty(deadline));
       record(&mut failures, self.reap_direct_child(deadline));
       let evidence = record_value(&mut failures, self.capture_final_evidence());
       record(&mut failures, self.remove_cgroup());
       finish_cleanup(failures, evidence)
   }
   ```

   Do not return early after `cgroup.kill`, populated wait, reap, evidence capture, or removal.

2. Use the caller's absolute deadline for owned API operations. Use a fixed five-second emergency
   deadline for launch errors and `Drop`; do not expose it as an execution-limit setting.

3. Define failure precedence explicitly: incomplete cleanup returns `ContainmentError::Failed` even
   when the initiating error was `Unavailable` or an exec failure. Include both the cleanup failure
   and initiating diagnostic in `detail`. When cleanup completes, return the initiating error
   unchanged.

4. Ensure parent pipe endpoints and child/error-pipe descriptors close on every branch. A missing
   executable must leave no direct child, descendant, readable pipe holder, or attempt directory.

5. Require final evidence capture while the cgroup still exists, then remove it only after
   `populated 0` and direct-child reap. Mark removal once so repeated cleanup and `Drop` are
   idempotent.

6. Run the same narrow command from Task 1 on the delegated Linux runner. All tests must execute;
   a capability skip does not count when `PANGLOSS_CGROUP_TEST_REQUIRED=1`. Commit cleanup changes
   separately from Task 2 when practical.

## Task 4: Add an honest Linux path to the managed Rust wrapper

**Files:**

- Modify: `rust/tools/_common.ps1`
- Modify: `rust/tools/pg.ps1`
- Add: `rust/tools/tests/linux-platform.tests.ps1`
- Modify only if extraction reduces risk: add `rust/tools/_platform_windows.ps1` and
  `rust/tools/_platform_linux.ps1`

1. Write PowerShell contract tests first. On Linux they must prove:

   - available/total/commit memory comes from `/proc/meminfo` with checked KiB-to-byte conversion;
   - a machine-wide build slot uses an exclusive file lock and releases on normal exit;
   - the current build is rejected before Cargo when no finite ancestral cgroup memory cap exists;
   - malformed/unreadable cgroup data fails closed;
   - Cargo exit codes and working directory are preserved inside a valid pre-contained host cgroup;
   - no test invokes real Cargo or touches a real cache/target directory.

   Existing Windows tool tests must remain unchanged and green.

2. Dispatch platform-native functions at `_common.ps1` load time. Keep Windows CIM, Job Object,
   procgov, named mutex, drive-space, and reaping behavior byte-for-byte where possible. The Linux
   implementation may share argument parsing, command construction, reserve arithmetic, and report
   formatting, but must not execute Windows P/Invoke declarations.

3. Parse `/proc/self/cgroup`, `/proc/self/mountinfo`, and ancestor `memory.max` files to prove the
   wrapper itself is already hierarchically bounded. Accept only a numeric finite effective cap;
   `max`, missing data, ambiguity, or parse failure stops before Cargo. Report the effective host cap
   in the preflight summary. This is the repository operational build cap, not the worker attempt's
   configurable 10 GiB limit.

4. Use an exclusive `FileStream` lock in a deterministic machine-wide path for `Enter-BuildSlot` on
   Linux. Store the owned stream in the returned token and dispose it in `Exit-BuildSlot`. Do not
   emulate the Windows named mutex or weaken `MaxConcurrent` semantics.

5. Invoke Cargo as a normal descendant only after the pre-existing host cgroup proof succeeds, so
   it inherits that host cap. Preserve signal/exit status and terminate/reap the complete descendant
   tree on wrapper interruption using the host's service lifecycle. Do not copy the worker cgroup
   launcher into PowerShell, use `Start-Process` as a containment substitute, call bare Cargo as a
   fallback, or introduce `systemd-run` ad hoc.

6. Run all PowerShell tool tests and the Windows Rust gate:

   ```powershell
   pwsh -NoProfile -File rust/tools/tests/run-all.ps1
   rust/tools/pg.ps1 -Mode test -Package pg-worker-containment -NoNextest -MaxConcurrent 1 -Jobs 2 -TestThreads 1
   ```

   Then run the Linux narrow gate through the new wrapper inside the configured host service.

## Task 5: Install the required-capability CI gate

**Files:**

- Modify: `.github/workflows/rust-ci.yml`
- Modify: `docs/superpowers/specs/2026-08-26-worker-process-tree-containment-design.md` only if the
  operational runner contract needs clarification, never to weaken it

1. Add a separate job on labels equivalent to
   `[self-hosted, linux, x64, cgroup-v2-delegated]`. Do not replace the generic workspace test job;
   it is not proof of delegated authority.

2. Require the runner service to provide:

   - cgroup v2 and permitted `clone3`;
   - an empty delegated root with `memory` enabled for children;
   - the runner/PowerShell/Cargo processes in a supervisor leaf;
   - `PANGLOSS_CGROUP_DELEGATED_ROOT` set to the root's hierarchy path;
   - a finite host-managed memory cap for the complete CI build;
   - writable worker surfaces including `cgroup.kill`, `cgroup.events`, `memory.events`, and
     `memory.peak`.

3. Set `PANGLOSS_CGROUP_TEST_REQUIRED=1` in the job and execute exactly:

   ```powershell
   ./tools/pg.ps1 -Mode test -Package pg-worker-containment -TestTarget linux_containment -NoNextest -MaxConcurrent 1 -Jobs 2 -TestThreads 1
   ```

4. Fail the job if the required root variable or host containment preflight is absent. Never turn
   this into a skipped green result. Record the runner provisioning contract beside the job rather
   than hardcoding a machine-specific hierarchy path.

## Task 6: Review, integrate, and record the proof

**Files:**

- Modify: `docs/superpowers/plans/2026-08-26-worker-process-tree-containment.md`
- Modify: `docs/simplification-rip-list.md`

1. Rebase the test-author commit onto the integration tip and inspect every changed assertion.
   Rebase the implementation only after the red contract is accepted. Resolve overlap by preserving
   the test contract, not by restoring implementation-shaped expectations.

2. Obtain two fresh Luna reviews on the exact candidate tip:

   - spec compliance: topology, authority, fail-closed launch, evidence, cleanup, and CI honesty;
   - code quality/safety: every unsafe block, descriptor ownership, allocation-free child branch,
     parser edge cases, deadline behavior, and Windows regression risk.

3. Fix every finding test-first. Commit review fixes separately and repeat both narrow platform
   gates. The primary agent personally inspects all diffs and reruns the authoritative commands.

4. Only after a required-capability Linux run executes all tests, mark parent Task 4 complete and
   update the cleanup ledger. Do not count the protected Windows adapter, raw protocol, or safe API
   as removed cruft. Record net additions/deletions from the cleanup baseline separately from this
   containment slice's local churn.

5. Proceed immediately to parent Task 5: route `run_compile_worker` through the safe owned process,
   then delete the old direct spawn/kill loop. Do not claim Stage 2 removal value before that
   deletion lands.
