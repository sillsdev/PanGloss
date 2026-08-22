# Filter-reach shipping specification

## Outcome

Ship the five-language/backend-characterization work without permitting an incomplete FST to look
successful. The branch is complete only after the required tests pass, temporary diagnostics are
removed, and the reviewed changes are committed and pushed.

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
5. Mbugwe may cleanly produce no trusted FST. TunedSurface currently reports a resource Error;
   TemplatedUnderlyingTokens and PlanComposed report correctness Critical findings. The full
   morphological parser remains an analysis path, not proof of FST completeness.
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
- linked remedy IDs and plain-English remedy text; and
- the mandatory language-validity safety statement on potentially meaning-changing remedies.

Build integration generates deterministic output and verifies it against the checked-in cards.
The remaining implementation choice is whether a normal build only fails with an explicit
regeneration command on drift (recommended), or rewrites the tracked cards automatically.

## Verification and delivery

- Resolve every failure from the 1,067-test `pg-foma` run without weakening normal fail-closed
  construction.
- Re-run focused regression targets, the two PanGloss-only completeness targets, five-language
  backend reports, Mbugwe corpus smoke, package tests, CLI/pack tests, and repository hygiene.
- Run the authoritative package/full-suite checks after focused tests are green.
- Remove diagnostic output hooks and exclude `.tmp/`, the transcript, and the intentional dirty
  `machine` pointer from the commit.
- Inspect the final diff, commit on `filter-reach`, and push the branch.

## Explicitly out of scope

FLExText ingestion, LibLCM cache export/import, Motif orchestration, text filtering, UI presentation,
and promotion of the PanGloss-only fixtures are separate work.
