# Machine pin FST measurements

The Machine conformance pin moves from `aab4c59819adb7b366915e09df2fb28c62b8c03a`
to `4a66977f6587033f6dbc800e6b844c0770dbde0b`. The latter was confirmed as PR #480's head.
These measurements classify **correctness/representability**, not production readiness or
resource containment. The faithfulness ratchet counts observed missing proposals; it does not
admit a grammar or prove incomplete output correct. The soundness requirement remains zero.

Before changing the ratchets, compare main `14975991f65dcbd290c2a58c30ecd894247d0851`
with issue #8 fix `52753e61b3c944c66a9d8b8155ddc56cbf8afa27`, each using the new pin
and removing the same 66 graduated staged duplicates. The fixture inventory differs by the
issue #8 fixture. Faithfulness measured 35 held, 25 failed, and 12 not attempted on each;
all 25 failing construct/strategy pairs are identical, and both measured zero soundness
violations. The exact pairs are in [faithfulness-comparison.tsv](evidence/machine-pin/faithfulness-comparison.tsv).

The [scoreboard comparison](evidence/machine-pin/scoreboard-comparison.tsv) contains every
measured cell on either side. All 432 shared cells agree; the extra issue #8 fixture is exact
for tuned surface and templated underlying strategies and refused for plan composed. The
new inventory yields the following buckets in exact / misses / refused / unmeasurable order:

| Strategy | Counts |
| --- | --- |
| TunedSurfaceProbed | 134 / 9 / 2 / 0 |
| TemplatedUnderlyingTokens | 107 / 12 / 26 / 0 |
| PlanComposed | 51 / 3 / 88 / 3 |

One crash-expectation fixture remains explicitly excluded from scoreboard measurement. These
are observation baselines, including known missing proposals, not evidence of full coverage.

Graduation replaced the local variant-specific exercise tags with Machine's shared construct
vocabulary. `UPSTREAM_VARIANT_TAG_EXTENSIONS` preserves the 60 fixture/variant pairs actually
authored in the pre-graduation staged `words.yaml` files at `52753e61`. A mapping contributes
only when the upstream word carries the original kind's construct tag, the word passes replay,
and the owning predicate observes the variant. The coverage gate still checks invalid tags,
fixture gaps, and containment gaps; its thresholds were not relaxed. The fixture-specific
refusal and cross-table checks resolve graduated fixtures from the upstream root.
