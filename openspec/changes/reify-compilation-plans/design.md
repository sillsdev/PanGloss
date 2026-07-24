## Context

This is the "massive refactor" (ADR 0002): the compile step becomes a **reified, enumerable
compilation plan** rather than hardcoded topology branching. It is the substrate the capability
characteristics check (`add-capability-characteristics-check`) composes its envelope over, so the
two are co-designed; per `STAGING.md`, **the reified `Plan` lands first** and the envelope composes
over it.

A topology-SOTA research pass (six areas: FST-toolkit plan representation, composition-order
optimization, differential/translation-validation testing, combinatorial interaction testing,
orthogonality proofs, and the overapproximate-then-confirm pattern) informs every decision below.
Its load-bearing finding: **no production FST toolkit (OpenFST, foma, HFST, Pynini/Thrax, Kleene)
reifies its composition topology as enumerable, selectable data.** Thrax/Pynini build an AST but
evaluate it exactly once, straight-line; foma/HFST hardcode textual/shell order. So there is no
abstraction to adopt wholesale — we design `Plan` from first principles, borrowing *shapes* from
adjacent disciplines (Volcano query optimization, graph-transformation confluence theory,
CEGAR/soundy analysis).

## Decisions

### D1. `Plan` is a content-addressed AND-OR DAG, not a tree

Modeled on Graefe's Volcano optimizer's Logical Query DAG (the closest existing formalism for "many
legal topologies for one logical request"): OR-nodes are equivalence classes ("ways to compute the
same relation"), AND-nodes are operators. A closed node-kind enum, specialized to FSTs:

- `Leaf { fragment, provenance }` — an atomic FST compiled from source (a lexc-compiled lexicon
  fragment, a single rewrite rule's transducer, a gate/guard automaton). `provenance` records which
  grammar construct it encodes and is the source of ADR 0001's `capability evidence provenance`
  field.
- `Compose { children: Vec<NodeId>, strategy }` — **n-ary, not binary**. Allauzen & Mohri's 3-way
  composition result proves n-ary composition is strictly cost-relevant (not sugar for a binary
  fold) when out-degrees are skewed. `strategy ∈ {Static, Lazy, LazyLookahead}` is the *physical*
  composition strategy (materialize-then-trim vs. on-the-fly vs. on-the-fly with a label-reachability
  lookahead filter), kept **separate from topology** so the cost model can vary it per edge without
  the enumerator emitting a combinatorially separate topology per strategy.
- `Union { children }` — merges independently-compiled branches. Legal only where the
  characteristics check's orthogonality predicate licenses it (see `add-capability-characteristics-
  check` design; parallel-independence / critical-pair non-overlap). This is the node kind through
  which "proving orthogonality retires combination space" (ADR 0001) becomes concrete.
- `Gate { partition, children }` — PanGloss's existing `gate.rs` subrule-gated partition-and-union,
  promoted to a named node kind: partition entries by their gate key, compile one child network per
  group, union. Today's `gate::partition_entries` becomes this node's partition function.
- `Replace { … }` — PanGloss's existing `replace.rs` rewrite-cascade construction, promoted to a
  named node kind so the enumerator can reorder/rewire around it.

**Node identity is content-addressed**: `NodeId = hash(kind, child NodeIds, config)`. This is what
makes (a) the plan cache key, (b) cross-plan subtree sharing (two plans differing only in how the
phonological cascade is grouped share their identical lexicon leaves — measured once, stored once),
and (c) the memoized AND-OR search actually work. A tree would force duplicating shared subtrees.

### D2. The three hardcoded seams become enumerator decisions over node kinds

The refactor's concrete deliverable is deleting the imperative branching and re-expressing each as a
choice the enumerator makes when emitting candidate plans:

| Today (imperative) | Location | Becomes |
|---|---|---|
| `preexpand::should_run` | preexpand.rs:199 | whether the plan includes the composite-emission `Leaf`/subtree at all (a no-op-elidable subtree, not a bool) |
| `emit::probe_would_refuse` | emit.rs:1729 | whether the plan routes affix rules through the **structural-composite** subtree vs. the ordinary concatenative subtree — a topology choice, enumerable as two candidate plans the oracle can diff |
| `gate::partition_entries` | gate.rs:224 | the `Gate` node's partition function; "ungated" collapses to a single-group `Gate` (== today's degenerate 1-group behavior), so this is a strict generalization |

Because these are now *alternative plans for the same grammar*, they are exactly the ≥2 plans the
differential oracle (D4) diffs — the refactor pays for its own correctness check.

### D3. Selection is capability-safe by construction; deterministic objective

The enumerator emits only plans all of whose nodes pass the characteristics-check envelope
(`add-capability-characteristics-check`). **Every capability-passing plan is recall-preserving**, so
all produce the identical confirmed set — selection can never pick a fast-but-wrong plan; it only
trades cost. The default selection objective is deterministic and cheap: minimize a
measure-or-estimate of `(states + arcs)` (controllable path) / payload size (black-box foma path),
tie-broken by content-address for reproducibility. The projected-cost model with error bounds, the
committed-plan cache, and profile-guided autotuning are **parked** to `add-compilation-cost-planner`
(ADR 0002), triggered by real multi-topology pressure; v1 is "enumerate, filter by capability, pick
by measured/estimated size, build."

### D4. Differential-correctness oracle — two tiers

The free oracle ADR 0002 promises, made rigorous per the differential/translation-validation
research:

- **Cheap, always-on tier.** For any grammar with ≥2 capability-passing plans, run the (already
  committed, already ground-truthed) conformance corpus through both proposers and assert
  `confirm(propose_{P1}(w)) == confirm(propose_{P2}(w))` **as sets**, per word. This is a
  cross-configuration *metamorphic relation* that is an exact equality (not a statistical
  invariant) because both sides funnel through the same trusted confirm engine — a stronger position
  than typical differential testing, which lacks any independent oracle. On mismatch, emit the
  **shortest disagreeing word** plus the **symmetric difference** of proposed analyses (the
  CFG-equivalence-tool pattern), and classify it as a predicate bug (a capability gate admitted a
  plan that is not in fact recall-equivalent). This is just one extra pass of the existing
  conformance run.
- **Expensive, opt-in tier.** Exact equivalence of the two plans' propose-language projections.
  FST equivalence is undecidable in general but **decidable for functional/finite-valued
  transducers**; our proposer is finite-valued (each word → a bounded number of analyses), so this
  is reachable, reserved for tuning runs where a stronger-than-sampling guarantee is wanted before
  committing a plan change. Marked a stretch goal, not a v1 requirement.

### D5. Nodes are individually addressable fuzz targets

(Grill requirement feeding `add-pairwise-grammar-interaction-coverage`'s reframe.) Because every
node has a stable content-address and a declared kind, a conformance/fuzz fixture can be tagged by
**which plan-node-kind pairs/triples it exercises** — extending the existing `constructs.txt` /
`exercises:` metadata. Interaction coverage is then t-wise coverage over *(node kind, adjacent node
kind, strategy)* tuples restricted to capability-legal combinations (a constrained CIT / software-
product-line sampling problem), not covering arrays over raw grammar knobs. The plan DAG *is* the
interaction surface.

### D6. Propose is a soundy / may-analysis; only language-preserving ops inside it

Formal framing (CEGAR + may/must analysis): recall ("this word *may* have analysis X") is a **may**
property, so propose must **over-approximate** to be sound *for recall*; precision is confirm's job.
Enforced as a hard construction rule: **only language-preserving operations** (trim/coaccessible,
ε-removal, determinization-where-valid, minimization) may appear anywhere in a `propose` pipeline.
Any operation that can change the recognized relation — weight-based beam pruning, top-k / best-path
shortcuts — is **categorically forbidden in propose** and confined to confirm/ranking. This is the
structural counterpart to ADR 0001's confirm-only-by-default.

## Dependencies

Co-designed with `add-capability-characteristics-check` (lands first, before that change's envelope).
Single-owner over the STAGING merge hotspots `replace.rs` / `gate.rs` / `emit.rs` / the composition
constructor for the duration of this change. `lower-fst-pattern-environments` (Stage 1B) supplies the
shared IR the `Leaf` fragments and predicates compile patterns through. Grants no new construct
capability by itself.

## Novelty / risk (flagged, per research)

The database-optimizer analogy is strong for the *search* (AND-OR DAG, memoized DP) but weak for the
*cost function* — FST compose size has no mature estimator, so cost never prunes alone; build-and-
measure resolves ties (the discipline ADR 0002 independently reached). The genuinely novel move —
**building ≥2 independently-derived over-approximations of one grammar and using their disagreement
as a designed-in correctness oracle** — has no found prior art (it composes CEGAR-style sound
over-approximation with differential/metamorphic testing). Treat D4 as this project's research
contribution and document it rigorously; there is no existing paper of pitfalls to lean on.
