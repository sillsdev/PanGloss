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

- [x] **Step 1: Add a failing production-contract test**

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

- [x] **Step 2: Run the production test and observe failure**

Run: `& rust/tools/pg.ps1 -Mode test -Package pg-cli -TestTarget developer_flags_contract`

Expected: FAIL because current help exposes the switches and parsers accept or positionalize them.

- [x] **Step 3: Add the opt-in features and parser helper**

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

- [x] **Step 4: Prove both build surfaces**

Run the default command from Step 2; expected PASS. Then set
`PANGLOSS_EXTRA_ARGS=--features developer-tools`, rerun it, and require the feature-gated assertions
to see the developer flags in help.

- [x] **Step 5: Commit**

Commit: `feat(cli): quarantine developer FST flags`

### Task 2: Correctness override only

**Files:**
- Modify: `rust/crates/pg-cli/src/pack.rs`
- Modify: `rust/crates/pg-cli/src/make_report.rs`
- Modify: `rust/crates/pg-foma/src/analyzer.rs`
- Modify: `rust/crates/pg-foma/src/health.rs`
- Test: `rust/crates/pg-cli/tests/developer_flags_contract.rs`

- [x] **Step 1: Add failing trust/readiness tests**

Under `developer-tools`, prove `--allow-unproven` can cross only a capability refusal, writes local
developer evidence with `CapabilityTrust::Overridden`, never certifies or production-publishes, and
does not change an independent Error readiness finding. The refused partial fixture's `PGF0013`
capability finding belongs in its backend assessment, so its readiness projection may be `Ideal`;
use a separate readiness fixture to prove raw Error still blocks publication.

```rust
assert_eq!(pack.manifest.capability_trust, CapabilityTrust::Overridden { /* fixture record */ });
assert_eq!(pack.manifest.fst_health.admission_without_overrides(), Severity::Ideal);
assert!(!report.certified);
```

- [x] **Step 2: Run and observe the health-coupling failure**

Run the feature-enabled `developer_flags_contract`; expected FAIL in `apply_health_override` because
the current Boolean override also admits Error/Critical health.

- [x] **Step 3: Split the code paths**

Feature-gate `FomaProposer::new_unproven_with_profile`. Replace `apply_health_override(...,
allow_unproven, ...)` with correctness-specific trust construction; never attach an override record
to an Error readiness finding. In `health.rs`, make payload sizes above the old Critical floor remain
Error readiness findings; capability refusal supplies Critical at the compatibility-report layer.
Keep `readiness_verdict.rs` returning `NotSupported` for overridden trust.

- [x] **Step 4: Run focused CLI/unit tests**

Run `pg-cli`'s developer contract plus existing `pack` and `make_report` unit filters through
`pg.ps1`; expected PASS and no production help exposure.

- [x] **Step 5: Commit**

Commit: `fix(pack): separate trust from readiness`

Verification note: the focused trust/readiness filters and production flag contract pass. A full
`pg-cli` developer-feature run also exposes four separate recipe-optimizer regressions introduced
by `87320bff`: marker-bearing candidates return `Unsupported` before measurement, while the older
continuation/evidence tests still expect confirmation work. This slice does not change
`backend_runtime.rs`, `recipe_optimize.rs`, or those tests; repair that contract independently.
An independent Sol/xhigh source review found no P0-P2 defects and verified the three corrected
invariants: current-grammar override selection, overridden/completeness pack rejection, and
missing-payload evidence in the gated backend assessment.

### Task 3: Typed size-limit stress mode

**Files:**
- Modify: `rust/crates/pg-foma/src/resource_envelope.rs`
- Modify: `rust/crates/pg-foma/src/compose_budget.rs`
- Modify: `rust/crates/pg-foma/src/morphotactics.rs`
- Modify: `rust/crates/pg-foma/src/characterization.rs`
- Modify: `rust/crates/pg-foma/src/worker.rs`
- Modify: `rust/crates/pg-foma/src/worker_contract.rs`
- Modify: `rust/crates/pg-foma/src/emit.rs`
- Modify: `rust/crates/pg-foma/src/completed_build.rs`
- Modify: `rust/crates/pg-foma/src/analyzer.rs`
- Coordinate CLI propagation in `rust/crates/pg-cli/src/main.rs` (the Task 4 CLI wiring owns the
  final flag surface).
- Test: `rust/crates/pg-foma/tests/developer_budget_controls.rs`

- [ ] **Step 1: Add failing orthogonality tests**

Define expected mode semantics in the test:

```rust
assert_eq!(CompileSizeMode::Managed.internal_caps_removed(), false);
assert_eq!(CompileSizeMode::DeveloperStress.internal_caps_removed(), true);
assert_eq!(stress.watchdog(), managed.watchdog());
assert_eq!(stress.communication(), managed.communication());
```

Construct `managed` and `stress` as typed projections of the same shipped
`ResourceEnvelope`; assert that the projection preserves the envelope's identity/digest and
protocol/communication/watchdog values. Also require a mechanism-engaged counter: a fixture must
cross at least one managed deterministic compose/enumeration/closure cap, then complete under
stress with the observed counter retained in evidence. Inject a live successor separately and
assert `Incomplete`, never `SelectedSuccess`.

- [ ] **Step 2: Run and observe missing API failure**

Run feature-enabled `developer_budget_controls`; expected compile failure because
`CompileSizeMode` does not exist.

- [ ] **Step 3: Implement the minimum typed mode**

Add a serde-stable worker field:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CompileSizeMode { Managed, #[cfg(feature = "developer-tools")] DeveloperStress }
```

Keep `ResourceEnvelope::for_id` and its canonical identity/digest as the exact shipped managed
profile. Derive a typed, non-digest-changing mode projection for effective budgets; never mutate or
re-hash the shipped envelope. Thread the mode through both generic and selected worker requests,
`CompileEnvelopeRequest`, the analyzer/emitter, and the completed-build evidence/wire. Bump the
worker protocol version in `worker_contract.rs`.

In `DeveloperStress`, disable only deterministic internal compose, enumeration, and closure
size/work caps (including their compound dimensions), while retaining observed counters. Preserve
the compose step timeout, apply-time containment, the versioned absolute chain-depth ceiling,
representation/correctness caps (including uncovered-material reporting), and all worker watchdog,
RSS, output, request/result protocol, and payload ceilings. Do not alter capability results,
closure-terminal semantics, exact terminal evidence, payload identity validation, or semantic parity
validation. A live successor, pending work, skipped/truncated/uncovered material, or any containment
breach remains incomplete and can never produce a selected artifact.

The CLI must not consume `--remove-size-limits` and do nothing: `parse`/`batch` either propagate
the typed mode into their Foma compile path or reject it with an explicit command/path error.

- [ ] **Step 4: Prove containment and completion**

Run `developer_budget_controls`, `worker_supervisor`, `closure_terminal_parity_gate`, and
`trusted_selected_build_gate` through `pg.ps1`; expected PASS. The developer-budget test must prove
the mode was engaged via observed counters, outer containment is unchanged, a complete stress
artifact remains readiness `Error`, and incomplete/live-successor attempts never become selected.

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
