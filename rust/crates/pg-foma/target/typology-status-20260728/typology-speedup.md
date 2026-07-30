# Typology speedup: complete engine vs compiled proposer+confirm

Per-fixture median-of-medians over 1 timed samples per word (1 discarded warmup call each). Timer floor calibrated this run at **100ns**; any aggregate at or below it is shown as `<100ns`, never `0`. Grouped per fixture, which is per construct/typology (fixtures are named by construct under `edge-cases/`, by typology under `languages/`).

## Languages (typology)

| source | fixture | words | complete engine | compiled (foma) | speedup |
|---|---|---|---|---|---|
| machine:languages | fusional-realizational-morphology | 55 | 0.251 ms | REFUSED: circumfix-output-action.faithful-structural-composite (mrule 14 allomorph #0 (LHS-material-dropping output action)); mpr-group.overwrite-output (MprGroup 0 (Overwrite)) | n/a (refused) |
| machine:languages | metathesis-phase-isolation | 19 | 0.207 ms | REFUSED: metathesis.faithful-swap-construction (prule 2 (MetathesisRule)) | n/a (refused) |
| machine:languages | polysynthetic-stratal-derivation-chain | 20 | 0.224 ms | 0.347 ms | 0.65x |
| machine:languages | prefixal-discontinuous-slot-dependency | 10 | 0.093 ms | 0.086 ms | 1.08x |
| machine:languages | suffixing-evidential-adjacency-chain | 27 | 0.504 ms | 0.255 ms | 1.98x |
| machine:languages | suffixing-extension-slot-ordering | 38 | 0.140 ms | REFUSED: mpr-group.overwrite-output (MprGroup 0 (Overwrite)) | n/a (refused) |
| machine:languages | suffixing-vowel-harmony | 23 | 0.126 ms | 0.078 ms | 1.61x |
| machine:languages | templatic-root-modification | 25 | 0.143 ms | REFUSED: circumfix-output-action.faithful-structural-composite (mrule 3 allomorph #0 (LHS-material-dropping output action)) | n/a (refused) |

## Edge cases (construct)

| source | fixture | words | complete engine | compiled (foma) | speedup |
|---|---|---|---|---|---|
| machine:edge-cases | deep-optional-affix-nesting | 3 | 70.489 ms | 68.109 ms | 1.03x |
| machine:edge-cases | diacritic-segments | 13 | 0.104 ms | 0.105 ms | 0.99x |
| machine:edge-cases | disjunctive-recheck | 12 | 0.051 ms | 0.052 ms | 0.98x |
| machine:edge-cases | loader-default-symbol | 2 | 0.020 ms | 0.028 ms | 0.71x |
| machine:edge-cases | loader-isactive | 2 | 0.030 ms | 0.019 ms | 1.58x |
| machine:edge-cases | loader-pattern-shapes | 4 | 0.013 ms | 0.016 ms | 0.84x |
| machine:edge-cases | simultaneous-epenthesis-cascade | 1 | 0.024 ms | 0.037 ms | 0.64x |
| machine:edge-cases | strrep-identity | 12 | 0.063 ms | 0.066 ms | 0.96x |
| machine:edge-cases | truncate-morphotactic | 9 | 0.076 ms | 0.173 ms | 0.44x |
| staging:edge-cases | bistratal-overlapping-segment-representation | 5 | 0.008 ms | 0.002 ms | 3.85x |
| staging:edge-cases | circumfix-infix-interior-action-precedence | 2 | 0.051 ms | 0.052 ms | 0.99x |
| staging:edge-cases | circumfix-non-first-allomorph-selection | 3 | 0.050 ms | 0.052 ms | 0.96x |
| staging:edge-cases | circumfix-reduplication-precedence | 2 | 0.114 ms | 0.111 ms | 1.03x |
| staging:edge-cases | compounding-non-recursive | 9 | 0.029 ms | 0.015 ms | 2.02x |
| staging:edge-cases | guesser-pattern-root-fallback | 3 | 0.013 ms | 0.014 ms | 0.91x |
| staging:edge-cases | infix-interdigitation | 6 | 0.035 ms | 0.041 ms | 0.85x |
| staging:edge-cases | mpr-gated-exception | 9 | 0.078 ms | 0.073 ms | 1.06x |
| staging:edge-cases | multi-table-metathesis-shared-representation | 5 | 0.023 ms | 0.037 ms | 0.63x |
| staging:edge-cases | optional-template-composite | 11 | 0.082 ms | 0.066 ms | 1.24x |
| staging:edge-cases | recursive-endocentric-compounding | 6 | 0.087 ms | 0.094 ms | 0.93x |
| staging:edge-cases | right-to-left-anchor-environment | 6 | 0.020 ms | 0.035 ms | 0.58x |
| staging:edge-cases | right-to-left-bounded-quantifier-rewrite | 5 | 0.023 ms | 0.027 ms | 0.84x |
| staging:edge-cases | right-to-left-cross-table-segments-environment | 5 | 0.019 ms | 0.030 ms | 0.64x |
| staging:edge-cases | right-to-left-metathesis-reversal | 6 | 0.017 ms | 0.027 ms | 0.62x |
| staging:edge-cases | right-to-left-segments-environment | 5 | 0.017 ms | 0.020 ms | 0.82x |
| staging:edge-cases | segment-natural-class-table-binding | 4 | 0.020 ms | 0.033 ms | 0.60x |
| staging:edge-cases | simultaneous-subrule-genuine-overlap | 9 | 0.022 ms | REFUSED: simultaneous.subrule-overlap (prule 0 subrules 0/1) | n/a (refused) |
| staging:edge-cases | standalone-combining-mark | 4 | 0.032 ms | 0.035 ms | 0.92x |
| staging:edge-cases | subrule-morphosyntactic-gating | 2 | 0.048 ms | 0.095 ms | 0.51x |
| staging:edge-cases | template-category-sharing | 6 | 0.058 ms | 0.043 ms | 1.34x |
| staging:edge-cases | two-table-shared-representation-recall | 4 | 0.020 ms | 0.016 ms | 1.26x |
| staging:edge-cases | unbounded-iterative-quantifier-expansion | 7 | 0.026 ms | 0.031 ms | 0.85x |

