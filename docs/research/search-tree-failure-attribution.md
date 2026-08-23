# Per-object failure/blame attribution in a search tree with shared prefixes

Date: 2026-08-22

## Conclusion

For a first version, count **terminated-at** (self-time / wdeg-style immediate blame) as the
numerator, and use **applications of that object, live and dead, across all paths** as the
denominator — i.e. a per-object rate `dead-immediately-after / times-applied`, not a raw count.
This is not a compromise pick: it is the one attribution scheme this research finds *repeatedly
validated at production scale* across four independent fields (CSP solving, SAT solving, HPSG
parsing, CPU profiling), while the naive full-ancestor-chain scheme (`implicated-in`, i.e. gprof's
inclusive-time propagation) is the one this research finds *repeatedly and explicitly diagnosed as
unsound* wherever paths share ancestry. The "principled" fix for `implicated-in` — a minimal
conflict/explanation set per failure (QuickXplain/MUS, or a SAT solver's 1UIP cut) — exists and is
well understood, but every source that uses it treats it as expensive per-failure machinery, never
as a cheap first-pass instrument, and nothing found here applies it at 10^6–10^8 failures. Exact
Shapley/Aumann-Shapley attribution is the theoretically cleanest answer and is also the one this
research could not find applied to a tree/DAG at this scale at all; treat it as out of reach for a
first version. Subtree-size-weighted blame (addressing "this rule caused huge fan-out, not just one
death") has no established precedent as a *blame* technique — the only tree-shaped use of subtree
weighting found is Knuth/Kilby-style *tree-size estimation*, a different problem, though its
sampling machinery is repurposable to estimate this scheme's denominator cheaply without enumerating
every path.

## Recommendation

- **Numerator:** count one increment per object per dead leaf, charged only to the object applied
  immediately before that leaf died (`terminated-at`). Do not walk the ancestor chain for the
  primary metric.
- **Denominator:** total times that object was applied across the whole search, counting both
  paths that eventually died and paths that eventually succeeded — not "dead leaves below this
  object" (that denominator is `implicated-in`'s own scope and inherits its dilution problem) and
  not "total node visits" undifferentiated by object identity. This mirrors how the CSP and HPSG
  literature already normalizes: `dom/wdeg` divides accumulated conflict weight by domain size
  precisely so a heavily-used-but-harmless variable does not look worse than a rarely-used, truly
  toxic one ([Boussemart, Hemery, Lecoutre & Sais, "Boosting Systematic Search by Weighting
  Constraints", ECAI 2004](https://www.researchgate.net/publication/220838185_Boosting_Systematic_Search_by_Weighting_Constraints));
  Malouf, Carroll & Copestake's quick-check ranks feature paths by a per-path *failure rate*, not a
  raw failure count, precisely so an infrequently-checked-but-always-fatal path outranks a
  frequently-checked-but-rarely-fatal one ([Malouf, Carroll & Copestake, "Efficient feature
  structure operations without compilation", Natural Language Engineering 6(1), 2000](https://www.cambridge.org/core/services/aop-cambridge-core/content/view/6280E05F70439FFF4C0D7381193915B6/S1351324900002382a.pdf/efficient_feature_structure_operations_without_compilation.pdf)).
- **Second-tier, opt-in diagnostic:** once `terminated-at` rate has surfaced a suspect object, run a
  QuickXplain-style minimal-conflict-set extraction ([Junker, "QUICKXPLAIN: Preferred Explanations
  and Relaxations for Over-Constrained Problems", AAAI 2004](https://cdn.aaai.org/AAAI/2004/AAAI04-027.pdf))
  only on that object's own dead leaves, to answer "which ancestors were *actually* necessary for
  this specific death," rather than computing it for every one of 10^6–10^8 dead leaves. This is the
  same "cheap global rate first, expensive targeted trace second" shape PanGloss's own
  `docs/research/pangloss-health-vs-hc-rule-stats.md` already recommends for rule-stat/health
  integration — independent confirmation the pattern is right, not a new idea invented for this
  document.
- **Do not ship `implicated-in` as reported at all**, not even as a secondary column, without a
  minimality step. Every literature match for "blame everything on the path" is either a known
  anti-pattern (gprof) or was itself replaced, inside its own field, by a minimal-cut scheme (SAT's
  1UIP; QuickXplain).

## Candidate numerator/denominator pairs

| Scheme | Numerator / denominator | Used by (citation) | Over-credits | Under-credits | Cost at 10^6–10^8 dead leaves |
|---|---|---|---|---|---|
| `terminated-at` rate | dead leaves terminated here / total applications (live+dead) | `wdeg`/`dom-wdeg` charges the constraint whose propagation wiped a domain ([Boussemart et al. 2004](https://www.researchgate.net/publication/220838185_Boosting_Systematic_Search_by_Weighting_Constraints)); CHS refines this with a recency-weighted average of the same event ([Habet & Terrioux, "Conflict history based heuristic for constraint satisfaction problem solving", J. Heuristics 2021](https://link.springer.com/article/10.1007/s10732-021-09475-z), preprint: [heuristics21.pdf](https://pageperso.lis-lab.fr/cyril.terrioux/en/publis/heuristics21.pdf)); quick-check ranks feature paths by observed unification-failure frequency ([Malouf, Carroll & Copestake 2000](https://www.cambridge.org/core/services/aop-cambridge-core/content/view/6280E05F70439FFF4C0D7381193915B6/S1351324900002382a.pdf/efficient_feature_structure_operations_without_compilation.pdf)); "self time" in any sampling profiler / flame graph ([Gregg, "The Flame Graph", ACM Queue 14(2), 2016](https://queue.acm.org/detail.cfm?id=2927301)) | A rule with huge fan-out that always dies one hop *later* (child rule) — the true amplifier is invisible, only its child is charged | A rule whose direct effect is subtle and only bites several levels down (this is exactly `implicated-in`'s claimed advantage, and exactly what a QuickXplain follow-up is for) | Cheap: one counter increment per node, no extra passes |
| `implicated-in` raw count | every ancestor +1 per dead leaf / — (no literature normalizes this as a rate; it is reported as a raw count or percentage of all leaves) | gprof's call-graph propagated ("inclusive") time is the direct analogue: a function's reported time includes every descendant's time, attributed up through every caller on every recorded path ([GNU gprof manual, "How to Read the Call Graph"](https://www.math.utah.edu/docs/info/gprof_6.html); [GNU gprof, "Cycles"](https://sourceware.org/binutils/docs/gprof/Cycles.html)) | Any common, innocent early object sitting above a rare, toxic deep object — charged once per leaf under that deep object regardless of its own behavior | Nothing (it is a strict superset of `terminated-at`'s information, which is exactly the accusation against it) | O(path length) per dead leaf; only tractable if paths are shallow or leaves are few |
| `implicated-in`, subtree-size-normalized | dead leaves in subtree / (dead+live leaves in subtree) | No direct precedent found. Nearest relative: Knuth's random-probe estimator for total backtrack-tree size ([Knuth, "Estimating the Efficiency of Backtrack Programs", Math. Comp. 29(129), 1975](https://www.ams.org/journals/mcom/1975-29-129/S0025-5718-1975-0373371-6/S0025-5718-1975-0373371-6.pdf)) and Kilby, Slaney, Thiébaux & Walsh's weighted-sample / recursive estimators ([AAAI 2006](https://cdn.aaai.org/Workshops/2006/WS-06-11/WS06-11-005.pdf)) weight branches to estimate *total tree size*, not to *blame a node* | Still charges any ancestor sitting above a uniformly bad region of the tree — normalizing by subtree size does not distinguish an ancestor from any other equally-placed ancestor over the same subtree | Nothing new relative to raw `implicated-in` | One pass bottom-up to compute subtree dead/live counts is cheap (O(nodes)); the estimation literature's random-probing technique could make even this approximate without full enumeration |
| Minimal conflict/explanation set | 1 if this object is in the *minimal* subset of ancestors that provably caused the death, else 0 | SAT solvers learn a clause from the minimal 1UIP cut of the implication graph, not the whole graph — literals outside the cut are not blamed for that conflict ([Moskewicz, Madigan, Zhao, Zhang & Malik, "Chaff: Engineering an Efficient SAT Solver", DAC 2001](https://rg1-teaching.mpi-inf.mpg.de/advancedc-ws08/exercises/Chaff.pdf); UIP mechanism summarized in [Liang, Ganesh, Poupart & Czarnecki, "Understanding VSIDS Branching Heuristics...", 2015](https://arxiv.org/abs/1506.08905)); QuickXplain computes a minimal explaining subset of constraints for a CSP/SAT/DL failure by a divide-and-conquer search over subsets ([Junker, AAAI 2004](https://cdn.aaai.org/AAAI/2004/AAAI04-027.pdf)) | Nothing — minimality is the whole point | Nothing in principle; in practice the *approximate* minimal set found by a single QuickXplain run is not guaranteed globally minimum, only locally irreducible | Expensive: QuickXplain needs O(log n) extra consistency-check calls per invocation where n is candidate-set size, run once per dead leaf if applied everywhere — no source examined here reports running it at 10^6–10^8-leaf scale; treat as a per-object opt-in diagnostic, not a global counter |
| Exact Shapley / Aumann-Shapley value | this object's average marginal contribution to failure cost over all orderings/coalitions on the path | General cost-allocation theory ([Shapley 1953, summarized in Vlach, "The Shapley Value and Related Solution Concepts", 2011](https://onlinelibrary.wiley.com/doi/10.1002/9780470400531.eorms0768); Aumann–Shapley pricing, e.g. [Haviv & Winter, "Weighted Aumann-Shapley pricing", Int. J. Game Theory](https://link.springer.com/article/10.1007/s001820050087)); causal "degree of responsibility/blame" generalizes this to structural-model causality ([Chockler & Halpern, "Responsibility and Blame: A Structural-Model Approach", JAIR 22, 2004](https://jair.org/index.php/jair/article/view/10386)) | N/A (theoretically the most defensible scheme) | N/A | Exponential in the number of contributing objects in general ("Shapley value computation is exponential with the number of players, making its formulation intractable even for a few dozens of players" — surveyed in [MDPI, "The Shapley Value in Data Science", 2025](https://www.mdpi.com/2227-7390/13/10/1581)); Monte-Carlo approximations exist for dozens of players, but no source found here applies Shapley-style attribution to a search tree/DAG of this shape or size at all — flagged as an open gap, not a known-negative result |

## Known-unsound attributions found in the literature

- **gprof's inclusive-time propagation across shared/recursive call structure.** gprof collapses
  every strongly-connected set of mutually recursive functions into one synthetic "cycle" node
  specifically because per-member time cannot be soundly separated once recursion is present, and
  the manual documents this as a known degradation, not an edge case: "Programs that exhibit a
  large degree of recursion, such as recursive descent compilers, are not easily analyzed by gprof
  because most major routines are grouped into a single monolithic cycle, making it impossible to
  distinguish which members of the cycle are responsible for the execution time" ([GNU gprof,
  "Cycles"](https://sourceware.org/binutils/docs/gprof/Cycles.html)). Separately, gprof's call-graph
  edges attribute time to *caller–callee pairs*, which is itself only "a first, coarse form of
  context sensitivity" that later profiling work found insufficient once a function is called from
  many distinct contexts with different costs — the motivation named for full calling-context-tree
  (path-sensitive) profilers such as HPCToolkit ([Adhianto et al., "HPCTOOLKIT: tools for
  performance analysis of optimized parallel programs"](https://www.cs.umd.edu/class/spring2021/cmsc714/readings/Adhianto-hpctoolkit.pdf)).
  Robert Hall's "call path refinement profiles" is the direct historical fix for exactly this
  problem in a non-recursive setting: attribute cost to the specific call-path suffix rather than to
  the merged function identity, so that a function invoked cheaply from one context and expensively
  from another is not blended into one misleading average ([Hall, "Call path refinement profiles",
  IEEE Transactions on Software Engineering 21(6), 1995](https://ieeexplore.ieee.org/document/391375/)).
- **`wdeg`'s own documented weaknesses**, found independently of the design question at hand but
  directly relevant to a "simple counter" scheme: (a) accumulated weight has *inertia* — a
  constraint that was hard early in search keeps a high weight even after it stops mattering, which
  is why weight-aging/decay was introduced, and which CHS's exponential-recency-weighted average was
  built specifically to fix ([Habet & Terrioux 2021](https://link.springer.com/article/10.1007/s10732-021-09475-z));
  (b) `wdeg` has no principled rule for global (n-ary) constraints — naively bumping every variable
  in a failed global constraint "dilutes the conflict information" onto variables that had nothing
  to do with the failure, which is the identical shape of complaint leveled against `implicated-in`
  here (one failure, many innocent objects charged) — see the discussion in ["Weight-Based Variable
  Ordering in the Context of High-Level Consistencies"](https://arxiv.org/pdf/1711.00909); (c) the
  weighting outcome is undefined/ambiguous when more than one constraint causes the same domain
  wipeout simultaneously, which is the CSP-solving analogue of PanGloss's own multi-cause dead ends.
- **1UIP as a deliberate rejection of "blame the whole implication graph."** Modern CDCL SAT solvers
  do not bump every variable that appears anywhere in the conflict's implication graph; conflict
  analysis resolves backward until reaching a single "first unique implication point" and learns
  (and credits) only the literals on that minimal cut — established experimentally as "the most
  useful single clause to learn" relative to alternative, less-minimal cut points (surveyed in
  [Liang, Ganesh, Poupart & Czarnecki 2015](https://arxiv.org/abs/1506.08905)). This is the SAT
  field independently converging on the same minimal-conflict-set answer QuickXplain gives for CSP.

## Empirical finding that matters more than the theory

Across two of the four fields surveyed, a genuinely simple, *terminated-at*-shaped counter was found
to be not just adequate but state-of-the-art in head-to-head competition, which should outweigh any
elegance argument for a more sophisticated scheme:

- In the most recent XCSP3 competitions, CHS — which is still fundamentally "increment a
  per-constraint counter every time that constraint is the immediate cause of a domain wipeout,"
  with only a recency-decay refinement over plain `wdeg` — outperforms `dom/wdeg` by a wide margin
  and needs no auxiliary heuristic, unlike some competitors: "CHS solves 127 instances more than
  MAC+dom/wdeg+s and 174 more than MAC+wdegca.cd" ([Habet & Terrioux, J. Heuristics
  2021](https://link.springer.com/article/10.1007/s10732-021-09475-z)). `dom/wdeg` itself, from 2004,
  remains a standard baseline in CSP solvers twenty years later.
- VSIDS — "increment a counter for every literal in each newly learned clause, periodically halve
  all counters" — has needed no fundamentally different mechanism since 2001 and remains "one of the
  most effective branching heuristics" in modern CDCL solvers despite its simplicity being described
  as "an enigma" (i.e., it is not fully theoretically explained, only empirically dominant) — and a
  2015 analysis found it empirically tracks a real structural signal (it "overwhelmingly picks,
  bumps, and learns bridge variables" connecting distinct problem communities) that a much more
  complex, principled scheme would have had to discover deliberately ([Liang, Ganesh, Poupart &
  Czarnecki, "Understanding VSIDS Branching Heuristics in Conflict-Driven Clause-Learning SAT
  Solvers", 2015](https://arxiv.org/abs/1506.08905)).

This is the strongest available argument for shipping `terminated-at`-as-a-rate first: two separate,
mature, competitively-benchmarked fields kept the simple immediate-cause counter as their production
default for two decades, refining its *recency/normalization*, not replacing its *fundamental shape*
with full-path blame.

## Open questions / what could not be established

- **No source found applies exact or approximate Shapley/Aumann-Shapley attribution to a search
  tree or DAG at anything near 10^6–10^8 leaves.** The tractability claim ("intractable even for a
  few dozen players") is well sourced for the general case; whether the tree's specific structure
  (fixed root-to-leaf ordering, not free coalition formation) admits a cheap closed form was not
  established here one way or the other, and should not be assumed.
- **No source found treats "weight a node's blame by the size of the subtree hanging below it" as
  an established blame-attribution technique.** The only literature combining trees with subtree-size
  weighting (Knuth 1975; Kilby, Slaney, Thiébaux & Walsh 2006) targets *total tree-size estimation
  by sampling*, a different problem from localizing which authored object is at fault. Treat any
  subtree-size-weighted blame scheme for PanGloss as a novel combination that has not been validated
  elsewhere, though the sampling estimators from that literature look directly reusable for
  estimating this scheme's denominator cheaply.
- **QuickXplain's per-call cost at PanGloss's scale is estimated, not measured anywhere.** Junker's
  paper and its 2020 AAAI Classic Paper retrospective describe wall-clock wins over the naive
  exponential baseline, not an absolute cost budget suitable for extrapolating to 10^6–10^8 dead
  leaves; this document's claim that it is a targeted, opt-in diagnostic rather than a global counter
  rests on the shape of the algorithm (extra consistency-check calls per invocation), not a
  benchmark at this scale.
- **The PET/Callmeier quick-check literature's exact counting mechanism (numerator and denominator
  as implemented in the actual system, versus in the Malouf/Carroll/Copestake paper) could not be
  directly verified**: the Cambridge Core and arXiv-hosted PDFs for the primary quick-check papers
  returned binary/undecodable content or HTTP 403 through the tools available in this session, and
  only secondary summaries (search-result snippets, DELPH-IN wiki pages) were reachable. The
  fail-rate-ordering claim used above is corroborated by multiple independent secondary sources but
  is not confirmed against the primary PDF text.
- **No explicit critique of VSIDS-style credit assignment as "over-crediting" was found** beyond the
  structural fact that 1UIP already restricts credit to a minimal cut (which is itself evidence the
  field considered and rejected broader crediting, but this is an inference, not a quoted critique).
