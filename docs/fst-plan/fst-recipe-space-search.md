# Scriptable FST recipe-space search

## Decision in one sentence

Keep `Leaf`, `Compose`, `Union`, `Gate`, and `Replace` as the closed executable language, and
search named recipes that build DAGs from those five primitives. A search result is either the
best feasible candidate within declared finite bounds or the three strongest Pareto candidates;
it is never an unqualified global optimum.

The current implementation policy remains **documented/manual recipes first**. The registry in
`.claude/skills/fix-a-grammar/recipes/` is mode-neutral: it makes later automatic enumeration
possible without claiming the planner, stable metric bands, or lazy-composition builder already
exist. A new primitive still needs an ADR amendment; a new named recipe does not.

## Shared contract for all three approaches

### Input representation

Start from a grammar interaction graph, not from blind permutations. Its vertices are grammar
fragments (`Leaf` provenance): lexicon/root build, template slots, strata, ordered rules,
boundary cleanup, MPR/syntactic conditions, compounds, and fallback paths. Its edges capture:

- order dependencies (a cascade or boundary step must precede/follow another fragment);
- shared alphabet/boundary dependencies;
- a gate key affecting a rewrite subrule or lexicon partition;
- alternatives that can be independently compiled and later `Union`ed;
- measured non-additive cost or correctness interactions.

A recipe maps a feature predicate over this graph into a parameterized plan-template DAG. Its
parameters include a `Compose` order/strategy, branch boundaries, `Gate` partition key, which
rules enter a group-specific `Replace`, and permitted `Union` alternatives. Identical materialized
nodes retain the normal content-addressed identity and cache sharing.

### Candidate pipeline

For every materialized candidate:

1. validate Plan invariants, including one `Gate` child per partition group;
2. reject capability refusals and run full-oracle recall parity;
3. for a precision-only change, prove candidate-set containment against baseline;
4. collect structural metrics: payload bytes, states, arcs, Plan node reuse, and compose work;
5. build repeatedly on a quiet machine and retain wall-clock build milliseconds as a first-class
   metric; measure p50/p90/p95 proposal latency and candidate/rejection counts;
6. run both-engine conformance, Plan interaction coverage, focused discriminating fixtures, the
   affected crate/workspace suite, wasm check, and cross-grammar parity for shared code paths.

Hard constraints are feasibility filters, not weighted-score terms: 100% recall, no capability
refusal, valid Plan shape, stated byte/state/build/latency caps, and required gates. Rank feasible
candidates with Pareto dominance. If a product owner needs a single default from a frontier, use a
pre-declared lexicographic policy (for example: recall, then p95 latency, then wall-clock build,
then bytes) and label the result a policy-selected optimum.

### Returned evidence

Every method writes the registry binding, graph snapshot/hash, registry version, parameter bounds,
commands/environment, raw repetitions, rejected candidates and why, Pareto frontier, selected
candidate, and cross-grammar gate results. This is the feedback loop: trials update recipe
applicability and may promote a recipe from documented to validated; they do not silently create a
runtime tuning surface.

## 1. Bounded constraint-guided enumeration

### Method

This is the most direct answer for a new grammar with a small, explainable space. Extract graph
constraints, then enumerate only legal recipe bindings:

```text
for applicable recipe in registry:
  for assignment in finite(recipe.parameters):
    plan = materialize(recipe, grammar, assignment)
    if violates(graph ordering, gate cardinality, or hard structural estimate): continue
    evaluate(plan)
```

Constraints prune impossible ordering permutations, singleton gate partitions, mutually exclusive
features, unsupported `ComposeStrategy`s, and candidates estimated to exceed a hard cap. A
canonical serialization/hash prevents duplicate DAGs from different parameter descriptions.

### Optimality and stopping

The method has a strong, easy-to-explain guarantee: after every binding in the declared recipe
snapshot and finite parameter ranges has been evaluated (or soundly pruned), return the
lexicographic feasible winner or three strongest non-dominated candidates. The optimum is over
that exact bounded space. Stop earlier only on a wall-clock/candidate budget and label the output
`incomplete`; return the measured frontier rather than calling it optimal.

### Cost and fit

Worst-case cost is exponential in independent choices: `O(R * product(parameter cardinalities))`
materializations, plus full builds for candidates that survive estimates. It is practical when a
grammar has perhaps 3–8 composition/gate/branch decisions after graph pruning, and is ideal for
the first few recipe trials because an engineer can inspect every rejected possibility.

### Aweti example

Suppose the extracted Aweti graph contains a templated lexicon, an 18-rule ordered cascade,
boundary cleanup, reduplication/circumfix fragments, and six still-missed morphology/rule cases.
Search three recipe families within these bounds:

| Family | Recipe tree | Bounded parameters |
| --- | --- | --- |
| Baseline cascade | `Compose(Leaf(lexicon), Replace(all-rules), Leaf(cleanup))` | four legal cascade group boundaries |
| Partitioned cascade | `Gate(Compose(Leaf(group-lexicon), Replace(group-rules), Leaf(cleanup)))` | no gate / only proven group keys / <= 4 groups |
| Branched morphology | `Union(Compose(Leaf(common), Replace(cascade)), Compose(Leaf(redup-or-circumfix), Replace(cascade)))` | one of two source allocations; static strategy only |

The script first rejects any candidate that fails Aweti corpus recall or the C2/reduplication
fixture. It measures every remaining canonical tree, then returns the bounded winner/frontier;
it does not infer that one tree is globally best for all possible future allocations.

## 2. Cascades-style memoized dynamic programming

### Method

This is a planner-style alternative when the interaction graph can be decomposed into clusters.
Use an AND/OR memo (Cascades-style): a logical group represents a set of grammar fragments and
required boundary/gate interface; a physical expression represents a concrete `Compose`, `Union`,
`Gate`, or `Replace` arrangement. Store a Pareto set per `(fragment-set, interface-state)` rather
than a single cheapest subplan.

```text
memo[S, interface] = nondominated physical plans for S
for partition (A, B) of a legal group S:
  combine memo[A, left-interface] with memo[B, right-interface]
  add legal Compose/Union/Gate/Replace physical expressions
  retain only feasible nondominated expressions per interface
```

The interface includes tape/boundary compatibility, required ordering, gate-key scope, and whether
a `Replace` cascade is group-specific. A `Leaf` begins a group; `Compose` combines ordered groups;
`Union` combines alternatives; `Gate` expands only feature-derived partitions; `Replace` remains a
named cascade so its ordering and gate context are not accidentally treated as an arbitrary join.

### Optimality and stopping

With a complete transition rule set, finite interface states, and exact objective values, dynamic
programming finds the Pareto-optimal plan set for the admitted grammar graph and recipe rules.
Because actual wall-clock is learned only by building, use a two level guarantee: optimize exact
structural constraints in the memo, then benchmark every surviving root frontier candidate. If
measurement changes dominance, update that root frontier. Stop when no memo group has unexpanded
legal transitions and all root-frontier candidates have the prescribed repetitions. Otherwise
report a partial frontier and the unfinished memo states.

### Cost and fit

Unconstrained join-style DP is exponential in graph width (roughly `O(3^n)` partitions). It scales
better than enumeration when many recipe combinations share subtrees because each `(S, interface)`
is built once, but only when the interaction graph has small separators and Pareto sets are bounded.
Use an explicit `K` cap per memo frontier only as an approximate mode, record discarded candidates,
and never label its result exact.

### Aweti example

Partition Aweti into clusters: `{templated lexicon}`, `{morphotactics/reduplication/circumfix}`,
`{ordered phonology rules}`, and `{boundary cleanup}`. The DP knows the final cleanup follows
phonology and a rule group cannot cross a gate interface without group-specific `Replace`. It
memos the best physical ways to produce each cluster: a shared lexicon `Leaf`, a gated morphology
subtree, and several legal cascade subplans. Common cascade prefixes and group-specific rewrites
are built once per interface state. The final root frontier might show:

1. smaller static cascade with best bytes;
2. gated cascade with best p95 latency;
3. branch-union allocation with best build wall-clock.

All three still require corpus recall, the Aweti fixture gates, and cross-grammar paths before one
can become a default. This is the natural long-term method once the initial lazy-composition
builder and metric banding are real, because those add physical alternatives DP can compare.

## 3. Empirical portfolio with sequential allocation

### Method

Use this when full builds are expensive, estimates are weak, or recipe interactions make a
complete search too large. Treat each recipe binding as a portfolio member. Start with safe
screening: capability, Plan validation, small corpus/oracle slice only when it is known to be
diagnostic, and structural limits. Allocate repeated full measurements adaptively to candidates
that can still plausibly reach the feasible Pareto frontier.

For each candidate retain a distribution (median and confidence interval, or a conservative
non-parametric interval) for wall-clock build and latency, not one noisy timing. Successive
halving is a simple schedule: evaluate all candidates once, discard candidates confidently
dominated or constraint-breaking, double repetitions for survivors, and reserve the largest
corpus/cross-grammar runs for finalists. Use paired/interleaved baseline runs when machine drift is
material.

### Optimality, confidence, and stopping

This method cannot guarantee a global optimum unless it exhausts the finite candidate space with
the required repetitions. Its honest result is one of:

- **bounded empirical winner**: all candidates in the declared portfolio were measured and the
  chosen candidate is feasible and statistically/operationally preferred under the declared
  lexicographic policy;
- **confident Pareto shortlist**: return three candidates whose confidence intervals do not make
  them confidently dominated;
- **budget-exhausted frontier**: return all currently non-dominated candidates with uncertainty.

Stop when a fixed measurement budget is spent, all remaining candidates have confidence intervals
narrower than a stated practical difference, or no remaining candidate can enter the frontier.
Never discard a candidate for a noisy single timing, and never early-stop the oracle/conformance
gates: those are categorical requirements, not bandit rewards.

### Cost and fit

Initial cost is linear in the number of portfolio members; the expensive tail concentrates on a
few survivors. It is the right trade when one full Aweti-scale build costs minutes or when future
lazy strategies have unpredictable runtime behavior. It does not exploit shared subtree work as
well as DP, so cache/reuse is still valuable, but its main advantage is spending the timing budget
where it changes the decision.

### Aweti example

Begin with the three families from bounded enumeration and add only their legal parameterizations
to the portfolio. Run Plan validation and the full 106-word recall gate before timing—any of the
six known misses remains a correctness failure, not a performance datapoint. Screen survivors with
one clean build plus structural caps, then alternate baseline/candidate builds over the Aweti
corpus and pinned hard words. If the gated cascade is clearly slower and no smaller while the
branch-union plan has a wide but promising build-time interval, allocate more runs to branch-union
rather than repeatedly confirming the loser. Finalists receive conformance, interaction coverage,
and the reference-grammar cross-check. Report the three uncertainty-aware frontier members unless
the declared policy and intervals support one bounded empirical winner.

## Recommendation and staged rollout

Adopt all three methods, but in this order:

1. **Now:** bounded constraint-guided enumeration. It is scriptable with the current plan model,
   makes the recipe hypothesis inspectable, and supplies the evidence to populate the registry.
2. **When a grammar exposes a large decomposable space:** memoized DP, initially for static
   strategies and known interfaces. Add lazy strategies only after their builder is implemented;
   a type-level `ComposeStrategy` variant is not evidence that it can be executed.
3. **Whenever full builds are costly or noisy:** sequential allocation around either candidate
   generator, with a separate correctness gate before any adaptive timing choice.

The methods are complementary rather than competing: enumeration or DP generates the legal
portfolio; sequential allocation decides how to spend expensive empirical measurements. This is
how a new grammar can eventually receive a defensible statement that one combination is optimal
within a declared configuration space—or, when objectives genuinely disagree, that these three
are the strongest feasible Pareto choices.
