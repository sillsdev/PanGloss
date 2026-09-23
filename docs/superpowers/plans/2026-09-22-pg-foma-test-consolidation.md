# pg-foma Test Consolidation Implementation Plan

> **For agentic workers:** Work stays in `.worktrees/pg-foma-test-consolidation`; do not run Cargo or Rust builds until commit headroom is available.

**Goal:** Reduce pg-foma's 117 integration-test executables to 19 explicit harnesses while preserving every source and test exactly once.

**Architecture:** Disable implicit integration-target discovery and declare a small set of Cargo harnesses. Each thematic harness imports original flat test files as child modules; eleven special, stateful, corpus, or stress targets remain standalone. A source inventory gate compares declared Cargo targets and all child-module imports in both directions.

**Tech Stack:** Rust/Cargo manifests and integration tests, PowerShell structural gates, JSON corpus manifest.

---

### Task 1: Add a failing structural inventory gate

**Files:**
- Create `rust/tools/tests/pg-foma-test-consolidation.tests.ps1`.

- [ ] Assert the explicit harness count is 5–20, and that Cargo auto integration-test discovery is disabled.
- [ ] Derive module imports from every aggregate harness and compare them to source files and explicit standalone targets.
- [ ] Report missing sources separately from duplicate/extra imports and target paths.
- [ ] Run the PowerShell gate before implementation and record its expected failure.

### Task 2: Declare grouped harnesses and target mapping

**Files:**
- Modify `rust/crates/pg-foma/Cargo.toml`.
- Create thematic `rust/crates/pg-foma/tests/*.rs` aggregators.
- Preserve original source paths for citation liveness; add path-based child-module declarations.
- Keep the two panic-hook files, `f0_viability`, `typology_speedup`, and corpus/stress gates standalone.

### Task 3: Preserve module, corpus, and target-selection behavior

**Files:**
- Modify test sources with `mod common` to resolve the existing `tests/common/mod.rs` explicitly.
- Modify `rust/crates/pg-foma/tests/coverage_citation_liveness.rs` to scan recursively.
- Modify `rust/crates/pg-conformance-fixtures/src/corpus.rs` and `rust/tools/corpus-manifest.json` to resolve grouped-module test IDs through the declared harness.
- Update `rust/tools/pg.ps1`, `_common.ps1`, and target-specific scripts/docs for the new harness names or compatible source aliases.

### Task 4: Verify structurally without Rust builds

- [ ] Run the PowerShell source-to-harness inventory gate.
- [ ] Run the related PowerShell runner-selector and corpus-manifest structural gates.
- [ ] Run `git diff --check`; inspect the complete diff and exact target/source counts.
- [ ] Do not invoke Cargo, rustc, or any Rust build until the parent agent re-authorizes it.
