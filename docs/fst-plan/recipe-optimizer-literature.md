# Recipe optimizer: literature and algorithm-selection bounds

## Bottom line

The primary literature does **not** supply universal numeric cutoffs such as
“enumerate below 50,000 candidates; use beam search above 1,000,000.” Search
difficulty depends on the structure of the constraints, the strength of bounds
or propagation, duplicate subproblems, evaluation cost, and (for
multi-fidelity methods) how reliably cheap partial evaluations predict final
quality. Even classical branch-and-bound has exponential worst cases, while
some reformulated instance distributions solve at the root
([Pataki and Tural, 2009](https://optimization-online.org/2009/07/2345/)).

Therefore PanGloss should choose its algorithm from *measured work*, not raw
cardinality:

```text
N_raw      = product of the option counts at all HC decision points
N_static   = number left after local compatibility/domain filtering
N_feasible = number satisfying all cross-decision HC constraints
t_cheap    = median time for structural checks and cheap scoring
t_full     = median time to build, run the oracle/conformance corpus, and measure
B          = allowed optimizer wall-clock budget × safe parallelism

exhaustive_full_work ≈ N_feasible × t_full
```

Exact enumeration is the default whenever that estimated work fits comfortably
inside `B` (with a safety factor for long-tail build times). A proposed
implementation cutoff should consequently be calibrated, e.g.
`N_feasible × p95(t_full) <= 0.5 × B`, rather than embedded as an
instance-independent candidate count.

## What “realizable” should mean

Use three counts rather than one:

1. **Raw combinations**: the Cartesian product of every HC-exposed recipe
   choice. This is useful for explaining the grammar's hypothetical space but
   usually overstates work.
2. **Statically realizable combinations**: choices remaining after constraints
   that require no FST build—capability applicability, mutual exclusion,
   prerequisites, type/stratum compatibility, parameter-domain reduction, and
   symmetry/dominance rules.
3. **Dynamically feasible combinations**: statically realizable candidates that
   compile and satisfy the hard oracle/recall constraints.

This separation follows the purpose of constraint programming: add constraints
to narrow a very large possibility set to feasible solutions
([OR-Tools constraint optimization documentation](https://developers.google.com/optimization/cp)).
The same official documentation illustrates why raw size is misleading: a
small scheduling formulation has more than 4.5 billion raw schedules, while
constraints can sharply reduce the feasible subset.

For PanGloss, compute `N_static` exactly by constrained enumeration when cheap,
or by a SAT/CP model counter if the option graph becomes large. Do not estimate
it by multiplying independently reduced domains when constraints couple
decisions; that product remains only an upper bound.

## Algorithm families

### 1. Exhaustive constrained enumeration

Cost is approximately `O(N_feasible × C_full)` after static pruning; memory can
be `O(K)` if only the best/Pareto candidates and audit rows are retained.
Enumeration is the only simple option here that guarantees observing every
feasible recipe without requiring a valid decomposable cost model or
admissible bound.

Use it when:

- exact `N_static` is available;
- `N_static × p95(t_full)` fits the budget;
- full conformance is a hard constraint;
- or the four-language corpus is still small enough that exhaustive results
  are valuable as ground truth for validating later heuristics.

The operational threshold is not “50k”; it is
`floor(0.5B / p95(t_full))` after parallelism and resource limits are included.
For example, 50,000 candidates at 2 ms each is easy; 50,000 at 30 seconds each
is not.

### 2. Branch-and-bound, CP, or SAT-guided enumeration

Use this when raw HC combinations are large but static constraints and lower
bounds are strong. CP is explicitly designed to maintain feasibility as
constraints narrow a large possibility set, and CP-SAT can either find an
optimum or enumerate all solutions
([OR-Tools CP-SAT documentation](https://developers.google.com/optimization/cp/cp_solver)).

Branch-and-bound is exact only if:

- every prune uses a sound infeasibility proof or an admissible objective lower
  bound;
- the incumbent is a fully feasible recipe;
- and search reaches a proof of optimality.

A good incumbent tightens pruning: Volcano observes that after a complete plan
is known, a more expensive partial plan cannot be optimal, and passes cost
limits into subexpressions
([Graefe and McKenna, 1993, pp. 212–213](https://15721.courses.cs.cmu.edu/spring2017/papers/14-optimizer1/graefe-icde1993.pdf)).
But there is no candidate-count guarantee: classical branch-and-bound has
exponential worst-case complexity
([Pataki and Tural, 2009](https://optimization-online.org/2009/07/2345/)).

Decision rule: prefer CP/SAT or branch-and-bound when
`N_raw` is too large to materialize, but propagation or a pilot run reduces
visited nodes by an empirically large factor. Record `nodes visited`,
`candidates pruned by reason`, and the remaining optimality gap/status; a time
limit can return a feasible result without proving it optimal (CP-SAT
distinguishes `FEASIBLE` from `OPTIMAL` in its official API documentation).

### 3. Selinger/Volcano/Cascades-style dynamic programming

Dynamic programming is appropriate when recipe construction has reusable
equivalence classes and optimal substructure. System R retains the cheapest
plan for each joined subset and each “interesting” physical order, and reduces
the state count with order-equivalence classes
([Selinger et al., 1979, pp. 28–29](https://people.eecs.berkeley.edu/~brewer/cs262/3-selinger79.pdf)).
Volcano generalizes this by retaining partial optimization results and the best
plan for each equivalence class/physical-property combination
([Graefe and McKenna, 1993, pp. 212–213](https://15721.courses.cs.cmu.edu/spring2017/papers/14-optimizer1/graefe-icde1993.pdf)).
Cascades adds demand-driven exploration and returns an earlier plan when the
same group and optimization goal recur
([Graefe, 1995, pp. 20–21](https://15721.courses.cs.cmu.edu/spring2019/papers/22-optimizer1/graefe-ieee1995.pdf)).

For recipes, the meaningful complexity measure is:

```text
sum over memo groups g of
    (# realizable physical-property states for g)
  × (# applicable implementations/transitions for g)
```

not the raw product of all recipe choices. DP is the best exact method when
many complete recipes share the same HC subtree/state and downstream cost
depends only on a compact, explicit property vector. It is unsafe to discard a
locally inferior partial recipe if later interactions depend on unrecorded
properties (for example, a confirmation stage or another stratum changes its
relative value). Those properties must be added to the memo key, potentially
erasing the compression benefit.

Decision rule: build a memo-state estimate from HC. Choose directed DP when
that estimate is much smaller than `N_static` and the objective/constraints are
compositional. Combine it with sound branch-and-bound pruning, as Volcano does,
rather than treating the families as mutually exclusive.

### 4. Beam search

Beam search limits retained partial candidates to width `w`. For branching
factor `b` and depth `d`, its rough expansion work is `O(d × w × b)` and its
frontier memory is `O(w)` (implementation details can add sorting/queue costs).
It is a satisficing heuristic: increasing width trades computation for solution
cost, but does not by itself prove global optimality
([Lemons et al., 2022](https://ojs.aaai.org/index.php/ICAPS/article/view/19805)).

Use it only after exact methods exceed the budget and a pilot demonstrates that
the partial-score heuristic predicts final feasible quality. Always retain an
exact mode for the four-grammar benchmark. Report beam width, expanded/pruned
counts, best result by width, and whether the chosen recipe matches exhaustive
ground truth on benchmark grammars. There is no literature-backed universal
beam width.

### 5. Successive Halving and Hyperband

These methods address a different bottleneck: many candidate evaluations are
expensive but can be run at meaningful lower fidelity. Successive Halving
allocates a small resource to many candidates, discards poor candidates, and
increases resource for survivors. Hyperband searches the tradeoff between
number of configurations and resource per configuration on a geometric grid;
its overhead relative to knowing the best allocation in advance is logarithmic
in the budget
([Li et al., 2018](https://arxiv.org/abs/1603.06560)).

They are suitable only if PanGloss defines a fidelity axis—such as an
increasing, representative word/corpus sample—whose early scores correlate
with final conformance and whose hard failures are monotone or rechecked at
full fidelity. The Hyperband guarantees depend on assumptions about loss
sequences/convergence and the distribution of sampled arms; they are not
numeric search-space thresholds. In particular, full HC/oracle recall cannot
be inferred safely from a sample unless the omitted cases are eventually
checked.

Decision rule: use Hyperband/Successive Halving when `t_full` dominates,
`N_static` is too large for full evaluation, and pilot rank-correlation plus
false-negative measurements show a useful fidelity signal. Promote all final
survivors through complete conformance before calling them realizable or
optimal. If partial corpora do not rank candidates reliably, use constraint
pruning, DP, or beam search instead.

## Recommended PanGloss selector

The selector should be empirical and staged:

1. Derive `N_raw` from HC choice cardinalities.
2. Apply sound static constraints and symmetry/dominance reductions; exactly
   count `N_static` if possible.
3. Benchmark a stratified pilot to estimate `p50/p95(t_full)`, pruning ratio,
   memo-group compression, and (if proposed) low/full-fidelity rank
   correlation.
4. Select:
   - **enumeration** if estimated full work fits;
   - **CP/SAT + branch-and-bound** if constraints/bounds sharply prune;
   - **directed DP + branch-and-bound** if HC exposes repeated equivalent
     subproblems with complete property keys;
   - **Hyperband** if evaluation is expensive and fidelity is predictive;
   - **beam search** as the explicitly approximate fallback when none of the
     exact reductions make the work fit.
5. For the current four grammars, run exhaustive constrained enumeration first.
   It supplies the real reduction ratios and optimum needed to validate every
   more scalable algorithm.

The likely long-term design is therefore a hybrid, not a single magnitude
table: HC supplies a constraint graph and memoization keys; static propagation
removes unrealizable recipes; directed DP shares equivalent subproblems;
branch-and-bound removes dominated states; exhaustive evaluation is retained
when the resulting feasible frontier fits; and multi-fidelity or beam search is
activated only under measured, reported conditions.

