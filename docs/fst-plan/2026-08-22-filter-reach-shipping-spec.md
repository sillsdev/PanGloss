# Filter-reach shipping specification

## Outcome

Ship the three-language backend-characterization work for Indonesian, Amharic, and Aweti without
permitting an incomplete FST to look successful. Mbugwe is explicitly deferred and is not an
acceptance blocker for this deliverable. The branch is complete only after the required tests pass,
temporary diagnostics are removed, and the reviewed changes are committed and pushed.

This specification is a delivery contract, not evidence that any of these languages already has a
trusted shipped FST. A language is accepted only after its selected route has a complete payload,
certificate, and the required runtime evidence.

| Language | Code path | Bounded corpus evidence | Certified artifact | Trusted shipped FST |
|---|---|---|---|---|
| Indonesian | Foma propose→confirm and replace-rule prototype paths exist; current construction is not identity-bound. | Historical 121/121 F3, separate 114-case P6, and separate 97/97 non-redup P6 scopes; denominators are not interchangeable without an exact case mapping. | None | None |
| Amharic | Templated/replace prototype code paths exist. | Bounded comparison: 622 cases, 51 engine-timeout exclusions, 0 mismatches; not timeout-free. | None | None |
| Aweti | Templated-underlying prototype code path exists, including the 18-rule composition path. | 100/106 oracle-bearing words, with six gaps. | None | None |

Plain-English reading: a code path is an exercised implementation route; bounded evidence is a
scoped measurement; a certified artifact has a valid completeness certificate; and a trusted shipped
FST is a certified artifact accepted for delivery. None of the three current languages reaches the
last two columns.

## Terminology

In this work, a **computationally awkward FieldWorks grammar** may be described informally as a
“bad grammar” only in this narrow sense: the project models a valid language with constructs that
overgenerate candidates or interact less efficiently than a more precise, language-valid
FieldWorks representation might. It is not a claim that the language, analyses, or author are bad.
Any suggested restructuring remains conditional and must include: “Don't make any change that
would make your language invalid!”

## Required behavior

1. Every executable backend produces a compatibility report, including backends that refuse or
   fail. Reports retain findings, predicates, shapes, cost evidence, advice references, and status.
2. Correctness/completeness failures are Critical. Resource-envelope excess is Error. Normal builds
   fail closed on either. An explicit development override may produce an indelibly unproven
   artifact; worker/apply execution containment remains non-overrideable.
3. A completeness certificate exists only when a real FST payload was built, the emitter reported
   Full, no constructs were uncovered, no successors remained, and no enumeration budget tripped.
4. The selector reports warnings and errors for every backend and chooses only a backend with no
   Error or Critical finding. Ranking prefers fewer/lower findings before backend preference.
5. Mbugwe is deferred from the current three-language acceptance slice and is not an acceptance
   blocker. Its existing reports and full morphological parser remain future reference only; the
   parser is an analysis path, not proof of FST completeness.
6. The two regression grammars under
   `rust/crates/pg-foma/tests/fixtures/pangloss/fst-completeness/` remain PanGloss-internal and are
   never promoted to Machine.
7. Capability cards are static and machine-independent. Language, corpus, timing, and machine
   measurements belong only in per-build compatibility/build reports.

## Capability-card contract

Generate one checked-in Markdown card for each executable backend:

- `tuned-surface-probed`;
- `templated-underlying-tokens`; and
- `plan-composed`.

Each card is generated from a single versioned capability catalog and contains:

- stable backend and envelope IDs plus human names;
- whether each envelope is inherent or controlled by a named switch, including its default;
- Big-O notation, named variables, and which ordering, null, deletion, or other features contribute;
- linked remedy IDs and a reference to the authoritative shared advice catalog; and
- the mandatory language-validity safety statement on potentially meaning-changing remedies.

The explicit generator produces deterministic output for the checked-in cards. Ordinary builds do
not validate or rewrite these human- and AI-readable artifacts; the static catalog remains the
source of truth, and the generated cards may be refreshed deliberately when that catalog changes.
Card presence or metadata is not evidence that a language has a trusted build.

## Historical lessons for the three-language slice

These observations explain why the current work adds capability envelopes and artifact evidence;
they are historical context, not current shipping claims.

- **Indonesian:** the older construction had strong corpus evidence, including 121/121 parity, but
  that result did not by itself bind an exact envelope to a reproducible, complete artifact. See
  [`foma-fst-plan.md`](foma-fst-plan.md), the historical F3 verdict, and commit `87320bff`.
- **Amharic:** the older mainline result compared 622 cases with 51 engine-timeout exclusions and
  zero mismatches. That is useful bounded evidence, not timeout-free grammar-wide certification.
  See [`foma-fst-plan.md`](foma-fst-plan.md) and commit `87320bff`.
- **Aweti:** the templated prototype compiled all 18 rules and recalled 100/106 oracle-bearing
  words; six real morphology/rule gaps remain. See [`synthetic-stress-grammar-plan.md`](synthetic-stress-grammar-plan.md),
  [`2026-08-20-aweti-enum-budget-census.md`](2026-08-20-aweti-enum-budget-census.md), and commit
  `9b06b102`.

### Lessons carried forward / traceability

Each item below turns historical evidence into a current invariant. The cited OpenSpec task/gate
is the consumer; these are not new test results or claims that a trusted artifact already exists.

- **No single old builder solved all three languages.** The P6 rule compiler reached Indonesian
  parity and Amharic alpha-tuple scale, while the underlying emitter was still template-less and
  Aweti needed a different route. Retain per-grammar capability selection; reject one universal
  builder or language-specific shortcut. Consumed by tasks 2.1–4.3 and 6.1.
- **`hc-hybrid` retirement is intentional.** Commit `9a89a32c` removed the old proposer after the
  propose→confirm architecture was gated. Retain Foma proposal plus real-engine confirmation and
  fail closed; reject reviving a hidden full-engine/FST fallback. Consumed by tasks 7.1–7.2.
- **Indonesian bridges were useful but not proof.** Junction probing, static MPR/POS partitioning,
  and real-engine confirmation supplied upward candidates and bounded parity (121/121, 114, and
  97/97 are distinct scopes). Retain upward-only proposals and HC set confirmation; reject treating
  corpus parity, candidate identity, or a partial closure as a certified artifact. Consumed by
  tasks 3.1–3.6, 6.4, and 7.3.
- **Indonesian needs complete closure and exact artifact binding.** `8997cdac` made incomplete
  composite closure refuse, and `87320bff` recorded the fail-closed artifact boundary, while the 114-case retry was only static admission. Retain empty
  worklists, completeness certificates, corpus/grammar identity, envelope, and network fingerprint;
  reject depth-based success or static admission standing in for a built artifact. Consumed by
  tasks 4.2–4.3, 6.4, and 7.1.
- **Amharic successes remain bounded.** Interdigitation and 20-variable alpha tuples (312
  survivors), plus static partitioning, establish code-path evidence; the 622-case comparison
  excluded 51 engine timeouts. Retain those mechanisms and exact analysis-set comparison; reject
  timeout-free or grammar-wide claims. Consumed by tasks 3.3–3.6, 6.2, and 6.4.
- **Aweti's scale result selects architecture, not a pass.** The census measured 3,093,412 eager
  composites (the budget latch is only a floor); `dfb5025f`/`f892cfd0` show the underlying route
  compiles all 18 rules but recalls 100/106. Retain templated-underlying composition and bounded
  counters; reject eager budget inflation, unsound filtering, and depth caps. Consumed by tasks
  4.1–4.3 and 6.3–6.4.
- **The Aweti truncation hypothesis was false.** `fa81ec82` found 0/16 gain: the apparent drops were
  floating-segment realization, and post-hoc boundary deletion regressed `apply_up`. Retain only
  explicit classified structural recipes with cleanup after phonology; reject `rhs_drops_lhs_material`
  as a deletion proxy and post-hoc boundary deletion. Consumed by tasks 2.4, 3.6, and 4.2.
- **Fixed-depth and truncated closure are never success.** Historical chain work reduced ambiguity,
  but incomplete worklists still refuse and the remaining Aweti misses remain open. Retain native
  loops, finite closure, or typed refusal; reject fixed-depth returns, early stopping, and partial
  artifacts. Consumed by tasks 4.2–4.3 and 7.1.
- **Identity and marker handling must be closed under every consumer.** The Aweti marker experiments
  require one known marker consumed exactly once; the historical Foma-rs literal-`0` tag bug showed
  that `fsm_intersect` can fail even when `apply_up` appears correct. Retain source-level tag
  encoding, marker isolation/leak counters, and consumer regression gates; reject unsafe identity
  fallback, marker leakage, and trusting one API's view. Consumed by tasks 3.5–3.6, 4.1–4.4, and 6.4.
- **Selected and realized routes must be the same artifact.** Keep-old-paths and the Aweti cascade
  experiment showed that a smaller network can still lose recall. Retain `preferred == selected ==
  realized`, immutable envelope metadata, and a fingerprint for the measured network; reject
  language-ID switches, stale profiles, and un-fingerprinted substitutions. Consumed by tasks 6.1,
  6.4, and 7.1.
- **Semantic equality and deterministic counters outrank elapsed time.** Historical gates exposed
  losses hidden by reachability, containment, or timing; Aweti diagnostics require exact canonical
  analysis sets and reproducible recipe/state/arc/timeout counters. Retain exact set equality and
  deterministic evidence; reject elapsed-time success, first-analysis agreement, or containment
  as recall proof. Consumed by tasks 6.2–6.4 and 7.2.
- **Construct coverage must be explicit.** The cascade experiment lost 6/25 on process-morphology
  shapes despite smaller networks, and P6 left RTL, simultaneous, quantifier, and metathesis
  boundaries. Retain synthetic positive/negative witnesses and honest skips/refusals; reject
  generalizing from the three reference grammars. Consumed by tasks 2.1–2.6 and 5.2–5.3.

## Verification and delivery

- Resolve every failure from the 1,066-test `pg-foma` run without weakening normal fail-closed
  construction.
- Re-run focused regression targets, the two PanGloss-only completeness targets, Indonesian/
  Amharic/Aweti backend reports, package tests, CLI/pack tests, and repository hygiene. Mbugwe
  corpus smoke is deferred with the language, not a release gate for this slice.
- Run the authoritative package/full-suite checks after focused tests are green.
- Remove diagnostic output hooks and exclude `.tmp/`, the transcript, and the intentional dirty
  `machine` pointer from the commit.
- Inspect the final diff, commit on `filter-reach`, and push the branch.

## Explicitly out of scope

FLExText ingestion, LibLCM cache export/import, Motif orchestration, text filtering, UI presentation,
and promotion of the PanGloss-only fixtures are separate work.
