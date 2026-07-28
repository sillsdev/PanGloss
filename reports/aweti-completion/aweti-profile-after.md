# Aweti bounded profile after Task 5

Date: 2026-07-28
Platform: Windows, `x86_64-pc-windows-msvc`
Toolchain: `rustc 1.96.1 (31fca3adb 2026-06-26)`,
`cargo 1.96.1 (356927216 2026-06-26)`
Baseline commit: `2508eaa`
Task 5 implementation commit: `fb3e753`
External watchdog: 120 seconds

## Command and provenance

The after measurement used the same release command and 120-second external watchdog as the
Task 4 before profile:

```powershell
cargo run --release -p pg-foma --example p6_aweti_perf_trace
```

This is the exact P6 templated pipeline, not the generic eager compiler. The generic compiler's
`200657 > 200000` refusal remains valid preflight evidence only and is not included in these
performance measurements.

## One-time preparation cost

Task 5 applies the existing outgoing-arc preparation to the precompiled P6 network before repeated
word lookup. The measured one-time cost was **5.364 ms** (`apply_prepare`). The network relation and
its state/arc counts are unchanged.

## Bounded word probes

Each word used the same shared 50,000-raw-path allowance as the before profile. The table compares
only proposal traversal time; decode/deduplication and confirmation retain their separate Task 4
measurements.

| Word | Before traversal (ms) | After traversal (ms) | Change | Speedup |
|---|---:|---:|---:|---:|
| `parua` | 0.302 | 0.190 | -37.1% | 1.59x |
| `an` | 0.429 | 0.224 | -47.8% | 1.92x |
| `ti` | 1.428 | 0.475 | -66.7% | 3.01x |
| **Aggregate** | **2.159** | **0.889** | **-58.8%** | **2.43x** |

The three-word traversal saving is **1.270 ms** per pass. The measured break-even is therefore
`5.364 / 1.270 = 4.2` three-word passes, or approximately **13 lookups**. Workloads beyond that
point recover the one-time preparation cost and retain the traversal improvement.

## Exact behavior invariants

Candidate and confirmation identities did not change. Before and after values for
`raw paths / raw bytes / unique candidates / final candidates / confirmed analyses` were:

| Word | Before | After |
|---|---|---|
| `parua` | `12 / 168 / 12 / 12 / 1` | `12 / 168 / 12 / 12 / 1` |
| `an` | `48 / 672 / 48 / 48 / 1` | `48 / 672 / 48 / 48 / 1` |
| `ti` | `33 / 525 / 33 / 33 / 2` | `33 / 525 / 33 / 33 / 2` |

The full P6 gate also remained unchanged: **100/106** oracle-bearing words recalled, the same six
residual misses (`muʼazan`, `tsãkỹjokwaw`, `moʼazan`, `tsãn`, `moʼaza`, `kỹjokwaw`), **10,609
states / 298,830 arcs**, and all **18** phonological rules compiled with no skips.

## Verdict

**SHIP.** The change preserves the exact measured proposal/confirmation behavior and the full P6
recall/network invariants while reducing aggregate traversal time by 58.8% (2.43x). Its 5.364 ms
one-time cost breaks even after approximately 13 lookups, which is well below a normal analyzer
session's expected reuse of one compiled grammar.
