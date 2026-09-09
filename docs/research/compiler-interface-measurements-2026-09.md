# Compiler interface measurements, September 2026

Before/after numbers for the four "Strong" architecture slices landed on `main` between `0819dc9b`
and `088f135d`, and for the review-fix round that followed (`b8cf73f5..7306499d`). Recorded because
CLAUDE.md requires the differential measurement to exist before the change is trusted, and the
architecture review that motivated the slices asked for exactly these figures.

## Runtime/Compiler seam (`pg-health`)

Dependency graph under `cargo metadata --filter-platform wasm32-unknown-unknown`, dev-only edges
excluded, transitive closure from the named root.

| Root | Before (nodes) | After (nodes) | `foma` reachable | `pg-foma` reachable |
|---|---|---|---|---|
| `pg-wasm` | 131 | 85 | yes -> no | yes -> no |
| `pg-pack` | 123 | 38 | yes -> no | yes -> no |

Pinned by `rust/crates/pg-wasm/tests/wasm_excludes_compiler.rs`.

## Single admission owner

`rust/crates/pg-foma/tests/admission_single_owner_gate.rs`, every conformance fixture x every
`EmissionStrategy`: 201 observations, 0 reports missing, 0 gate/selector disagreements, before and
after the consolidation. Every `EmissionStrategy` member always receives a report, so
`BackendSelection::decision_for`'s fail-closed arm is a guard, not live policy.

## Interface vs workbench

| Metric | Before | After gating | After report move |
|---|---|---|---|
| `pub mod` in `pg-foma/src/lib.rs` | 60 | 60 (28 gated behind `test-support`) | 55 |
| `pub` items in `pg-foma/src` (grep count) | 1,177 | 1,177 (visibility is cfg-conditional, invisible to the grep) | 1,121 |
| pg-foma example targets | 38 | 8 (31 collapsed into `examples/lab`) | 8 |
| pg-foma integration test targets | 116 | 116 | 117 -> 117 (one gate trimmed, none added) |
| Product build of pg-foma, dead-code warnings | 0 | 216 | 2 (after `#[allow(dead_code)]` on the gated declarations) |
| `-Mode build -Package pg-cli`, warm cache, second of two runs | 1m 17s | -- | 1m 15s |

The report move (`backend_report`, `readiness_verdict`, `readiness_policy` into pg-cli) did not
change link time measurably: the same code is compiled and linked into `pangloss.exe` either way.
The interface shrank; the build cost did not. Treat "moving modules toward pg-cli shortens the
build" as unconfirmed.

## Backend seam

`rg -n 'match .*(strategy|adapter)' rust/crates/pg-foma/src`: 16 hits before the trait, 15 after
the third trait method landed. The remaining hits are label mapping, capability composition,
coverage reporting, emitter tier/precision matches, and one documented exception
(`witnessed_coverage::compile_with_backend_for_measurement`, which needs a measurement compile that
neither `Backend` compile method provides). `LoweringAdapter` has no remaining references.
