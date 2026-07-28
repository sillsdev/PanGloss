---
name: fix-a-grammar
description: >-
  Diagnose and improve a PanGloss grammar or its FST construction when compilation is refused,
  slow, oversized, imprecise, or incomplete. Use for a new grammar onboarding, a real-language
  recall/performance problem, a request to reorganize an FST plan/tree, or whenever an engineer
  needs to compare several grammar-specific FST construction strategies instead of making a
  one-off optimization. Also use it to localize missing analyses within a staged compiler before
  changing the FST tree. This skill preserves 100% proposer recall and drives a measured,
  conformance-gated choice among at least three genuinely different candidate models.
---

# Fix a grammar

PanGloss uses a deliberately asymmetric pipeline: the FST proposes and HermitCrab confirms.
The FST may over-propose, but it may never omit an analysis the oracle can make. This skill is a
way to improve one grammar without turning that invariant into a hopeful benchmark claim.

The executable plan language is intentionally small: `Leaf`, `Compose`, `Union`, `Gate`, and
`Replace`. Do not add a plan-node enum merely to give a technique a name. A *recipe* is a named,
parameterized composition of those primitives; it is a reusable strategy, not a sixth primitive.

## When to start

Use this workflow when one of these is true:

- capability evaluation refuses a construct (a hard stop);
- build time, artifact size, candidate rejection share, or latency exceeds a stated grammar
  budget;
- recall/parity is missing for a grammar feature;
- a new grammar needs an evidence-backed FST plan rather than the default construction.

For a slow propose+confirm grammar, run `dead-end-census` first. Its attribution is input to this
workflow, not a replacement for it. Read
`docs/fst-plan/grammar-optimization-techniques.md` and this skill's `NOTES-research.md` before
choosing candidates. For recipe search, also read `docs/fst-plan/fst-recipe-space-search.md` and
`recipes/README.md`. When baseline recall is incomplete, read
`references/stage-localized-recall-diagnostics.md` before proposing or scoring recipe changes.

## Guardrails

- Begin with an oracle baseline. Record corpus recall, candidate counts, build time, artifact
  bytes, state/arc counts, and per-word latency with the commands that produced them.
- A proposer change must retain 100% recall against the oracle. Precision work must additionally
  prove `new_candidates subset_of baseline_candidates` for every checked word.
- Put any candidate worth comparing into a real `crate::plan::Plan` before serious scoring. That
  is what permits Plan-DAG interaction coverage and preserves content-addressed caching.
- Keep a recipe build-time and grammar-specific. Never recreate the removed runtime precision
  knob as a user-facing optimization dial.
- Do not combine candidate evidence from unrelated machines or concurrent compilation runs.
  Capture the environment, corpus, repetitions, and wall-clock timing with each result.

## Workflow

### 1. Characterize the grammar and name its budget

Run capability/feature characterization, baseline recall parity, `pangloss make-report`, and the
relevant conformance gates. Classify every observed pressure as capability, compile time, bytes,
state/arcs, proposer precision, or query latency. State product constraints explicitly (for
example: full recall, <= 2x baseline compile time, no more than 4x states, and p95 proposal
latency <= 1.5x baseline), rather than optimizing a weighted score in the dark.

Build an interaction graph with nodes for lexicon/root construction, templates and slots,
morphotactic strata, rule cascades, boundary cleanup, MPR/syntactic gates, and any observed
feature dependency. Add an edge when two constructs share an alphabet/boundary, must preserve
order, share a gate key, or have empirically non-additive cost. Record feature facts in a registry
candidate using `recipes/schema.json`.

If the baseline misses oracle analyses, first build one bounded diagnostic pipeline that checks
the same complete analysis–surface relation at the source/oracle, lexicon, post-rule, and final
cleanup boundaries. Run all misses through one compilation and report the first failing stage for
every required analysis.
This separates a recipe/tree problem from a rule, cleanup, encoding, or confirmation problem and
prevents tree search from optimizing around an unexplained false negative. Follow
`references/stage-localized-recall-diagnostics.md`.

### 2. Form at least three different candidates

Create three *structurally distinct* models, including one that reallocates grammar work into a
different tree shape when that is plausible. Variations that only change a timeout or measurement
count do not count. Useful contrasts include:

- static composition order versus a gated/partitioned sub-tree;
- independent branches joined by `Union` versus a common `Compose` prefix;
- ordered rewrite work represented through `Replace` versus a permissive fallback that leaves
  more to confirmation;
- a boundary-marked context restriction versus flags for genuinely long-distance features.

Use the technique catalogue to reject known dead ends for the actual problem shape. Each candidate
must spell out its `Leaf` provenance, `Compose` ordering/strategy, `Union` boundaries,
`Gate` partition keys, and `Replace` cascades. Store the reusable recipe definition and its
grammar-specific binding separately; do not bake Aweti names into a generic recipe.

### 3. Choose how to search combinations

The repository supports documented/manual recipes now and can later admit registry recipes to an
automatic planner. Keep that policy choice visible in `planner_eligibility`; `documented` does
not mean automatically enumerated. Select one search method from the recipe-space design:

1. bounded constraint-guided enumeration for small, explainable spaces;
2. Cascades-style memo/dynamic programming for a decomposable interaction graph;
3. empirical portfolio/sequential allocation for uncertain or expensive candidates.

All three methods return either an optimum within explicitly declared finite bounds or the three
best non-dominated feasible candidates. Do not claim a global optimum outside the explored
representation, constraints, and timing budget.

### 4. Score feasibility before performance

Reject a candidate immediately for capability refusal, recall loss, an invalid plan invariant, or
a tripped hard budget. For surviving candidates collect the structural vector (bytes, states,
arcs, node reuse, estimated and observed compose work), wall-clock build time, candidate
precision, and p50/p90/p95 latency. Use Pareto dominance: candidate A beats B only when A is no
worse in every required dimension and better in at least one. If the frontier contains several
choices, report the three strongest with the tradeoff rather than inventing a weighted winner.

### 5. Implement one candidate shadow-first

Make each adopted candidate a real plan and leave the previous construction available long enough
to run direct comparisons. Add the smallest conformance fixture that discriminates the new
interaction, especially when existing tests could pass without touching the affected table,
gate, or shared construct. Then run:

1. capability and full-corpus oracle recall parity;
2. candidate-set containment for a precision-tightening change;
3. both-engine conformance and Plan interaction coverage;
4. relevant crate/workspace tests and `cargo check --target wasm32-unknown-unknown`;
5. clean, repeated measurement on the standard corpus and pinned worst words;
6. cross-grammar regressions for every grammar sharing the altered construction path.

If realized benefit is materially below the pre-registered projection, preserve the evidence,
decline the default flip, and re-characterize rather than tuning until a number looks good.

### 6. Record and review the result

Update the recipe binding with environment, command lines, baselines, all rejected candidates,
frontier position, selected recipe, and cross-grammar gates. A generic recipe graduates from
`documented` to `validated` only after two independently structured grammars support it. It
graduates to `automated` only after the planner has an implemented builder, stable feature
extractor, and the registry proof/evidence requirements described in `recipes/README.md`.

## Report format

Use this format in the issue, plan, or handoff:

```text
Grammar and trigger:
Baseline and declared bounds:
Interaction graph:
Candidates (at least three):
Search method and explored bounds:
Feasibility failures:
Pareto frontier / selected optimum:
Implementation delta:
Correctness and cross-grammar gates:
Measurements (including wall-clock):
Registry evidence and next automation status:
```

## Proven four-construct recipes

Aweti established reusable candidates: non-tracking MPR overwrite plus exact confirmation; edge-anchor-erased metathesis proposal plus exact confirmation; structural oracle replay for process/zero-derivation rules; and widened derived-POS subrule gates plus confirmation. Probe once without capability enforcement to separate classifier refusal from compiler recall. Rank these with at least two structurally different alternatives using hard recall/conformance constraints, then Pareto-rank build cost, states/arcs, bytes, candidates, and latency. Call a winner optimal only within declared combinations and bounds.

## Evaluation assets

`evals/evals.json` contains realistic prompts for this skill. Treat them as a regression suite
when editing the workflow: run with-skill and no-skill baselines, grade whether the output names
three structural candidates, declares bounds, preserves recall gates, and states an honest
optimality claim. The recipe registry validator can be run with:

```text
python .claude/skills/fix-a-grammar/recipes/validate_registry.py \
  .claude/skills/fix-a-grammar/recipes/registry.example.json
```
