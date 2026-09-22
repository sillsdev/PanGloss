# Minimal Single-Word Trace JSON Implementation Plan

**Goal:** Add the smallest opt-in Try-a-Word-style JSON output to `pangloss parse`, using data PanGloss already records.

## Work

- [x] Add `Morpher::parse_word_traced_with_stats` so one unmerged parse produces its outcome, trace, and existing `StatsRow` values.
- [x] Add `--trace-details` only to the single-word `parse` command and require `--trace --trace-format=json`.
- [x] Serialize a versioned envelope around the existing compact trace tree.
- [x] Include authoritative completion flags, steps, whole-run elapsed time, signature, guessed state, and successful analyses.
- [x] Group existing counters and self timing by `ObjectKind`; report unsupported timing as `null` using `self_time_supported`.
- [x] Leave ordinary parse and ordinary trace execution unchanged.
- [x] Document the public format and run primary review plus managed verification.
- [x] Commit and fast-forward the reviewed branch to `main` without including unrelated working-tree changes.

## Files

- `rust/crates/pg-parse/src/morpher.rs`: combined traced/stats wrapper.
- `rust/crates/pg-parse/tests/stats_collector_gate.rs`: combined-run parity and determinism.
- `rust/crates/pg-cli/src/rich_trace.rs`: validation and JSON envelope.
- `rust/crates/pg-cli/src/main.rs`: opt-in branch and overall timer.
- `rust/crates/pg-cli/src/surface.rs`: flag metadata.
- `docs/formats/trace-format.md`: user-facing contract.

## Verification commands

```powershell
rust/tools/pg.ps1 -Mode check -Package pg-parse
rust/tools/pg.ps1 -Mode test -Package pg-parse -TestTarget stats_collector_gate -Filter traced_stats_run_matches_separate_trace_and_stats_runs
rust/tools/pg.ps1 -Mode check -Package pg-cli
rust/tools/pg.ps1 -Mode test -Package pg-cli -Filter rich_trace
```

Run the repository conformance gate after review because the combined wrapper touches the parse call surface. Timing values are checked structurally rather than compared as fixed numbers.