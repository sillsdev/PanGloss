# Task 7 — four-language release matrix

## Provenance and interpretation

All commands were run serially from `rust/` at final matrix SHA `b819eb706c0fa42b9404636cb2ae4e142aafc984`, with `rustc 1.96.1 (31fca3adb 2026-06-26)` and `cargo 1.96.1 (356927216 2026-06-26)`.
Every command used its linked release command with `--include-ignored --nocapture --test-threads=1`, exited `0`, and passed four tests. Cargo compile/test elapsed data is preserved; no separate process-wall stopwatch was retained. Amharic is explicitly a final-run transcript, not a redirected file.
“Corpus words”, “analyzed words”, and “oracle analyses” remain distinct: fewer analyzed words are not silently called exclusions without a gate-emitted exclusion or timeout record.

| Language | Corpus / actual oracle denominator | Exclusions, timeouts, unsupported rows | Emit / compile / network | Result | Lookup / confirmation | Cargo build / test |
|---|---|---|---|---|---|---|
| Sena | 120 corpus words; **326/326** engine analyses across 87 analyzed words | Full tier, uncovered 0. Capture does not label the other 33 corpus words as excluded/timed out. | 68.3973 ms / 13.0185975 s; 255,707 lexc lines, 7,912,781 bytes; states/arcs not emitted | 326/326 covered; `mbali`: 8 engine sequences, 104 proposer candidates | engine 30.2393105 s; proposal 59.8148 ms total, 7.2021 ms max, 687.526 µs mean; nonsense 0 in 37.5 µs | 57.11 s / 82.47 s |
| Indonesian | 121 words; 7 explicit reduplication exclusions; **97/97** engine analyses across 96 analyzed words | Unsupported: 3 reduplication rules (`mrule6`, `mrule12`, `mrule14`). Excluded: `membagi-bagi`, `memijit-mijit`, `meminta-minta`, `mengamat-amati`, `mengayuh-ngayuh`, `menulis-nulis`, `menyewa-nyewa`. | 85.7049 ms / 88.7883 ms; 896 lines, 30,126 bytes; states/arcs not emitted | 97/97 covered; five junction checks covered | engine 148.2457 ms; proposal 392.8 µs total, 14.3 µs max, 4.091 µs mean; nonsense 0 in 3.9 µs | 0.15 s / 0.63 s |
| Amharic | 100 words; **31/31** engine analyses across 29 analyzed words | 6 zero-analysis words hit the 10 s engine timeout; parity uses 29 post-timeout words. Partial tier, 1 uncovered row (identity not retained in transcript). | 4.1357297 s / 4.1543726 s; 80,482 lines, 5,089,278 bytes; states/arcs not emitted | 31/31 covered; parity on 29 words | engine 122.4872975 s; proposal 337.8 µs total, 24.9 µs max, 11.648 µs mean; parity analysis 687.9788 ms total, 148.0608 ms max, 23.723406 ms mean; nonsense 0 | 0.15 s / 270.12 s |
| Aweti | **100/106** composition-recall denominator | Partial: 16 uncovered—12 reduplication (`mrule100–102`, `106–114`) and 4 circumfix-prefix placement (`mrule40#allo1–4`); no timeout. Residual misses: `muʼazan`, `tsãkỹjokwaw`, `moʼazan`, `tsãn`, `moʼaza`, `kỹjokwaw`. | 251.4396 ms / 215.7936 ms lexc + 43.964 ms rules + 365.5263 ms final; final 10,609 states / 298,830 arcs; `skipped=[]` | 100/106 = 94.3%; `parua` covered, 12 raw paths | corpus sweep 12.0115363 s; `parua` apply-up 176.1 µs | 30.64 s / 15.88 s |

All four release gates are green. This is not a claim of complete Aweti correctness: its six residual morphology/rule misses remain open.

## Task 4/5 bounded profiling — historical, not a fresh-matrix comparison

These separate Aweti trace results are included for Task 4/5 evidence, not compared to Task 7 as if collected in the same invocation or revision.

| Task | Revision / command | Scope and result | Safety boundary |
|---|---|---|---|
| 4 — before | `bed809d` + `09a5e48`; `cargo run --release -p pg-foma --example p6_aweti_perf_trace` | Exact P6 compiler: 913.535 ms measured compile; final composition/minimize 369.271 ms (40.4%). `parua`/`an`/`ti`: traversal 2.159 ms, decode/dedup 0.042 ms, confirmation 2.912 ms. | 50,000 raw-path allowance per word; complete candidate/confirmation sets. Generic eager refusal is preflight evidence, not P6 timing. |
| 5 — after | baseline `2508eaa`, implementation `fb3e753`; same trace | One-time preparation 5.364 ms. Same probes: traversal 2.159 → 0.889 ms (58.8% reduction, 2.43×); break-even about 13 lookups. | Exact candidate/confirmed-analysis identities; full recall stayed 100/106 and final network stayed 10,609 / 298,830. |

Authoritative detail: [before](aweti-profile-before.md) and [after](aweti-profile-after.md).

The ranked, safety-bounded follow-on experiments are documented in
[Aweti performance follow-on](../../docs/fst-plan/aweti-performance-follow-on.md).

## Durable evidence

- [Sena release log](sena-release.log)

- [Indonesian release log](indonesian-release.log)
- [Amharic release transcript](amharic-release.log)
- [Aweti release log](aweti-release.log)
