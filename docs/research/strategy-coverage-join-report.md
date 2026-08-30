# The `strategy_coverage` table x measurement join

Measured at `719c2773bfdecbf8d4abedb690382a6197b826c9` by
`rust/crates/pg-foma/examples/strategy_coverage_join_report.rs`. Reproduce:

```
rust/tools/pg.ps1 -Mode run -Example strategy_coverage_join_report
```

`rust/crates/pg-foma/src/strategy_coverage.rs`'s table asserts, per `(EmissionStrategy,
CharacteristicKind)`, whether that compiler's proposer can represent that construct. It is
hand-curated and load-bearing, and until now was compared against nothing. This is that
comparison: every one of the table's 69 rows (23 `CharacteristicKind`s x 3 `EmissionStrategy`s)
against a real measurement over all 61 discovered conformance fixtures (`conformance-staging/**` +
`machine/conformance/**`).

## Why the join is not one-to-one, and which direction is sound

A fixture exercises several `CharacteristicKind`s (its authored `exercises:` tags,
`conformance_coverage::construct_ids_for`'s own vocabulary), so a fixture's aggregate per-strategy
outcome cannot be attributed to any ONE kind it exercises. Only one direction is airtight:

- **Sound.** A `CannotRepresent` row claims a compiler proposes NOTHING for a construct. If ANY
  fixture exercising that construct is measured **exact** (`Certification::FullHcConfirmed` --
  every comparable word in the fixture matched the live oracle) on that strategy, the row is
  **contradicted** -- "every word matched" already includes whichever word carried the tag, so no
  attribution is needed.
- **Not sound, reported only as `unsupported`.** A `Represents`/`RepresentsWithKnownGap` row claims
  a compiler CAN propose a construct. A fixture that measures NOT exact on that strategy may be
  failing on a *different* construct the same fixture also exercises, so absence of an exact
  witness is never a refutation.

See `rust/crates/pg-foma/src/strategy_coverage_join.rs` for the implementation
(`classify`/`classify_with_witnesses`) and `rust/crates/pg-foma/tests/strategy_coverage_join_gate.rs`
for the build-breaking gate over the sound direction.

## Headline

| | rows |
|---|---|
| Total table rows | 69 |
| **Agreed** | **50** |
| **Contradicted** | **0** |
| Unsupported | 19 |
| No-evidence | 0 |

**Zero contradictions.** Every `CannotRepresent` row in the table today --
`PlanComposed x RealizationalMorphology`, `PlanComposed x ProcessMorphology`, and
`TemplatedUnderlyingTokens x ProcessMorphology` -- survives the sound check: every fixture
exercising those constructs (four for `RealizationalMorphology`, three for `ProcessMorphology`)
either fails to compile or compiles without reaching oracle-exact output on the relevant strategy.
No table entry is refuted by this run.

Every one of the 61 discovered fixtures was measurable (no grammar-load or empty-corpus failures),
so `no-evidence` at zero is a real "every construct has at least one exhibiting fixture" fact, not
an artifact of fixtures this run could not reach.

## The 19 `unsupported` rows

None of these contradicts anything (see above) -- each names a table claim this run found no
positive witness for, because every fixture exercising that construct either doesn't compile on
that strategy at all, or compiles without reaching oracle-exact output for an unrelated reason.

| Strategy | CharacteristicKind | Why unsupported (no exact exhibiting fixture) |
|---|---|---|
| PlanComposed | OrderedMorphRuleApplication | both exhibiting fixtures (`polysynthetic-stratal-derivation-chain`, `backend-strata-generic`) fail to compile on every strategy (see below) |
| PlanComposed | UnorderedMorphRuleApplication | same two fixtures, same reason |
| PlanComposed | IterativeRewrite | exhibiting fixtures compile but miss oracle-exactness on `PlanComposed` specifically |
| PlanComposed | SimultaneousRewrite | same shape |
| PlanComposed | LeftToRightRewrite | same shape |
| PlanComposed | RightToLeftRewrite | same shape |
| PlanComposed | Metathesis | same shape |
| PlanComposed | Epenthesis | same shape |
| PlanComposed | SubruleGating | same shape |
| PlanComposed | Reduplication | same shape |
| PlanComposed | QuantifierPattern | same shape |
| TunedSurfaceProbed | OrderedMorphRuleApplication | same two all-refused fixtures |
| TunedSurfaceProbed | UnorderedMorphRuleApplication | same two all-refused fixtures |
| TemplatedUnderlyingTokens | OrderedMorphRuleApplication | same two all-refused fixtures |
| TemplatedUnderlyingTokens | UnorderedMorphRuleApplication | same two all-refused fixtures |
| TemplatedUnderlyingTokens | IterativeRewrite | exhibiting fixtures compile but miss oracle-exactness on `TemplatedUnderlyingTokens` specifically |
| TemplatedUnderlyingTokens | SimultaneousRewrite | same shape |
| TemplatedUnderlyingTokens | Epenthesis | same shape |
| TemplatedUnderlyingTokens | Reduplication | same shape |

11 of 19 are `PlanComposed` rows: that strategy compiles only a minority of fixtures at all (a
known, separate capability gap unrelated to any of these specific constructs), so most of its
`Represents` claims simply have no fixture reaching exact output to witness them from, regardless
of whether `PlanComposed` could represent the construct in isolation.

**One observation, not a contradiction and not a claim the table is wrong:**
`OrderedMorphRuleApplication` and `UnorderedMorphRuleApplication` are the only two
`CharacteristicKind`s with **zero positive evidence on any of the three strategies** in this
corpus -- their only two exhibiting fixtures (`machine:languages/polysynthetic-stratal-derivation-chain`,
`staging:edge-cases/backend-strata-generic`) fail to compile at all, on every strategy, for reasons
unrelated to rule ordering (`rep-variant-overflow` and an open strata-buildability gap
respectively). Every other unsupported row has at least one strategy with a real, agreed exact
witness. This is a conformance-corpus gap (two fixtures nothing can currently compile), not
evidence against the table -- flagged here as worth an eventual look, not acted on.

## The gate (`tests/strategy_coverage_join_gate.rs`)

Scoped to the 3 `CannotRepresent` rows only (the sound direction), not the full 69-row sweep this
report runs (which is reporting-only and takes a full-corpus compile+oracle pass -- see the example
above). Measured contradiction count today: **0**, so `CONTRADICTION_RATCHET = 0`. A regression
that makes any `CannotRepresent` row's construct measurably compile exact on that strategy fails
the build and prints the contradicting fixture.

A second test, `a_synthetic_cannot_represent_claim_is_contradicted_by_a_real_exact_fixture`,
injects a KNOWN-WRONG `CannotRepresent` claim (`TunedSurfaceProbed x RealizationalMorphology`,
which the real table does not claim -- `TunedSurfaceProbed` represents every kind) over real
measured data and asserts the join reports `Contradicted`, naming the real witnessing fixtures
(`machine:edge-cases/feature-gating-breadth`, `machine:languages/fusional-realizational-morphology`).
This is the only evidence, at a zero ratchet, that the mechanism actually fires rather than passing
vacuously.

## Full table (69 rows)

PC = `PlanComposed`, TSP = `TunedSurfaceProbed`, TUT = `TemplatedUnderlyingTokens`. Witness lists
truncated to 2 + a count where long; rerun the example for the full lists.

| Strategy | CharacteristicKind | Table claim | Verdict | Witness fixture(s) |
|---|---|---|---|---|
| PlanComposed | Affixation | Represents | agreed | m:edge-cases/diacritic-segments |
| PlanComposed | RealizationalMorphology | CannotRepresent | agreed | m:edge-cases/feature-gating-breadth; m:edge-cases/morphotactic-attribute-breadth; +2 more |
| PlanComposed | Compounding | Represents | agreed | m:edge-cases/compounding-breadth; s:edge-cases/recursive-endocentric-compounding |
| PlanComposed | OrderedMorphRuleApplication | Represents | unsupported | m:languages/polysynthetic-stratal-derivation-chain; s:edge-cases/backend-strata-generic |
| PlanComposed | UnorderedMorphRuleApplication | Represents | unsupported | m:languages/polysynthetic-stratal-derivation-chain; s:edge-cases/backend-strata-generic |
| PlanComposed | MprGroupAppend | Represents | agreed | m:edge-cases/mpr-group-overwrite-without-realizational |
| PlanComposed | MprGroupOverwrite | Represents | agreed | m:edge-cases/mpr-group-overwrite-without-realizational |
| PlanComposed | IterativeRewrite | Represents | unsupported | m:edge-cases/feature-system-breadth; m:languages/metathesis-phase-isolation; +6 more |
| PlanComposed | SimultaneousRewrite | Represents | unsupported | m:languages/suffixing-vowel-harmony; m:languages/templatic-root-modification; s:edge-cases/simultaneous-subrule-genuine-overlap |
| PlanComposed | LeftToRightRewrite | Represents | unsupported | m:edge-cases/mpr-gated-exception; s:edge-cases/backend-gated-generic; s:edge-cases/deletion-reduplication-exception-composite |
| PlanComposed | RightToLeftRewrite | Represents | unsupported | m:edge-cases/right-to-left-anchor-environment; s:edge-cases/right-to-left-bounded-quantifier-rewrite; +2 more |
| PlanComposed | Metathesis | Represents | unsupported | m:edge-cases/feature-system-breadth; m:edge-cases/metathesis-comparison-crash; +4 more |
| PlanComposed | Epenthesis | Represents | unsupported | m:edge-cases/feature-system-breadth; m:languages/metathesis-phase-isolation; +6 more |
| PlanComposed | SubruleGating | Represents | unsupported | m:edge-cases/subrule-morphosyntactic-gating |
| PlanComposed | CircumfixOutputAction | RepresentsWithKnownGap | agreed | m:edge-cases/diacritic-segments |
| PlanComposed | Reduplication | Represents | unsupported | m:languages/metathesis-phase-isolation; m:languages/suffixing-extension-slot-ordering; +3 more |
| PlanComposed | CoOccurrenceConstraint | Represents | agreed | m:languages/suffixing-evidential-adjacency-chain |
| PlanComposed | NaturalClassDefinition | Represents | agreed | m:edge-cases/diacritic-segments; m:edge-cases/strrep-identity |
| PlanComposed | MultiTable | Represents | agreed | m:edge-cases/bistratal-overlapping-segment-representation |
| PlanComposed | QuantifierPattern | Represents | unsupported | m:edge-cases/loader-pattern-shapes; s:edge-cases/right-to-left-bounded-quantifier-rewrite; +1 more |
| PlanComposed | StemName | Represents | agreed | m:edge-cases/stem-name-restricted-root-allomorph |
| PlanComposed | FreeFluctuation | Represents | agreed | m:edge-cases/disjunctive-recheck; m:edge-cases/free-fluctuating-allomorph-pair; m:languages/suffixing-evidential-adjacency-chain |
| PlanComposed | ProcessMorphology | CannotRepresent | agreed | m:edge-cases/process-morphology-in-place-mutation; m:languages/fusional-realizational-morphology; m:languages/templatic-root-modification |
| TunedSurfaceProbed | Affixation | Represents | agreed | m:edge-cases/diacritic-segments; m:edge-cases/feature-system-breadth; +9 more |
| TunedSurfaceProbed | RealizationalMorphology | Represents | agreed | m:edge-cases/feature-gating-breadth; m:languages/fusional-realizational-morphology |
| TunedSurfaceProbed | Compounding | Represents | agreed | m:edge-cases/compounding-breadth; m:languages/fusional-realizational-morphology; s:edge-cases/recursive-endocentric-compounding |
| TunedSurfaceProbed | OrderedMorphRuleApplication | Represents | unsupported | m:languages/polysynthetic-stratal-derivation-chain; s:edge-cases/backend-strata-generic |
| TunedSurfaceProbed | UnorderedMorphRuleApplication | Represents | unsupported | m:languages/polysynthetic-stratal-derivation-chain; s:edge-cases/backend-strata-generic |
| TunedSurfaceProbed | MprGroupAppend | Represents | agreed | m:edge-cases/mpr-group-overwrite-without-realizational; m:languages/fusional-realizational-morphology |
| TunedSurfaceProbed | MprGroupOverwrite | Represents | agreed | m:edge-cases/mpr-group-overwrite-without-realizational; m:languages/fusional-realizational-morphology |
| TunedSurfaceProbed | IterativeRewrite | Represents | agreed | m:edge-cases/feature-system-breadth; m:languages/metathesis-phase-isolation; +3 more |
| TunedSurfaceProbed | SimultaneousRewrite | Represents | agreed | m:languages/suffixing-vowel-harmony; m:languages/templatic-root-modification; s:edge-cases/simultaneous-subrule-genuine-overlap |
| TunedSurfaceProbed | LeftToRightRewrite | Represents | agreed | m:edge-cases/mpr-gated-exception; s:edge-cases/backend-gated-generic; s:edge-cases/deletion-reduplication-exception-composite |
| TunedSurfaceProbed | RightToLeftRewrite | Represents | agreed | m:edge-cases/right-to-left-anchor-environment; s:edge-cases/right-to-left-bounded-quantifier-rewrite; +2 more |
| TunedSurfaceProbed | Metathesis | Represents | agreed | m:edge-cases/feature-system-breadth; m:languages/metathesis-phase-isolation; +3 more |
| TunedSurfaceProbed | Epenthesis | Represents | agreed | m:edge-cases/feature-system-breadth; m:languages/metathesis-phase-isolation; +3 more |
| TunedSurfaceProbed | SubruleGating | Represents | agreed | m:edge-cases/subrule-morphosyntactic-gating |
| TunedSurfaceProbed | CircumfixOutputAction | Represents | agreed | m:edge-cases/diacritic-segments; m:edge-cases/feature-system-breadth; +8 more |
| TunedSurfaceProbed | Reduplication | Represents | agreed | m:languages/metathesis-phase-isolation; s:edge-cases/backend-ordered-generic; +2 more |
| TunedSurfaceProbed | CoOccurrenceConstraint | Represents | agreed | m:languages/suffixing-evidential-adjacency-chain; m:languages/templatic-root-modification |
| TunedSurfaceProbed | NaturalClassDefinition | Represents | agreed | m:edge-cases/diacritic-segments; m:edge-cases/feature-system-breadth; +3 more |
| TunedSurfaceProbed | MultiTable | Represents | agreed | m:edge-cases/bistratal-overlapping-segment-representation; s:edge-cases/multi-table-metathesis-shared-representation; +2 more |
| TunedSurfaceProbed | QuantifierPattern | Represents | agreed | m:edge-cases/loader-pattern-shapes; s:edge-cases/right-to-left-bounded-quantifier-rewrite; +1 more |
| TunedSurfaceProbed | StemName | Represents | agreed | m:edge-cases/stem-name-restricted-root-allomorph; m:languages/fusional-realizational-morphology; m:languages/templatic-root-modification |
| TunedSurfaceProbed | FreeFluctuation | Represents | agreed | m:edge-cases/disjunctive-recheck; m:edge-cases/free-fluctuating-allomorph-pair; m:languages/suffixing-evidential-adjacency-chain |
| TunedSurfaceProbed | ProcessMorphology | Represents | agreed | m:languages/fusional-realizational-morphology; m:languages/templatic-root-modification |
| TemplatedUnderlyingTokens | Affixation | Represents | agreed | m:edge-cases/diacritic-segments; s:edge-cases/circumfix-in-template-slot |
| TemplatedUnderlyingTokens | RealizationalMorphology | Represents | agreed | m:edge-cases/feature-gating-breadth |
| TemplatedUnderlyingTokens | Compounding | Represents | agreed | m:edge-cases/compounding-breadth; s:edge-cases/recursive-endocentric-compounding |
| TemplatedUnderlyingTokens | OrderedMorphRuleApplication | Represents | unsupported | m:languages/polysynthetic-stratal-derivation-chain; s:edge-cases/backend-strata-generic |
| TemplatedUnderlyingTokens | UnorderedMorphRuleApplication | Represents | unsupported | m:languages/polysynthetic-stratal-derivation-chain; s:edge-cases/backend-strata-generic |
| TemplatedUnderlyingTokens | MprGroupAppend | Represents | agreed | m:edge-cases/mpr-group-overwrite-without-realizational |
| TemplatedUnderlyingTokens | MprGroupOverwrite | Represents | agreed | m:edge-cases/mpr-group-overwrite-without-realizational |
| TemplatedUnderlyingTokens | IterativeRewrite | Represents | unsupported | m:edge-cases/feature-system-breadth; m:languages/metathesis-phase-isolation; +6 more |
| TemplatedUnderlyingTokens | SimultaneousRewrite | Represents | unsupported | m:languages/suffixing-vowel-harmony; m:languages/templatic-root-modification; s:edge-cases/simultaneous-subrule-genuine-overlap |
| TemplatedUnderlyingTokens | LeftToRightRewrite | Represents | agreed | m:edge-cases/mpr-gated-exception; s:edge-cases/backend-gated-generic |
| TemplatedUnderlyingTokens | RightToLeftRewrite | Represents | agreed | m:edge-cases/right-to-left-anchor-environment; s:edge-cases/right-to-left-bounded-quantifier-rewrite; +2 more |
| TemplatedUnderlyingTokens | Metathesis | Represents | agreed | s:edge-cases/multi-table-metathesis-shared-representation; s:edge-cases/right-to-left-metathesis-reversal |
| TemplatedUnderlyingTokens | Epenthesis | Represents | unsupported | m:edge-cases/feature-system-breadth; m:languages/metathesis-phase-isolation; +6 more |
| TemplatedUnderlyingTokens | SubruleGating | Represents | agreed | m:edge-cases/subrule-morphosyntactic-gating |
| TemplatedUnderlyingTokens | CircumfixOutputAction | RepresentsWithKnownGap | agreed | m:edge-cases/diacritic-segments; s:edge-cases/circumfix-in-template-slot |
| TemplatedUnderlyingTokens | Reduplication | Represents | unsupported | m:languages/metathesis-phase-isolation; m:languages/suffixing-extension-slot-ordering; +3 more |
| TemplatedUnderlyingTokens | CoOccurrenceConstraint | Represents | agreed | m:languages/suffixing-evidential-adjacency-chain |
| TemplatedUnderlyingTokens | NaturalClassDefinition | Represents | agreed | m:edge-cases/diacritic-segments |
| TemplatedUnderlyingTokens | MultiTable | Represents | agreed | s:edge-cases/multi-table-metathesis-shared-representation; s:edge-cases/two-table-shared-representation-recall |
| TemplatedUnderlyingTokens | QuantifierPattern | Represents | agreed | s:edge-cases/right-to-left-bounded-quantifier-rewrite; s:edge-cases/unbounded-iterative-quantifier-expansion |
| TemplatedUnderlyingTokens | StemName | Represents | agreed | m:edge-cases/stem-name-restricted-root-allomorph |
| TemplatedUnderlyingTokens | FreeFluctuation | Represents | agreed | m:edge-cases/disjunctive-recheck; m:edge-cases/free-fluctuating-allomorph-pair; m:languages/suffixing-evidential-adjacency-chain |
| TemplatedUnderlyingTokens | ProcessMorphology | CannotRepresent | agreed | m:edge-cases/process-morphology-in-place-mutation; m:languages/fusional-realizational-morphology; m:languages/templatic-root-modification |

## What this does NOT assert

Same discipline as `docs/research/pg-foma-conformance-coverage-gate-notes.md`: `agreed` on a
`Represents` row means "at least one exhibiting fixture reached oracle-exact output", not "every
configuration of that construct is representable" -- row-level agreement and configuration-level
completeness are different questions. And `unsupported` is never evidence the table is wrong; it
is evidence the corpus does not currently demonstrate the claim, for reasons this report deliberately
does not attribute further.
