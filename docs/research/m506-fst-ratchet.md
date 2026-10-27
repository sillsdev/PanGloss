# PR #9 FST coverage ratchets

Classification: **correctness/representability**. These measurements compare the actual proposal
and confirmation results with full HC-Rust. They do not establish production readiness or turn an
incomplete proposal set into a complete one. This change adjusts only tests' accounting baselines;
it does not alter a backend's admission, compiler, output, or proof.

The baseline is `origin/main` `14975991` at Machine `aab4c598`; the measured PR #9 base is
`0080a68c`, with the same Machine pin. The [cell comparison](evidence/m506-fst/scoreboard-comparison.tsv)
uses the earlier main pin measurement in `parser-settings-variants/scoreboard-pin-comparison.tsv`
and CI run 40046409690's `SCOREBOARD_CELL` rows. All 396 existing fixture/strategy cells have the
same outcome in both directions. Four staged fixtures add 12 cells: seven exact and five
`compiles_but_misses`. The new counts are:

| Strategy | Exact | Misses | Refused | Unmeasurable |
|---|---:|---:|---:|---:|
| Tuned surface probed | 126 | 8 | 2 | 0 |
| Templated underlying tokens | 102 | 9 | 25 | 0 |
| Plan composed | 47 | 3 | 83 | 3 |

The new `truncate-morphotactic-copy-u` fixture misses under all three strategies; the new
`truncate-morphotactic-identity` and `truncate-morphotactic-subsumption` miss under templated
underlying tokens. `final-template-partial-root` is exact for all three. The corrected full-HC
identities and the new fixtures expose actual proposal gaps; the count adjustment leaves them
visible in the scoreboard output rather than treating them as exact. Soundness remains a strict
zero-tolerance assertion.

A local main replay observed 133 fixtures and 38 held, 22 failed, 12 not-attempted pairs.
The [failure comparison](evidence/m506-fst/faithfulness-failures.tsv) shows the 22 main failures
are unchanged and identifies both added pairs. The faithfulness report at the PR #9 base observed
137 fixtures and all 24 construct kinds:
36 pairs held, 24 failed, and 12 were not attempted. The two pairs newly above the old 22-pair
ratchet are CircumfixOutputAction under plan-composed and templated-underlying-tokens, exhibited
by the copy-U fixture's `gau`: an HC identity is absent from the proposal set. Other misses in
that fixture overlap existing failed construct/backend pairs. The measured soundness count was
zero. `NoMoreThan { failures: 24 }` records this larger tested inventory; it does not admit a
backend or assert that a missing identity is representable by its current proposer. It should be
lowered when the proposer gaps are fixed, and every future regression above it remains a failure.

The two CI failures are the scoreboard test in `backend_strategy` and the faithfulness report in
`evidence_contracts`, not four independent failures. Local runs of the same two tests on the
untouched PR #9 base reproduced the CI outcomes. The underlying Machine #500 and #556 oracle
changes are not yet merged, so PR #9 remains draft pending those decisions.
