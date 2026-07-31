# Cleanup and recipe parity

## Why

Fresh four-corpus measurement (2026-07-30) shows the recipe optimizer beats the hand-spun compiler
on one corpus, ties-with-an-explanation on a second, and fails to finish or fully certify on the
other two — and a seven-agent research pass traced every one of those gaps to a specific,
addressable cause: a mis-wired emitter (the plan-composed path routes templated grammars through
`uflexc`, documented as not generalizing to them), provably-redundant per-candidate work (the
grammar-invariant oracle parse and `emit()` report recompute once per candidate), a ranking key
that is structurally blind to propose-side cost, and dead/phantom code that misleads readers
(`ComposeStrategy::Lazy`, an inert branch-and-bound whose `pruned` is always 0). The full evidence
record is `docs/fst-plan/recipe-parity-plan-2026-07-30.md`.

## What Changes

- **Strategy routing:** offer the template-aware `TemplatedUnderlyingTokens` whole-grammar
  candidate to templated grammars, not only to grammars with phonological rules; templated
  grammars stop being served by `uflexc`'s self-looping affix chains as their only underlying
  candidate.
- **Search efficiency:** plan-rewrite families that provably tie after minimization stay
  *declared* (with their evidence) but are no longer *searched* by default; grammar-invariant
  work (oracle ground truth, whole-grammar emission report) is computed once per run instead of
  once per candidate; the surface-probe candidate stops paying the emission cost twice; a
  budget-exhausted run banks completed candidate results instead of losing everything.
- **Objective:** propose-side work becomes visible to the ranking key via the already-computed,
  deterministic `raw_paths` counter (today discarded before scoring). Winner selection on the
  four corpora is the acceptance oracle.
- **Cross-compiler honesty:** an equivalence/regression gate compares the independent
  Grammar→network pipelines on fixed fixtures so emitter mis-routing of this class is caught by
  a failing test, not by a corpus investigation.
- **Aweti certification breadth:** the pilot's 6-word certification is extended toward the full
  corpus with calibrated oracle caps (sweep for oracle-pathological words first).
- **Cleanup:** delete the unbuildable `ComposeStrategy::Lazy`/`LazyLookahead` variants; make
  branch-and-bound's inert bound explicit (or wire a real one); replace stringly family-id
  comparisons with shared constants; route zeroed-`Score` literals through the existing
  `build_failed` constructor; mark superseded plan docs
  (`large-lexicon-proposal-explosion.md`, `four-grammar-recipe-evidence-2026-07-28.md`).
- Later rounds (research-gated, same change): junction/deletion facts compiled as composed
  filter rules in the token-cascade path; opening the plan→emitter seam (strategy-parameter
  object; staged split of `emit_with_budget_profiled`); E5 order-faithful continuation classes
  re-censused against the fixed routing.

**Not breaking:** all CLI flags and report fields are additive (`raw_paths` joins `Score` with
`#[serde(default)]`); winner *values* may legitimately change where the old winner was an
artifact of the blind spot.

## Capabilities

### New Capabilities

- `recipe-strategy-routing`: which whole-grammar emission strategies are offered to which grammar
  shapes, and the guarantee that a templated grammar is never left with only a
  non-template-aware underlying candidate.
- `recipe-search-efficiency`: per-run cost discipline of the optimizer — declared-not-searched
  tie families, hoisted grammar-invariant work, single emission per candidate, partial-result
  banking on budget exhaustion.
- `recipe-objective`: the deterministic work-ranked objective, now including propose-side cost;
  four-corpus winner correctness is the acceptance criterion.
- `cross-compiler-equivalence`: the standing gate that the independent Grammar→network pipelines
  agree (or their divergence is a stated, tested property) on fixed fixtures.
- `recipe-pipeline-hygiene`: no unconstructible strategy variants, no structurally-dead report
  fields presented as live signals, no stringly-typed family identities at decision sites.

### Modified Capabilities

<!-- none: openspec/specs contains no prior specs for these behaviors; the
     implement-language-backed-recipe-optimizer change owns the optimizer's existence,
     this change owns its correctness/efficiency properties above. -->

## Impact

- **Code:** `rust/crates/pg-foma/src/{recipe_registry,recipe_runtime,recipe_optimizer,
  recipe_report,enumerate,build,plan,plan_diagram,plan_interaction_coverage,oracle,analyzer,
  uflexc}.rs`, `rust/crates/pg-cli/src/recipe_optimize.rs`; later rounds touch `emit.rs` and
  `templated_compile.rs` (single-owner hotspot — serialized, never two agents at once).
- **Dependencies on sibling changes:** extends `implement-language-backed-recipe-optimizer`
  (landed substrate); does not touch `replace.rs`/`gate.rs` semantic ownership in round 1;
  respects STAGING.md's emit.rs single-owner rule for later rounds.
- **Tests:** existing recipe gate/census tests keep passing (they are threshold-based); new gates
  for routing, tie-skipping, objective, and cross-compiler equivalence; four-corpus measurement
  is evidence, run out-of-band via the managed `pg.ps1` entry points (corpus data is gitignored;
  no language names enter code or fixtures — synthetic-only rule).
- **Observation vs certification claims:** four-corpus winner flips are *observations* recorded
  in the evidence doc; *certification* claims remain scoped to what `measure_and_certify`
  actually certifies (Aweti's is slice-scoped until the full-corpus run lands).
