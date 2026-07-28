# Aweti bounded profile before Task 5

Date: 2026-07-27  
Platform: Windows, `x86_64-pc-windows-msvc`  
Toolchain: `rustc 1.96.1 (31fca3adb 2026-06-26)`,
`cargo 1.96.1 (356927216 2026-06-26)`  
Instrumentation commit: `bed809d`  
Corrected shared P6 compiler commit: `09a5e48`  
External watchdog: 120 seconds

## Commands and provenance

The first release trace incorrectly entered the generic eager emitter through
`FomaProposer::new_with_profile`. Including its incremental release build, it
ran for 54.64 seconds and then returned the expected typed safety refusal:

```text
UNMEASURED stage=compile reason=unsupported error=grammar exceeds the
foma-engine's eager-enumeration budget: composite lexc entries = 200657
(limit 200000)
```

That is valid generic-compiler preflight evidence, but it is not an Aweti P6
runtime measurement. The safety limit was not raised.

Commit `09a5e48` extracted the exact pipeline already exercised by
`p6_templated_morphotactics_gate` into
`compile_templated_morphotactics`: templated underlying emission, lexc
compilation, all stratum-ordered phonological rules, boundary cleanup, final
composition, and minimization. The corrected trace was run from `rust/`:

```powershell
cargo run --release -p pg-foma --example p6_aweti_perf_trace
```

The process completed successfully under the 120-second watchdog in 95.3
seconds, including a 53.81-second incremental release build. Build time is not
included in any compiler-stage or word-stage percentage below.

## Compiler result

```text
COMPILE states=10609 arcs=298830 lexc_states=11530 lexc_arcs=114616
rules=18 skipped_rules=0 tuple_report_rules=18 lexc_lines=11360
```

All 18 rules compiled; none were silently skipped.

| Stage | Time (ms) | Share of measured P6 compile |
|---|---:|---:|
| Templated emission | 274.820 | 30.1% |
| Lexc compilation | 223.270 | 24.4% |
| Rule compilation and composition | 44.417 | 4.9% |
| Boundary-cleanup compilation | 1.757 | 0.2% |
| Final compositions and minimization | 369.271 | 40.4% |
| **Measured total** | **913.535** | **100.0%** |

The dominant measured compile stage is the combined final
composition/minimization stage at 40.4%. This timing does not separate its two
compositions from minimization, so it is not evidence that any one of those
operations can safely be removed.

## Bounded word probes

Each word shared one 50,000-raw-path allowance across direct proposal and any
peel-root proposals. All three completed within the allowance. No partial
candidate set was sent to confirmation.

| Word | Raw paths | Raw bytes | Unique candidates | Traversal (ms) | Decode/dedup (ms) | Confirm groups/calls | Confirmation (ms) | Confirmed |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `parua` | 12 | 168 | 12 | 0.302 | 0.010 | 3 / 3 | 1.223 | 1 |
| `an` | 48 | 672 | 48 | 0.429 | 0.017 | 3 / 3 | 0.588 | 1 |
| `ti` | 33 | 525 | 33 | 1.428 | 0.015 | 5 / 5 | 1.101 | 2 |
| **Total** | **93** | **1,365** | **93** | **2.159** | **0.042** | **11 / 11** | **2.912** | **4** |

Across the three probes, confirmation is the largest measured word-time
component at 56.9%, traversal is 42.2%, and decode/deduplication is 0.8%.
Absolute times are small: every complete probe took less than 2.6 ms across
these three measured components.

Per-word shares:

| Word | Traversal | Decode/dedup | Confirmation |
|---|---:|---:|---:|
| `parua` | 19.7% | 0.7% | 79.7% |
| `an` | 41.5% | 1.6% | 56.9% |
| `ti` | 56.1% | 0.6% | 43.3% |

`tomoʼatu` was deliberately not sent through this trace. It remains restricted
to the existing capped full-engine oracle probe.

## Task 5 interpretation

The trace does not support decode/allocation work: decode/dedup is only 0.8% of
the measured word time. Confirmation is already fused to 3–5 true internal
groups and costs 0.588–1.223 ms, so the three-word sample does not justify a
riskier confirmation-partition change.

The one measurement-supported, relation-preserving experiment is to apply the
same outgoing-arc preparation used by `FomaProposer::new` to this new
precompiled P6 path. The P6 network has 298,830 arcs, well above the existing
10,000-arc threshold; the new `from_precompiled_network` route currently
bypasses that preparation. Acceptance requires:

1. exact candidate and confirmed-analysis equality on the full Aweti recall
   corpus;
2. zero loss from the 100/106 Task 3 recall set;
3. identical network state/arc counts and all 18 rules still compiled;
4. a material traversal-time improvement under the same bounded probes.

If those conditions are not demonstrated, no speedup ships; this profile and
the negative experiment remain the Task 5 result.
