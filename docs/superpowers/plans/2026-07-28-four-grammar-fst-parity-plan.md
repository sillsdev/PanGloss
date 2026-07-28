# Four-Grammar FST Parity Implementation Plan

## Goal

Reach correct PanGloss/Foma analysis parity for the four currently refused Machine
grammars, graduate the four missing HermitCrab conformance constructs, and leave
the recipe system able to classify, compose, rank, and explain the selected FST
construction.

The known Machine infinite-loop fixture is reported as an expected reference
crash; it is not reproduced in PanGloss.

## 1. Establish the real failure boundary

1. Run the Machine conformance driver against the Foma adapter with capability
   enforcement disabled.
2. Record, per grammar, whether the existing compiled proposer is complete and
   whether only the capability classifier refuses it.
3. Prefer promoting a proven-complete existing recipe over adding a parallel
   implementation. Add new compilation machinery only where candidate
   containment or analysis parity actually fails.

## 2. Add red recipe and parity tests

1. Add focused tests for the two structural-allomorph grammars:
   fusional-realizational morphology and templatic root modification.
2. Add focused tests for MPR overwrite in fusional-realizational morphology and
   suffixing extension slot ordering.
3. Add a focused metathesis test covering the anchored complex pattern used by
   metathesis phase isolation, plus a bounded repeated regular context.
4. Assert that each supported construct receives a non-refusing capability
   disposition and that the complete analyzer remains the final oracle.
5. Add a four-grammar integration test that compares compiled candidates and
   complete analyses for every fixture word.

## 3. Implement composable recipes

### Structural allomorph recipe

1. Reuse the general structural-composite relation when it is already active.
2. Extend the lightweight templated structural layer only for regular,
   allomorph-local Copy/Insert/finite-Modify shapes not covered by that relation.
3. Bind the resulting capability evidence to the structural-affix recipe,
   without grammar-name special cases.

### MPR overwrite recipe

1. Model the finite MPR state needed by the affected slot and group constraints.
2. Compose state-specific lexicon admission with legal morphology transitions.
3. Keep complete analysis as confirmation and classify the feature as
   ConfirmOnly unless exact compiled completeness has been demonstrated.

### Metathesis cascade recipe

1. Lower anchors and bounded regular context around switch candidates.
2. Preserve mirror/reverse semantics and phase-local cascade ordering.
3. Reject only genuinely non-regular or unbounded constructions, with an
   explicit explanation.

## 4. Register and rank recipe combinations

1. Add registry entries for the structural-affix, MPR-state, and
   metathesis-cascade recipes.
2. Bind emitted evidence to those entries.
3. Add bounded-enumeration tests showing that the four grammars select one of
   the supported combinations deterministically and that the ranking explanation
   reports completeness, risk, and cost.

## 5. Graduate conformance coverage

1. Promote minimal witnesses for:
   - multiple character-definition tables, one per stratum;
   - left-to-right rewrite direction;
   - right-to-left rewrite direction;
   - subrule-level POS/MPR required and excluded gating.
2. Normalize invalid XML comments before running the Machine oracle.
3. Regenerate Machine coverage artifacts and require 28/28 in-scope constructs.
4. Normalize the conformance shell scripts to LF so the documented commands run.

## 6. Verify and publish

Run, in order:

1. Focused Rust unit and integration tests for each new recipe.
2. Full `pg-foma` tests.
3. Machine self-check, including pathological fixtures.
4. Machine parity coverage check: 28/28.
5. PanGloss default-adapter conformance.
6. PanGloss Foma-adapter conformance with capability enforcement.
7. Combined typology/conformance run and recipe ranking tests.
8. Review the complete diff, commit the Machine fixture changes, update the
   PanGloss submodule, commit PanGloss, fast-forward local `main`, and push both
   required remote branch tips.

## Completion criteria

- All non-pathological words in all Machine and PanGloss conformance grammars
  pass under the complete and compiled paths.
- The only non-pass result is the explicitly classified expected Machine
  reference crash.
- Machine coverage reports 28/28.
- The four target grammars have deterministic recipe selections and no
  unsupported-feature refusal.
- The work is committed on PanGloss `main` and pushed to `origin/main`.
