# Final-template interleaving prune: results

## Outcome

PanGloss now prunes analysis paths that unapply an ordinary morphological rule and then enter a
final affix template when synthesis cannot replay that ordering. This is a
**correctness/representability** gate, not a production-readiness refusal or a resource-containment
limit. By default, a partial rule at the stratum or below keeps the broader search because it may
represent a real rescue path. `batch --always-enforce-final-templates` deliberately bypasses that
guard and can change results.

The implementation mirrors the essential behavior of SIL Machine PR #491:

- a stratum-local two-state marker distinguishes paths whose last relevant unapplication was an
  ordinary rule;
- final templates are skipped at their selection seam, before memoization and rule/step accounting;
- all-final batteries are skipped as a unit, while mixed batteries skip only final templates;
- the state participates in deduplication and memo keys, survives replay, and resets at stratum
  boundaries;
- grammar-owned facts decide whether the default policy is sound, including partial-rule depth and
  template-slot/ordinary-rule overlap;
- analysis and synthesis classify the actual invocation as ordinary or template-slot, including
  shared rule IDs and compounding rules; and
- dense counters report template entries, batteries skipped, and individual final templates
  skipped.

## Scope of the timing evidence

Everything in the two timing sections below is **five-word, private, local evidence**: five words
per grammar, on gitignored exports that exist only on this machine, measured by hand rather than by
a gate. It is corroboration for a policy decision, never a reproducible result and never a
correctness claim. Nothing in CI can re-derive any figure here. Read the step counts as the
substantive part and the milliseconds as the weaker part; a step count is deterministic, a
millisecond figure on a machine with a live build pool is not.

## Stable three-run timings

These are medians of three warmed five-word batches. Each run used the same input and options,
`--threads 1 --memo=on --word-timeout-ms 120000`, under the repository-managed 2 GB/one-core run
job with `HC_STEP_STATS=1`. Grammar loading and Morpher construction are excluded.

The five original exports all contain either no applicable template or a partial-rule rescue fact.
Consequently the conservative default records zero skips and preserves the baseline step count.

| Grammar | Baseline ms | Default-policy candidate ms | Speedup | Steps before → after |
| --- | ---: | ---: | ---: | ---: |
| Indonesian | 4.037 | 4.158 | 0.97× | 133 → 133 |
| Sena | 24.645 | 24.517 | 1.01× | 6,631 → 6,631 |
| Amharic | 2,362.793 | 2,360.851 | 1.00× | 3,748 → 3,748 |
| Aweti | 817.383 | 802.938 | 1.02× | 342,966 → 342,966 |
| Mbugwe | 18,542.005 | 18,620.532 | 1.00× | 4,118,089 → 4,118,089 |

The explicit override demonstrates the available pruning on these same slices. It is a policy
experiment, not a safe recommendation for the original grammars, because their partial rules make
the broader search representable even though no sampled result needed it.

| Grammar | Baseline ms | Override ms | Speedup | Steps before → after | Step reduction |
| --- | ---: | ---: | ---: | ---: | ---: |
| Indonesian | 4.037 | 3.925 | 1.03× | 133 → 133 | 0.00% |
| Sena | 24.645 | 14.924 | 1.65× | 6,631 → 2,753 | 58.48% |
| Amharic | 2,362.793 | 2,093.871 | 1.13× | 3,748 → 2,868 | 23.48% |
| Aweti | 817.383 | 143.302 | 5.70× | 342,966 → 70,325 | 79.50% |
| Mbugwe | 18,542.005 | 761.509 | 24.35× | 4,118,089 → 45,284 | 98.90% |

The elapsed samples were:

| Grammar | Baseline samples ms | Default samples ms | Override samples ms |
| --- | --- | --- | --- |
| Indonesian | 3.917 / 4.037 / 4.149 | 4.158 / 6.306 / 3.920 | 3.895 / 3.925 / 4.045 |
| Sena | 24.415 / 24.645 / 24.748 | 24.868 / 24.517 / 23.980 | 14.924 / 15.411 / 14.280 |
| Amharic | 2,356.740 / 2,362.793 / 2,378.475 | 2,360.851 / 2,374.244 / 2,352.752 | 2,093.871 / 2,095.509 / 2,093.851 |
| Aweti | 818.878 / 809.061 / 817.383 | 804.111 / 800.464 / 802.938 | 145.912 / 141.419 / 143.302 |
| Mbugwe | 18,542.005 / 18,906.839 / 18,462.149 | 18,620.532 / 18,789.629 / 18,539.952 | 761.509 / 755.755 / 762.102 |

## Final-binary confirmation

- Baseline commit: `d2717652312aee355968daf57583a8d8cd58d747`
- Baseline binary SHA-256: `BBBC0F337039555FBE7F0EC4CAFD6A8A8F14E7598FED5680234D7057B00DF60D`
- Three-run timing candidate: `6759ab585281791be1949554914ce16d40edb22b`
- Timing-candidate binary SHA-256: `CAA83C2C8E3F543F893F0A4C15AA539BDA45A63421BF0B5C2B20B6E3799678E4`
- Final code commit: `a6f8f976364abd456c3f8f4e2f14c34bee699142`
- Final binary SHA-256: `2C0D8C8EFBF668D10C7D9B7D43FA8A8E53C8A4B4723FBE6BDC83723014D18142`

The final patch after the three-run measurements changes invocation-role classification and legacy
wrapper compatibility. All five benchmark grammars report template-slot/ordinary-rule disjointness,
so that patch cannot enter their measured analysis paths. One fresh run of both policies against
the exact final binary confirmed the same step counts and zero result-set divergence in either
direction for all 50 grammar/word/policy comparisons.

Fresh final-binary override samples were 3.911 ms (Indonesian), 13.603 ms (Sena), 1,974.866 ms
(Amharic), 140.888 ms (Aweti), and 737.866 ms (Mbugwe). These one-shot observations corroborate the
stable medians but do not replace them. The concurrent build pool was active during these smoke
runs, so the quiet three-run series above remains the timing result.

The exact final-binary default step counts were 133, 6,631, 3,748, 342,966, and 4,118,089. Override
step counts were 133, 2,753, 2,868, 70,325, and 45,284. For each result row, comparison normalized
the word, status, and sorted semicolon-delimited signature multiset. `Compare-Object` then reported
zero baseline-only and zero candidate-only rows for both policies on all five grammars.

## Sound positive control

The original Mbugwe XML has one partial rule, `mrule22`. A private control changes only that rule's
`partial="true"` to `partial="false"`, making the default prune provably admissible.

- Baseline median: **8,352.524 ms**, 2,195,961 steps
- Default-policy candidate median: **408.551 ms**, 30,671 steps
- Effect: **20.44× faster** and **98.60% fewer steps**
- Result parity: zero divergence in either direction for all five words in all three repetitions

This control is the sound implementation speedup. The explicit override figures above show why
classifying partial rules correctly matters, but a forced policy is not a substitute for proving
representability.

Private grammars, word slices, binaries, raw result TSVs, and stdout records remain under the
gitignored `.tmp/final-template-prune-benchmark` directory and are not committed.

## The same partial facts make these grammars FST-production-ineligible

Read the numbers above as HermitCrab evidence only. They say the default policy is conservative on
these grammars **because** those grammars declare partial morphemes, and they say nothing about
whether an FST built from them may be shipped. That second question was answered separately, and
the answer is no: a grammar declaring any partial lexical entry or partial affix-process rule is
`Severity::NotProductionReady` / `FindingClass::Readiness` for **every** FST strategy, so no
selectable, serialized, published or reconstructable production artifact comes out of one. See
`docs/superpowers/plans/2026-09-04-reject-partial-fst-builds-results.md`.

Three things that verdict is NOT, because each has been mistaken for it before:

- it is not `CannotRepresent` — the grammar is representable, and HC-Rust analyses it normally;
- it is not a compiler failure — the contained attempt really does compile, and a gate asserts it;
- it is not a resource-containment event — no monitor fired and no limit was reached.

So "all five reference grammars keep an accepted backend" (which
`five_language_backend_reports_gate.rs` still asserts, and which remains true) and "an FST built
from them may be published" are different claims, and only the first one holds. The per-grammar
partial inventory is now derived from `Grammar::partial_morpheme_facts` inside that gate rather
than from a hardcoded grammar-name list, so a grammar that gains or loses a partial morpheme moves
its own verdict.
