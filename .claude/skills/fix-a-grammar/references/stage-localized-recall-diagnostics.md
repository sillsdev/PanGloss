# Stage-localized recall diagnostics

Use this procedure when a grammar's FST proposer omits one or more analyses accepted by the
oracle. Do it before selecting a recipe winner: an unexplained false negative is a correctness
defect, not a performance datapoint.

## Purpose

Construct one reusable, bounded diagnostic pipeline that answers:

1. Does the oracle contain the required complete analysis?
2. Can the lexicon network realize its complete tag chain?
3. After the ordered rule cascade, does the same complete analysis–surface pair remain?
4. After boundary cleanup and final composition, does that exact pair remain?

The first false cell localizes the earliest observed failing boundary; it does not by itself prove
root cause. That result narrows the recipe space: a lexicon loss points toward source allocation or
morphotactic construction; a post-rule loss implicates `Replace` ordering, grouping, alphabets, or
gate scope; a cleanup-only loss implicates boundary handling or the final `Compose`. Do not vary
unrelated tree dimensions merely because they are available.

## Probe model

Represent probes as data, not one test per word:

```text
AnalysisSpec:
  stable morpheme/rule identifiers
  root position or other identity needed to disambiguate the analysis

ProbeSpec:
  cluster name
  surface word
  one or more required complete AnalysisSpecs

ProbeResult:
  oracle_present
  lexicon_complete_analysis_present
  post_rules_exact_pair_present
  final_exact_pair_present
  final_tags_atomic_diagnostic_only
  final_exact_intersection
  first_failing_boundary | inconclusive_budget_exceeded | harness_mismatch
```

Resolve stable identifiers back to readable grammar entries in failures. For an ambiguous surface,
success means that each explicitly required oracle reading is preserved; do not accidentally
require unrelated homophones, and do not accept an arbitrary reading as proof for a named one.

## Bounded construction

- Declare oracle step/time limits, per-operation time limits, state/arc ceilings, and a memory
  budget before running. A finite automaton operation is not necessarily operationally bounded.
  Return `inconclusive_budget_exceeded`, never `absent`, when any cap or watchdog fires.
- Load the grammar and compile each stage once. Reuse those stage networks for every probe.
- Reconstruct intermediate stages in test or diagnostic code when the production result exposes
  only the final network. Avoid expanding the production API solely for observation.
- Record and compare stage order, configuration, selected rules, and every compile/skip disposition
  with production. Report intentional confirmation-only skips separately. Verify the reconstructed
  final relation against production on a stable fingerprint or discriminating corpus; otherwise
  return `harness_mismatch` rather than attributing a grammar failure.
- Use finite composition, projection, and intersection to ask membership questions. Avoid
  unrestricted `apply_up`/`apply_down` enumeration for shared diagnostics: a result cap is not a
  hard time bound when enumeration stalls before producing a result.
- At every relational boundary, intersect the same complete analysis with its expected surface;
  surface-only reachability can succeed on the wrong lexical path. Before cleanup, allow boundary
  symbols before, between, and after surface tokens inside that exact-pair matcher.
- Probe complete accepted tag chains. If only a prefix is known, use an explicit prefix acceptor
  followed by zero or more known tag symbols and label the evidence as prefix reachability—not a
  complete analysis.
- Run the pipeline in one adequately sized worker and consume or release intermediate networks
  sequentially when peak memory matters.

Compute all result rows before asserting. Print one matrix containing every probe and its earliest
observed failing boundary; this keeps one early failure from hiding the other clusters.

Also emit a machine-readable record (JSON or TSV) containing the grammar/content hash, compiler
revision, stage configuration/fingerprint, declared budgets, cap status, stage timings and
state/arc counts, every probe result, and the commands/environment needed to reproduce it. Define
stable exit statuses for success, exact-pair failure, budget-inconclusive, and harness mismatch so a
recipe-search script cannot mistake missing evidence for a negative result.

## From diagnosis to three recipe candidates

First decide whether the evidence identifies a recipe-space problem. A malformed rule lowering,
encoding defect, or incorrect cleanup relation requires a semantic repair first; report “not a
recipe-space problem” and do not search or score tree shapes while exact-pair recall is red.

When tree structure is implicated, form three structurally distinct candidates over the existing
five plan primitives, varying only dimensions that can change the first failing boundary. Examples
include `Compose`/`Replace` order or grouping for rule-stage losses, `Gate` partition or common
prefix for feature/table scope, and `Union` branch or source allocation for lexicon/morphotactic
losses. These are possibilities, not a mandatory one-of-each trio.

Every candidate must turn all previously red exact intersections green, retain the full oracle
recall corpus, preserve complete candidate/analysis multisets when that is the contract, and pass
cross-grammar conformance. Tag atomicity is diagnostic evidence, not a substitute for the exact
complete analysis–surface intersection gate. Only then compare build time, size, states/arcs, or
latency using one of the three recipe-search methods.

## Approved Aweti structural-allomorph design

The Aweti residual investigation established two independent failures and therefore two independent
recipe dimensions:

- An RTL phonological rule is compiled as a confirmation-only superset. In a propose-then-confirm
  pipeline, preserve an identity alternative at that individual RTL stage so an over-proposal
  cannot destroy an otherwise valid analysis before HermitCrab confirms it. Do not make the whole
  cascade optional: the global optional cascade exceeded the diagnostic cap, while the targeted
  exact-analysis-first variant stayed bounded and recovered `tsãn`.
- The templated lexc emitter concatenates literal `InsertSegments` and deliberately skips global
  structural-composite enumeration. That loses affine suffix processes whose RHS copies a stem
  variable but drops or replaces a matched tail. For example, Aweti `-aw` has an allomorph matching
  `variable + ou` and producing `copy(variable) + w + +aw`; literal concatenation produces the
  wrong `...oko+aw`, while the intended relation produces `...okw+aw`.

The approved general repair is a bounded local structural-rewrite layer:

1. During templated emission, give each supported structural allomorph an opaque, allomorph-owned
   marker instead of pretending its inserted text is a complete underlying realization.
2. Compile that allomorph's local LHS/RHS relation over char-definition tokens, including natural
   class membership, copied parts, dropped matched material, inserted boundaries, and inserted
   segments.
3. Compose this structural layer immediately after templated lexc and before phonological rules.
   Union it with the old literal/identity path while the proposer remains confirmation-only; never
   remove an existing proposal until exact relation parity is proven.
4. Scope compilation to adjacent, single-sided affine affix processes first. Refuse or report
   unsupported `Modify`, `InsertContext`, nonlocal copying, reduplication, and unbounded recursion
   rather than silently approximating them.
5. Key and memoize local transducers by normalized action shape, table, natural-class membership,
   and inserted token sequence. This avoids the rejected `roots × rules^depth` enumeration and
   makes cost depend on distinct structural shapes instead of lexical root count.

A minimal conformance fixture must discriminate `stem + class` → `stem + replacement + suffix`
from literal suffix concatenation. The Aweti gates then require exact pairs for both `kỹjokwaw` and
`tsãkỹjokwaw`, plus the full corpus and cross-grammar recall gates.

## Three scriptable maps of recipe space

For a new grammar, materialize the same finite candidate registry and interaction graph, then map
and search it in three complementary ways:

1. **Constraint lattice / bounded enumeration.** Encode each legal choice—source allocation,
   `Compose` order and grouping, `Union` fallback, `Gate` key, `Replace` cascade, and structural
   rewrite family—as a finite variable. Apply capability, ordering, recall, and resource
   constraints before building. Exhaust the remaining combinations within declared bounds and
   return the Pareto frontier. This is the strongest “optimal within this finite space” claim and
   is best when the legal space is small.
2. **Interaction graph / dynamic programming.** Decompose the FST plan at separators whose
   subgraphs share only a declared interface alphabet or gate key. Memoize non-dominated partial
   plans by interface signature, then combine frontiers bottom-up. Use this when subtrees repeat or
   most choices are locally independent; fall back to joint enumeration for every connected
   interaction component.
3. **Empirical portfolio / sequential allocation.** Seed at least three structurally distinct
   recipes, run cheap recall/capability gates first, then allocate progressively larger corpus and
   timing budgets to surviving candidates using successive halving or confidence-bound racing.
   Return the best three non-dominated feasible candidates when the budget expires. This is best
   when build cost and interaction effects cannot be predicted reliably; its optimality claim is
   “best observed under the registered budget,” never global.

The script should run all three when affordable and report agreement or disagreement. A winner is
eligible only if exact-pair and full-oracle recall are green. If all three maps reject every
combination, return a semantic/compiler defect with the first failing boundary instead of choosing
the least-bad recipe.
## Aweti-derived cautions that generalize

- Cluster related misses by shared root, affix chain, or rule path so one mechanism is tested by a
  short discriminating ladder rather than many surface-only assertions.
- A longer failing analysis may contain an extra rule while a shorter sibling also fails; the
  extra rule cannot be the shared primary cause.
- Tag-symbol atomicity is useful diagnostic evidence but does not prove semantic reachability.
- Preserve a named exact miss set until an approved semantic fix changes it. Performance work must
  not convert missing analyses into an accepted baseline.
