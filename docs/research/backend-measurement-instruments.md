# What can and cannot measure a backend's correctness

Two competent reviews of the same nine fixtures reached opposite conclusions about whether the
templated backend over- and under-generates. Adjudicating that produced a finding larger than the
disagreement: **the repo has no canonical way to ask the question.** This file records what each
instrument actually measures, so the next reader does not have to re-derive it.

## The pipeline everything below assumes

`propose` (FST, per `EmissionStrategy`) -> `confirm` (`pg-foma/src/confirm.rs`, a **restricted**
`pg_parse::Morpher` reparse narrowed to the candidate's own proposed rule/root set) -> output.

The **full-HC oracle** is the same `pg_parse::Morpher` run **unrestricted**. It is authoritative for
exactly one thing: what the grammar licenses for a word. It is recomputed live at measurement time
(`RunEvaluationCache::prepare`), **not** read from `words.yaml`.

## The instruments

| Instrument | Compares | Direction | Cannot tell you |
|---|---|---|---|
| `certify_word` -> `Certification` | Post-confirm output vs. live oracle, identity **set equality** | Both, conflated | Which DIRECTION a mismatch is |
| `IdentityDivergence` (`parity.rs`) | Same, but counts `oracle_only_identities` and `candidate_only_identities` **separately** | Both, separated | Nothing -- if you can reach it. See defect 1 |
| `word_proposal_containment` / `faithfulness_coverage_gate` | **Pre-confirm** raw proposals vs. live oracle, admission-key containment | Recall ONLY, by design | Anything about over-generation, or about the confirmed output a caller receives |
| `parity_divergence_census` | Post-confirm `candidate_only_identities` | Soundness ONLY, strict | Two of three backends -- see defect 3 |
| `witnessed_strategy_coverage_gate` | Did a backend COMPILE a network | n/a | Anything about correctness. The coverage-inheritance trap |
| `envelope_agrees_with_compiler_gate` | Envelope admit/refuse vs. whether a compile builds | n/a | Anything about the built network's correctness |
| `conformance_fixtures_gate` (pg-parse) | Unbounded oracle vs. `words.yaml`'s own transcript | Both | Anything about any FST backend -- it never touches pg-foma |

**Recall has a near-canonical instrument. Soundness does not.**

## Four defects

**1. `IdentityDivergence` is computed and discarded at the boundary every gate uses.** It is the one
place in the crate that separates a recall failure from a soundness hazard with typed fields.
`evaluate_plans_observed_with_cache` and `evaluate_plans_with_cache` both drop `result.divergence`
before any caller sees it; it survives only via `RunEvaluationCache::identity_divergence()`, which
`faithfulness_coverage`'s own `observe_fixture_containment` never calls. This is the same shape as
this session's four other "ran, returned, enforced nothing" defects.

**2. `Certification::IdentityMismatch` conflates two opposite-direction defects into one verdict**,
recoverable only by parsing a free-text `detail` string. Callers matching it structurally cannot
tell a recall miss from a surviving over-generation. This is the ambiguity that let two reviews
assign the same verdict to opposite classes.

**3. No soundness gate exists for two of three backends.** `parity_divergence_census` is scoped to
`PlanComposed` and capped at 8 words per fixture. `TunedSurfaceProbed` and
`TemplatedUnderlyingTokens` -- the two whole-grammar backends, one of which carries all five
reference languages -- have none. An over-generation regression on either is undetectable by any
strict gate.

**4. `words.yaml` is not the ground truth any backend instrument uses.** Roughly half a fixture's
entries are deliberate negative controls (`expect_fail` / `expect_skip`, sometimes with `blocked_by`
naming the construct that would admit the word). Backend instruments never read those fields; they
recompute expectations live. A reader who takes every listed `word:` as "should parse" derives a
different expected set than the tooling checks against -- the mechanism behind two of the four
disputed witness forms.

Related but not a bug: oracle step-caps differ between instrument families (`usize::MAX` in
`conformance_fixtures_gate`, `DEFAULT_ORACLE_STEP_CAP` elsewhere). Capped words are excluded by name
rather than silently, so this is honest; it is only a hazard when comparing across families.

## What the canonical instrument should be

One function, per (fixture, backend, word), reporting three counts:

- `oracle_only` -- a real defect, recall. ADR-0001's "never miss".
- `candidate_only_pre_confirm` -- **legal** per ADR-0001, informational, already bounded by
  `check_proposal_ratio`.
- `candidate_only_post_confirm` -- a real defect, soundness, regardless of ADR-0001.

`IdentityDivergence` already computes this split. The work is exposing it through the API every gate
already calls, not building it.

## Two rules for anyone measuring a backend

**Judge a proposer by what the pipeline CONFIRMS, never by what the network ACCEPTS.** ADR-0001
permits the proposer to overgenerate; the confirm step prunes. A form accepted raw and pruned by
confirm is correct behaviour, not a defect.

**A capability-refused backend is `NotAttempted`, not "passing".** `select_backends` gates the recall
instrument, so a refused backend is never measured. To measure one, bypass the selector and call
`evaluate_plans_observed_with_cache` or `compile_with_backend` directly -- which
`envelope_agrees_with_compiler_gate` and `parity_divergence_census` already do deliberately.
