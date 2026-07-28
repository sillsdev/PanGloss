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

## Task 9 completion audit — evidence status: in progress

This is an audit of evidence, not a declaration of full Aweti correctness. The approved design is restored verbatim from `ae87f0c` at `docs/superpowers/specs/2026-07-20-aweti-correctness-performance-design.md`; its permanent 100% proposer-recall requirement remains unmet.

| Objective / criterion | Authoritative source | Test / exact command | Observed result | Audit status |
|---|---|---|---|---|
| Preserve baseline recall while raising Aweti recall | `tags.rs`; [baseline record](baseline-retrospective.log); [Aweti release log](aweti-release.log) | `cargo test -p pg-foma --release --test p6_templated_morphotactics_gate -- --include-ignored --nocapture --test-threads=1` | Fresh final run: 100/106 (94.3%), exact six misses; executable assertion enforces numerator, denominator, and miss set | **Proven** by the repaired gate and fresh 4/4 release run. |
| Fix the demonstrated bare-root/tag boundary at the source | `tags.rs` `ZERO_GLYPH`; `p6_templated_morphotactics_gate` test `d_bare_root_tag_atomicity_boundary` | Same P6 release gate | Literal `0` is avoided in emitted tag numerals and decoded reversibly; atomicity boundary test passes | Proven by source/test and recorded gate. |
| Preserve exact proposal/confirmation behavior under bounded diagnostics | [before profile](aweti-profile-before.md), [after profile](aweti-profile-after.md), `p6_aweti_perf_trace.rs` | `cargo run --release -p pg-foma --example p6_aweti_perf_trace` | Shared 50,000-path allowance; `parua`/`an`/`ti` candidate and confirmed-analysis identities exact | Proven for bounded probes; not a claim about unmeasured inputs. |
| Ship only a measured recall-preserving speedup | `templated_compile.rs`; [after profile](aweti-profile-after.md) | Trace command above plus the P6 release gate | Prepared outgoing arcs: 5.364-ms one-time cost; traversal 2.159 → 0.889 ms (2.43x); recall/network/rules unchanged | Proven by Task 5 evidence. |
| Compile supported Aweti phonology without silent skips | [Aweti release log](aweti-release.log) | P6 release gate above | All 18 phonological rules compiled; `skipped=[]`; final 10,609 states / 298,830 arcs | Proven by recorded release gate. |
| Phase C stage-2 coverage is parity, honest skip, or detected failure | Plan Task 6 execution record; Phase C integration tests | Recorded Phase C batches plus fresh `cargo test -p pg-foma --test phase_c_right_to_left -- --nocapture --test-threads=1` | Recorded 18/18 then 13/13; fresh RTL 9/9; fresh coverage gates 1/1 and 2/2 | **Proven** for the integrated focused regression scope. |
| Four-language release evidence has real denominators and exclusions | This matrix and four linked release logs | The four commands in the Task 7 plan block, each with `--include-ignored --nocapture --test-threads=1` | Sena 326/326 analyses; Indonesian 97/97 with 7 redup exclusions; Amharic 31/31 after 6 engine timeouts; Aweti 100/106 | **Proven** by durable captures; Amharic is explicitly transcript provenance. |
| Publish a safe prioritized performance plan | [Aweti performance follow-on](../../docs/fst-plan/aweti-performance-follow-on.md) | Reader-tested documentation review; future candidate commands are specified in that plan | Six ranked options each map to red test, metric, bounded experiment, equality invariant, and ship rule; shortcuts rejected | Proven as a planning deliverable, not an implemented performance gain. |
| Final focused/regression verification and independent evidence review | Task 9 plan gate | P6 release gate; `phase_c_right_to_left`; `conformance_coverage_gate`; `plan_interaction_coverage_gate`; independent Luna review | Working-tree results: P6 4/4 at exact 100/106; RTL 9/9; coverage 1/1 and 2/2. Review found the weak recall assertion, which was repaired and red/green mutation-tested. | **Pending final-commit rerun**; Task 9 is not yet complete. |

### Unresolved Aweti miss classes and completion decision

The unresolved words are `muʼazan`, `tsãkỹjokwaw`, `moʼazan`, `tsãn`, `moʼaza`, and `kỹjokwaw`. They are six genuine **morphology/rule gaps**: not the fixed zero-digit tag/sigma defect, not the combining-mark red herring, not a candidate-cap result, and not a test timeout. Their finer linguistic subclasses remain uninvestigated; this audit does not invent them.

The overall Aweti correctness goal remains **open**. Current recall is **100/106 (94.3%)**, not 100%. The approved design and Task 1/2 retrospective records are present, but Task 1's required raw logs are not, and Task 9 still needs verification from the final commit.

### Final-verification qualification (discovered during Task 9)

Independent review found that the historical gate could pass at only 32 recalls. The repaired `p6_templated_morphotactics_gate` now sorts and compares the exact miss set and asserts the `(100, 106)` result. Its fresh release rerun passed all four tests, so 100/106 is now a durable regression boundary rather than output-only evidence.

### Coverage corrections from independent reader review

The Amharic matrix row means **six engine timeouts**, not six exclusions; its denominator policy remains 31/31 analyses across 29 analyzed words, with parity restricted after those timeouts.

| Plan task / criterion | Source / command / observation | Evidence status |
|---|---|---|
| Task 1 — clean bounded baseline | Commit `f892cfd` records Tasks 1–3 and broad verification; [retrospective baseline](baseline-retrospective.log) preserves the exported 68/106 and network measurements. The required contemporaneous commands, toolchain, wall times, watchdog outcomes, and three non-Aweti raw baseline logs were never committed. | **Partially evidenced**; retrospective data is not misrepresented as verbatim raw capture. |
| Task 2 — isolate bare-root failure | Commit `f892cfd`, [diagnostic record](bare-root-diagnostic.md), `tags.rs`, and `d_bare_root_tag_atomicity_boundary` identify the first failing `fsm_intersect`/sigma boundary, record the RED sigma-membership assertion, and reject the combining-mark hypothesis. | **Contemporaneously evidenced and regression-tested.** |

Accordingly, the engineering changes are locatable and verified, but the completion audit remains open for two independent reasons: Task 1's required raw evidence was not preserved, and the six genuine morphology/rule misses keep proposer recall below the approved 100% requirement.
