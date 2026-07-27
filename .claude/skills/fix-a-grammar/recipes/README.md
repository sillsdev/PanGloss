# FST recipe registry

This directory defines the portable part of a FST construction strategy. It deliberately does
not select a grammar's winner at runtime. The current repository policy is **manual/documented
recipe selection with measured evidence**; the schema preserves the information required for a
future planner without pretending that the planner exists today.

## What a recipe is

A recipe is a named, parameterized tree pattern made entirely of the five existing `PlanNodeKind`
primitives:

| Primitive | Recipe meaning |
| --- | --- |
| `Leaf` | compile one source fragment with recorded grammar provenance |
| `Compose` | sequence child transducers; ordering and physical strategy are parameters |
| `Union` | merge independently valid alternatives |
| `Gate` | partition a grammar-dependent subproblem and union its group children |
| `Replace` | construct an ordered rewrite cascade with its own gate context |

Recipes must not introduce new node kinds. The concrete plan interners retain Merkle-style node
identity and cache sharing; a recipe merely gives a useful subtree shape a stable name.

## Files

- `schema.json` — JSON Schema for a registry document.
- `registry.example.json` — a valid, mode-neutral example with non-production recipes.
- `validate_registry.py` — dependency-free structural validator used in CI or a local handoff.

Run:

```text
python .claude/skills/fix-a-grammar/recipes/validate_registry.py \
  .claude/skills/fix-a-grammar/recipes/registry.example.json
```

The validator intentionally checks semantic constraints that generic JSON Schema cannot express:
the recipe root must exist, every input must name a node, only the five primitives may appear, and
an `automated` recipe must contain planner evidence. It does not claim the generated plan is
language-equivalent; that belongs to the compiler, oracle parity, and conformance gates.

## Registry model

A `recipe` contains:

- a feature predicate over an extracted grammar interaction graph;
- a parameter space and plan-template DAG;
- hard constraints plus tracked structural and wall-clock objectives;
- an automation status and evidence record;
- feedback slots for grammar-specific trials.

A `binding` (usually recorded beside the grammar plan rather than in this generic registry) fixes
recipe parameters, source constructs, corpus, declared bounds, measurements, and gate results for
one grammar. This separation means `gated-replace-partition` can be a reusable strategy without
claiming every grammar's gate keys or rewrite rules are alike.

## Manual now; automatic later

`planner_eligibility` is deliberately three-valued:

- `documented` — engineers may instantiate and measure it manually. Default for a new recipe.
- `validated` — at least two structurally different grammars have evidence, but a human still
  decides whether it belongs in a search.
- `automated` — a planner may enumerate it only after feature extraction, builder support,
  constraints, and cross-grammar proof evidence are present.

This does not settle the product decision between manual and first-class automation. It prevents
that decision from forcing a future schema migration, while avoiding an unusable auto-enumerator
before metric bands and lazy-composition builders exist.

## Required evidence for an automated recipe

An `automated` entry must identify its feature extractor and builder, name finite parameter bounds,
record at least two independent grammar trials, and provide a correctness/cross-grammar gate
reference. It must be demoted when a later grammar exposes an unmodelled interaction.

Every trial records wall-clock build time in addition to structural metrics. Structural estimates
help prune; they cannot substitute for wall-clock because allocation, minimization, and engine
behavior do not reduce to state/arc counts.

## Search contract

The planner or a manual engineer must choose one of the three methods in
`docs/fst-plan/fst-recipe-space-search.md`. A result is only an optimum **within** the registry
snapshot, parameter bounds, budgets, corpus, and search method. Otherwise return the three best
feasible non-dominated candidates and their measurements.
