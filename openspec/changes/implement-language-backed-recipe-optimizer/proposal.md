## Why

PanGloss can represent and compare compilation plans, but it cannot yet derive a useful
recipe space from a new grammar or search that space at a scale appropriate to the
grammar. The result is a fixed, very small candidate set even when grammar structure and
attested linguistic constructions imply materially different, realizable FST recipes.

## What Changes

- Add an extensible registry of realizable recipe families, seeded from the construct
  patterns documented in `docs/fst-plan/linguistic-recipe-harvest.md`.
- Characterize each grammar's raw, attested-family, statically admissible, and observed
  feasible recipe spaces from HC grammar facts and compiler capability constraints.
- Add an approximate-first optimizer that chooses and records a search strategy from
  measured space size, pruning effectiveness, evaluation cost, and caller budget.
- Require every selected finalist to be buildable and to agree with the full-HC oracle at
  analysis identity and multiplicity level; partial evaluations guide search but never
  certify correctness.
- Rank confirmed candidates on a deterministic Pareto policy over FST size, build cost,
  apply cost, and proposal/confirmation work while always retaining the current default
  recipe as a baseline.
- Emit a replayable optimization report, candidate table, unexplored-space statement,
  selected-plan JSON, and Mermaid plan diagram.
- Exercise at least two distinct realizable recipes on each of four promoted synthetic
  conformance grammars where their hard constraints permit it, and publish a detailed
  comparison for the most informative grammar.

## Capabilities

### New Capabilities

- `recipe-family-registry`: Versioned, extensible definitions of realizable FST recipe
  families, applicability predicates, parameters, and materializers.
- `recipe-space-characterization`: Deterministic grammar-derived search-space counts,
  constraint pruning, and pilot cost measurements.
- `approximate-recipe-optimization`: Budgeted adaptive search, full-HC finalist
  confirmation, deterministic ranking, and explicit approximation status.
- `recipe-optimization-reporting`: Replayable machine and human reports, candidate
  comparisons, search-accounting evidence, and plan diagrams.

### Modified Capabilities

None. This change composes existing plan, capability, build, oracle, selection, profiling,
and visualization behavior without changing their contracts.

## Impact

- Primary code: `rust/crates/pg-foma/src/enumerate.rs`, `selection.rs`, `plan_diagram.rs`,
  `lib.rs`, and new recipe-registry, space, optimizer, and report modules.
- CLI integration: `rust/crates/pg-cli` for an explicit offline/build-time optimization
  command; normal parsing remains deterministic and performs no online tuning.
- Evidence: synthetic conformance fixtures only. Research about actual languages informs
  recipe-family design and ranking priors, but does not constitute support, recall, or
  certification evidence.
- Dependencies: completed `reify-compilation-plans` and `visualize-compilation-plan`
  changes; existing capability checks, resource-safety budgets, differential oracle,
  compilation profiling, coverage ledger, and interaction gates described in
  `openspec/changes/STAGING.md`.
- Non-goals: changing HC semantics, weakening any recall/completeness/resource gate,
  introducing new Plan primitives, using language identity as a correctness predicate,
  or editing the semantic owners `replace.rs`, `gate.rs`, and `emit.rs`.
