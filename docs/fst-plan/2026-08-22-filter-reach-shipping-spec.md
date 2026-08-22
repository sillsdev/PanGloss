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
