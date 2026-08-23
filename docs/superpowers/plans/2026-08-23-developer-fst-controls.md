# Developer FST Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Quarantine unsafe FST controls to developer builds and separate correctness override from size-limit stress execution.

**Architecture:** Opt-in `developer-tools` Cargo features own all parsing, help, and library APIs for the experimental controls. A typed compile mode crosses the worker boundary; correctness trust, readiness, and containment remain separate values.

**Tech Stack:** Rust 1.90, Cargo features, serde worker protocol, foma, PanGloss managed PowerShell harness.

---

### Task 1: Production flag quarantine

**Files:**
- Modify: `rust/crates/pg-cli/Cargo.toml`
- Modify: `rust/crates/pg-foma/Cargo.toml`
- Modify: `rust/crates/pg-cli/src/main.rs`
- Test: `rust/crates/pg-cli/tests/developer_flags_contract.rs`

- [ ] **Step 1: Add a failing production-contract test**

Spawn `pangloss help`, `parse`, `batch`, `pack`, and `make-report` from the integration test. Assert
that default-build help omits all three spellings and that each command rejects
`--allow-unproven`, `--remove-size-limits`, and `--no-enforce-capability` as an unknown option.

```rust
for flag in ["--allow-unproven", "--remove-size-limits", "--no-enforce-capability"] {
    let output = Command::new(env!("CARGO_BIN_EXE_pangloss"))
        .args(["parse", grammar(), "word", flag])
        .output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown option"));
}
```

- [ ] **Step 2: Run the production test and observe failure**

Run: `& rust/tools/pg.ps1 -Mode test -Package pg-cli -TestTarget developer_flags_contract`

Expected: FAIL because current help exposes the switches and parsers accept or positionalize them.

- [ ] **Step 3: Add the opt-in features and parser helper**

Add `developer-tools = []` to `pg-foma` and
`developer-tools = ["pg-foma/developer-tools"]` to `pg-cli`. In `main.rs`, centralize parsing:

```rust
fn reject_or_accept_developer_flag(arg: &str) -> Result<bool, String> {
    if !matches!(arg, "--allow-unproven" | "--remove-size-limits" | "--no-enforce-capability") {
        return Ok(false);
    }
    #[cfg(feature = "developer-tools")]
    return Ok(true);
    #[cfg(not(feature = "developer-tools"))]
    Err(format!("unknown option {arg}"))
}
```

Use it before every positional fallback in `main.rs`, `pack.rs`, and `make_report.rs`; conditionally
render developer help. Production must not silently treat an unknown `--...` token as a path/word.

- [ ] **Step 4: Prove both build surfaces**

Run the default command from Step 2; expected PASS. Then set
`PANGLOSS_EXTRA_ARGS=--features developer-tools`, rerun it, and require the feature-gated assertions
to see the developer flags in help.

- [ ] **Step 5: Commit**

Commit: `feat(cli): quarantine developer FST flags`

### Task 2: Correctness override only

**Files:**
- Modify: `rust/crates/pg-cli/src/pack.rs`
- Modify: `rust/crates/pg-cli/src/make_report.rs`
- Modify: `rust/crates/pg-foma/src/analyzer.rs`
- Modify: `rust/crates/pg-foma/src/health.rs`
- Test: `rust/crates/pg-cli/tests/developer_flags_contract.rs`

- [ ] **Step 1: Add failing trust/readiness tests**

Under `developer-tools`, prove `--allow-unproven` can cross only a capability refusal, always writes
`CapabilityTrust::Overridden`, never certifies, and does not change an Error readiness finding.

```rust
assert_eq!(pack.manifest.capability_trust, CapabilityTrust::Overridden { /* fixture record */ });
assert_eq!(pack.manifest.fst_health.admission_without_overrides(), Severity::Error);
assert!(!report.certified);
```

- [ ] **Step 2: Run and observe the health-coupling failure**

Run the feature-enabled `developer_flags_contract`; expected FAIL in `apply_health_override` because
the current Boolean override also admits Error/Critical health.

- [ ] **Step 3: Split the code paths**

Feature-gate `FomaProposer::new_unproven_with_profile`. Replace `apply_health_override(...,
allow_unproven, ...)` with correctness-specific trust construction; never attach an override record
to an Error readiness finding. In `health.rs`, make payload sizes above the old Critical floor remain
Error readiness findings; capability refusal supplies Critical at the compatibility-report layer.
Keep `readiness_verdict.rs` returning `NotSupported` for overridden trust.

- [ ] **Step 4: Run focused CLI/unit tests**

Run `pg-cli`'s developer contract plus existing `pack` and `make_report` unit filters through
`pg.ps1`; expected PASS and no production help exposure.

- [ ] **Step 5: Commit**

Commit: `fix(pack): separate trust from readiness`

### Task 3: Typed size-limit stress mode

**Files:**
- Modify: `rust/crates/pg-foma/src/resource_envelope.rs`
- Modify: `rust/crates/pg-foma/src/compose_budget.rs`
- Modify: `rust/crates/pg-foma/src/morphotactics.rs`
- Modify: `rust/crates/pg-foma/src/worker.rs`
- Modify: `rust/crates/pg-foma/src/emit.rs`
- Test: `rust/crates/pg-foma/tests/developer_budget_controls.rs`

- [ ] **Step 1: Add failing orthogonality tests**

Define expected mode semantics in the test:

```rust
assert_eq!(CompileSizeMode::Managed.internal_caps_removed(), false);
assert_eq!(CompileSizeMode::DeveloperStress.internal_caps_removed(), true);
assert_eq!(stress.watchdog(), managed.watchdog());
assert_eq!(stress.communication(), managed.communication());
```

Also inject a live successor and assert `Incomplete`, never `SelectedSuccess`.

- [ ] **Step 2: Run and observe missing API failure**

Run feature-enabled `developer_budget_controls`; expected compile failure because
`CompileSizeMode` does not exist.

- [ ] **Step 3: Implement the minimum typed mode**

Add a serde-stable worker field:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CompileSizeMode { Managed, #[cfg(feature = "developer-tools")] DeveloperStress }
```

Convert only deterministic compose/enumeration/closure caps to optional disabled values in stress
mode. Do not alter `WatchdogEnvelope`, `CommunicationEnvelope`, absolute ceiling, capability result,
closure terminal, payload validation, or parity validation. Record the mode in attempt evidence and
bump the worker protocol version.

- [ ] **Step 4: Prove containment and completion**

Run `developer_budget_controls`, `worker_supervisor`, `closure_terminal_parity_gate`, and
`trusted_selected_build_gate` through `pg.ps1`; expected PASS.

- [ ] **Step 5: Commit**

Commit: `feat(foma): add contained stress mode`

### Task 4: CLI wiring and final regression

**Files:**
- Modify: `rust/crates/pg-cli/src/main.rs`
- Modify: `rust/crates/pg-cli/src/pack.rs`
- Modify: `rust/crates/pg-cli/src/make_report.rs`
- Test: `rust/crates/pg-cli/tests/developer_flags_contract.rs`

- [ ] Wire `--remove-size-limits` to `CompileSizeMode::DeveloperStress` only under the feature; make
pack/build reporting retain Error and exact completion evidence.
- [ ] Run default and feature-enabled CLI contracts; then run `pg-foma` worker/completion targets.
- [ ] Verify `git diff --check` and commit `feat(cli): wire FST stress execution`.
