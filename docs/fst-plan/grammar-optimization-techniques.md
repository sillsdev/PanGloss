# Grammar optimization technique catalogue and scoring metric

Reference document for the `fix-a-grammar` skill (`.claude/skills/fix-a-grammar/`). This is not
itself the skill's process — that is designed separately — but the catalogue and metric the skill
points at when it requires an engineer to consider **at least three genuinely different
multi-tree/multi-FST models** (including reallocating parts of the grammar into new construction
steps), score them, implement one, verify it empirically, and pin the result with a conformance
fixture.

Research only. Nothing in this document is wired into any compile path; no crate `src/` was
touched to write it.

## How to read an entry

Each catalogue entry states, in order: what the technique is (one paragraph), what problem shape
it addresses, whether **this repo already uses it** (with a file:line citation if so), a real,
checkable citation, and a battle-proven Rust reference where one exists. Status tags used
throughout:

- **In use** — cited to a specific file/line in this repo.
- **Candidate** — not used, plausible for a future grammar; no repo evidence either way.
- **Dead end (this shape)** — considered or tried for PanGloss's propose-FST/confirm-HC
  architecture specifically and rejected, with the reason. Not a claim the technique is bad in
  general.
- **Tried and reverted** — actually prototyped in this repo, kept as a documented negative result.

Citations are given as author/year/venue so they can be looked up independently; several were
verified by web search while writing this document (noted where relevant). Where no credible Rust
implementation exists, that is stated plainly — a gap is decision-relevant, not embarrassing.

---

## Part 1 — The catalogue

### A. Automata construction and size

**A1. Determinization (subset construction).** Converts a nondeterministic automaton into an
equivalent deterministic one so each input string has at most one run. *Problem shape:* efficient,
unambiguous per-word traversal; a prerequisite most minimization algorithms assume.
*Status:* **In use**, inherited from the vendored engine, not reimplemented here — vendored
`foma = "=0.4.2"`'s `fsm_compose` "internally minimizes both operands before composing"
(`rust/crates/pg-foma/src/compose_budget.rs:25-27`), which routes through
`foma::determinize::fsm_determinize` (imported by `foma-0.4.2/src/minimize.rs`). PanGloss's own
`Plan`/`build.rs` layer never re-implements subset construction; it only decides *what* to compose,
never *how* one composed net is determinized. *Citation:* Rabin & Scott (1959), "Finite Automata
and Their Decision Problems," *IBM Journal of Research and Development* 3(2). *Rust reference:*
vendored `foma` crate (in-tree); `rustfst` (github.com/garvys-org/rustfst, a from-scratch Rust
re-implementation of OpenFst) implements determinization independently and is a real,
crates.io-published alternative toolkit, verified live.

**A2. Minimization — Hopcroft partition refinement + Brzozowski fallback.** Collapses a
deterministic automaton to the unique minimal equivalent one. *Problem shape:* artifact-size
reduction (trigger b) after any composition step. *Status:* **In use.** Vendored
`foma-0.4.2/src/minimize.rs:1-14`'s own header: "Hopcroft partition-refinement minimization plus
the Brzozowski fallback" — a literal, bug-for-bug port, confirmed by reading the file directly.
Every `crate::gate`/`crate::replace` compose step in `pg-foma` ends with a minimize call
(`compose_budget.rs`'s own doc: "`fsm_union` does **not** minimize... the eventual minimize is the
true worst-case moment"). *Citation:* Hopcroft (1971), "An n log n Algorithm for Minimizing States
in a Finite Automaton," in *Theory of Machines and Computations* (Academic Press); Brzozowski
(1962), "Canonical Regular Expressions and Minimal State Graphs for Definite Events," in
*Mathematical Theory of Automata*. *Rust reference:* the vendored `foma` crate itself is the
reference implementation in this repo; `rustfst` also ships minimization independently.

**A3. Paige–Tarjan partition refinement.** A more general relational coarsest-partition algorithm
that Hopcroft's DFA-specific method is a specialization of. *Problem shape:* same as A2, for richer
relational structures than plain DFAs (labeled transition systems, bisimulation). *Status:* **Dead
end (this shape) — redundant, not wrong.** The vendored port already ships Hopcroft's
automaton-specific `O(n log n)` algorithm (A2); Paige–Tarjan's added generality buys nothing here
because PanGloss's compiled artifacts are ordinary (weighted) automata, not the richer relational
structures Paige–Tarjan targets. *Citation:* Paige & Tarjan (1987), "Three Partition Refinement
Algorithms," *SIAM Journal on Computing* 16(6).

**A4. Hyper-minimization.** Minimizes an automaton up to a *bounded number of differing strings*
rather than exact language equality. *Problem shape:* further size reduction when a handful of
false accepts/rejects are tolerable. *Status:* **Dead end (this shape), explicit.** This directly
contradicts the propose-and-confirm invariant every other document in this repo treats as
inviolable: the proposer must have **100% recall**, never approximate downward
(`.claude/skills/dead-end-census/SKILL.md`: "100% recall is inviolable and there is no per-grammar
fallback tier"). Hyper-minimization is, by definition, a controlled *language change* — exactly the
forbidden direction. *Citation:* Badr, Geffert & Shipman (2009), "Hyper-Minimizing Minimized
Deterministic Finite Automata," *RAIRO — Theoretical Informatics and Applications* 43(1).

**A5. ε-removal.** Eliminates epsilon transitions before determinization/minimization.
*Status:* **In use**, inherited from the vendored engine (part of its determinize/minimize
pipeline); also directly relevant to `FomaOptions::flag_is_epsilon`, discussed under F1 below.
*Citation:* Mohri (2002), "Generic ε-Removal and Input ε-Normalization Algorithms for Weighted
Transducers," *International Journal of Foundations of Computer Science* 13(1). *Rust reference:*
vendored `foma`; `rustfst`'s `rm_epsilon` algorithm independently.

**A6. Incremental minimal-automaton construction (Daciuk et al.).** Builds a minimal deterministic
acyclic automaton in one pass over *sorted* input, minimizing on the fly instead of
building-then-minimizing. *Problem shape:* build-time reduction (trigger a) for large lexicons —
directly relevant to the templated-lexc emitter, whose own calibration note records a 23,661-state/
346,727-arc lexc-only compile before the full cascade composition even starts
(`rust/crates/pg-foma/src/compose_budget.rs:76-89`, Aweti figures). *Status:* **Candidate**, not
used. *Citation:* Daciuk, Mihov, Watson & Watson (2000), "Incremental Construction of Minimal
Acyclic Finite-State Automata," *Computational Linguistics* 26(1), pp. 3–16 (verified via search:
ACL Anthology J00-1002). *Rust reference:* no exact port of Daciuk's algorithm was found. The `fst`
crate (BurntSushi, widely used, e.g. by ripgrep's index and ripgrep-adjacent tooling) builds
compact ordered automata from sorted key/value input, but via a different construction (streaming
insertion of already-sorted keys into a minimal-ish structure), not Daciuk's exact incremental-merge
algorithm — cite as an *adjacent*, not equivalent, battle-tested crate; verify its determinism/
minimality guarantees independently before relying on it structurally.

**A7. DAWG (Directed Acyclic Word Graph / suffix automaton).** A minimal automaton recognizing all
substrings of a text, built for substring-indexing workloads. *Status:* **Dead end (this shape).**
PanGloss has no substring-indexing problem: lexicon lookup is whole-root/whole-word, mediated by
composed rule cascades, not a "does this text contain this substring" query. Do not confuse with A6
(also sometimes informally called "MA-FSA" in NLP contexts) — that one is a live candidate; this one
is not applicable. *Citation:* Blumer, Blumer, Haussler, Ehrenfeucht, Chen & Seiferas (1985), "The
Smallest Automaton Recognizing the Subwords of a Text," *Theoretical Computer Science* 40.

**A8. LOUDS / succinct representations.** Bit-packed tree/graph encodings that avoid pointer
overhead. *Problem shape:* shrinking artifact **bytes** specifically (as opposed to state/arc
*count*, which composition/minimization already govern) — relevant to `health.rs`'s
`Metric::PayloadBytes`/R6 size bands. *Status:* **Candidate, low priority.** The measured size
problem in this codebase today is compose-time state/arc blowup (governed by `ComposeBudget`), not
encoding density of an already-built net; the vendored artifact is already gzip-compressed on
serialization (`docs/fst-plan/foma-fst-plan.md` D5: "gzip via `flate2`"). Worth revisiting only if a
future grammar is capability-safe, state/arc-healthy, and *still* too large in bytes to distribute —
not observed yet. *Citation:* Jacobson (1989), "Space-Efficient Static Trees and Graphs," *30th
Annual Symposium on Foundations of Computer Science*. *Rust reference:* `succinct` crate exists but
is thin; no PanGloss-scale precedent found.

**A9. Failure/φ-transitions (Aho–Corasick).** A single automaton matching many literal patterns in
one text pass via failure links. *Status:* **Dead end (this shape).** There is no "scan raw text for
many literal patterns" subproblem at any layer PanGloss owns: matching is already done through
composed weighted FSTs over a structured segment alphabet, with natural-class matching resolved by
feature unification (`pg_rules::bridge::nat_class_lanes`, cited in
`docs/conformance/multitable-shared-representation-design.md:69-75`), which strictly generalizes
multi-pattern matching for this domain. A pre-filter *before* the FST was tried in a different guise
and killed on measurement (see E5 below) — this is the same shape of idea, already ruled out
empirically, not merely by analogy. *Citation:* Aho & Corasick (1975), "Efficient String Matching:
An Aid to Bibliographic Search," *Communications of the ACM* 18(6).

### B. Composition and laziness

**B1. Composition filters (Allauzen & Mohri).** Filters that suppress redundant epsilon-paths
during weighted-transducer composition, and the associated result that composing three transducers
at once beats folding two binary composes. *Status:* **Partially in use.** The n-ary-cost half is
explicitly cited in this repo's own type design:
`rust/crates/pg-foma/src/plan.rs:18-20`, `PlanNodeKind::Compose` doc — "Allauzen & Mohri's 3-way
composition result: n-ary is cost-relevant, not sugar for a binary fold." The epsilon-filter
machinery itself is inherited from the vendored `foma` crate's own `fsm_compose`, never
re-implemented at the `pg-foma` plan layer. *Citation:* Allauzen & Mohri (2008), "3-Way Composition
of Weighted Finite-State Transducers," *CIAA 2008*, LNCS 5148, pp. 262–273 (verified via search);
Allauzen & Mohri (2009), "N-Way Composition of Weighted Finite-State Transducers," *International
Journal of Foundations of Computer Science* 20, pp. 613–627. *Rust reference:* `rustfst` implements
OpenFst-style composition filters (`TrivialComposeFilter`, `SequenceComposeFilter`,
`MatchComposeFilter`) directly — a different toolkit lineage from PanGloss's vendored
`mhulden`-derived `foma`, but a real, checkable Rust implementation if this crate is ever adopted.

**B2. Lazy / on-the-fly composition.** Expands composed states only as visited, instead of
materializing the full product up front. *Problem shape:* build time and artifact size when only a
fraction of the composed state space is ever reached by real input — the single clearest
"designed-for, not built" item in this codebase. *Status:* **Researched, but not modeled in the
active plan vocabulary.** `crate::plan::ComposeStrategy` contains only `Static`. The former
on-the-fly variants were removed because no builder could construct or execute them; `build.rs`
therefore has no strategy-rejection panic. `plan_interaction_coverage.rs`'s closed 7-tuple
legal-adjacency set confirms `Static` is the only active composition strategy. Adding B2 would
require a new executable strategy and backend implementation, not selecting dormant enum variants.
*Citation:* Allauzen, Riley, Schalkwyk, Skut & Mohri (2007), "OpenFst: A
General and Efficient Weighted Finite-State Transducer Library," *CIAA 2007*, LNCS 4783 (the
`ComposeFst` lazy-composition design this variant's name is patterned on). *Rust reference:*
`rustfst`'s lazy `Fst` trait objects/`ComposeFst` are the battle-proven analog; the vendored `foma`
crate has **no** lazy-compose primitive at all (confirmed: `fsm_compose` is a synchronous, tight
loop with no mid-call hook, `compose_budget.rs:21-24`) — adopting B2 here would mean either building
a lazy layer atop vendored `foma`'s eager primitives (nontrivial) or introducing a second FST toolkit
for the paths that need it.

**B3. Local determinization.** On-demand determinization of a lookahead-composed transducer,
avoiding a full separate determinize pass. *Status:* **Candidate**, not used; same family as B2 and
subject to the identical toolkit gap. *Citation:* part of the same OpenFst lazy-composition design
(Allauzen et al. 2007, above). *Rust reference:* `rustfst`, same caveat as B2.

**B4. Bimachines.** A pair of deterministic automata (one reading left-to-right, one right-to-left)
whose combination realizes functions no single subsequential transducer can, with `O(|w|)`
deterministic apply. *Problem shape:* trigger (d), per-candidate apply cost, for constructs whose
ambiguity resolves to a *function* even though no one-pass deterministic transducer computes it.
*Status:* **Candidate**, not used. *Citation:* Elgot & Mezei (1965) origin; accessible modern
treatment in Roche & Schabes, eds. (1997), *Finite-State Language Processing*, MIT Press; Mohri
(1997), "Finite-State Transducers in Language and Speech Processing," *Computational Linguistics*
23(2), discusses bimachine/subsequential constructions specifically for morphological analyzers.
*Rust reference:* none found; honest gap.

**B5. Sequential vs. subsequential transducers.** A transducer is sequential (deterministic,
one output per input) or *p*-subsequential (deterministic up to bounded output ambiguity `p`).
*Problem shape:* if a sub-cascade can be shown *p*-subsequential, its apply cost becomes a
deterministic `O(|w|)` traversal instead of nondeterministic search. *Status:* **Candidate, with a
real caveat.** Most of this repo's grammars carry genuine multi-analysis ambiguity by design (Sena's
`mbali` returns 8–15 analyses depending on engine, `docs/fst-plan/foma-fst-plan.md` D4) — a
whole-grammar subsequentiation is therefore not generally available. The candidate is per-*branch*:
identify sub-cascades with no genuine ambiguity (most phonological rewrite cascades, as opposed to
the lexical-choice/compounding layer) and check whether *those* subnets are already subsequential
(likely, since `fsm_compose` determinizes its operands) versus whether further work would be needed.
*Citation:* Mohri (1997), above; Allauzen & Mohri (2003), "Finitely Subsequential Transducers,"
*International Journal of Foundations of Computer Science* 14(6) (the *p*-subsequentiation algorithm
and its decidability limits).

### C. Factoring and decomposition — most relevant to "reallocate into new construction steps"

**C1. Graph partitioning (METIS / spectral).** Splits a large graph into balanced parts minimizing
edge cut, for distribution or divide-and-conquer. *Status:* **Dead end (this shape).** METIS-style
partitioning optimizes a *proxy* (edge cut) with no correctness meaning for this problem. PanGloss's
"partitioning" (`crate::gate`'s lexical partition-by-gating-key) is instead **required** to respect
lexical disjointness by construction — `gate.rs`'s own module doc argument for "why the union is
safe here" is a semantic property (each group of lexical entries realizes one truth assignment of
the gated MPR/POS features), not a graph-cut heuristic. An off-the-shelf partitioner has no way to
know that constraint and could produce an unsound split. *Citation:* Karypis & Kumar (1998), "A Fast
and High Quality Multilevel Scheme for Partitioning Irregular Graphs," *SIAM Journal on Scientific
Computing* 20(1).

**C2. Tree decomposition / treewidth.** Decomposes a graph into a tree of small "bags" of vertices;
treewidth is the tightness of the best such decomposition, and a classic complexity proxy for
graph-structured problems (many NP-hard problems become linear-time given bounded treewidth).
*Problem shape:* PREDICTING, before building, whether a proposed grammar refactoring (splitting one
`Compose` node into several `Gate`-partitioned pieces, or a multi-table split like
`docs/conformance/multitable-shared-representation-design.md`'s own worked decision) will blow up.
*Status:* **Candidate — the principled successor to an already-flagged placeholder.**
`characterization.rs`'s own `rule_interaction_product_finding` uses `mrule_count * prule_count` as a cost
proxy and says so explicitly: "a conservative, provisional placeholder... no real-grammar
calibration evidence exists yet for this specific product" (`rust/crates/pg-foma/src/characterization.rs`,
lines ~72–80). A treewidth-style analysis of `plan_interaction_coverage.rs`'s own adjacency-tuple
graph (which constructs actually interact, at which plan nodes) is exactly the more principled
version of that placeholder ADR 0001 anticipates when it says cost gating is "a predicted
conservative bound," not an invented multiplier. *Citation:* Robertson & Seymour (1986), "Graph
Minors. II. Algorithmic Aspects of Tree-Width," *Journal of Algorithms* 7(3); for the CSP framing
most directly analogous to a grammar's rule-interaction graph, Freuder (1985), "A Sufficient
Condition for Backtrack-Free Search," *Journal of the ACM* 32(4). *Rust reference:* no widely-used,
battle-tested Rust treewidth crate was found. This is a case where "no good Rust option" is an
acceptable honest gap rather than a blocker: the plan graphs in question (adjacency tuples over five
node kinds) are tiny, so an exact/naive treewidth computation, hand-written for this purpose, is
entirely adequate — general-purpose treewidth solvers exist for much larger, harder instances than
this repo will ever produce.

**C3. Junction trees.** A tree decomposition used specifically to drive exact inference by message
passing between cliques. *Status:* **Candidate**, same status and application as C2/C4 — not
independently useful without an elimination-ordering problem to apply it to. *Citation:* Lauritzen &
Spiegelhalter (1988), "Local Computations with Probabilities on Graphical Structures and Their
Application to Expert Systems," *Journal of the Royal Statistical Society* B 50(2).

**C4. Bucket / variable elimination.** Eliminates one variable at a time from a constraint/inference
network by grouping ("bucketing") every constraint mentioning it and combining them, choosing an
elimination *order* to keep intermediate bucket sizes small. *Problem shape:* directly analogous to
what `gate.rs` already does informally, and the clearest concrete lever for "reallocating grammar
structure into new construction steps" when *several* independent gating features interact.
*Status:* **In use, informally — worth formalizing.** `crate::gate`'s partition-by-gating-key is
exactly a one-shot elimination step: pick every gated subrule's key jointly, split the lexicon into
the cross product of truth-value buckets (`GatePartitionSpec::groups`, `plan.rs:230-246`), compile
each independently, union. Today's `partition_entries` takes the **full joint cross-product** of
every gated key at once rather than eliminating one variable/feature at a time in a chosen order —
which is fine at today's scale (Indonesian: 1 gated subrule; Amharic: 3) but is precisely the
mechanism bucket elimination generalizes once several *independent* gates interact: eliminating the
most-constraining (lowest-cardinality) feature first shrinks the group count before the remaining
cross-product is taken, the same win bucket elimination gets over naive joint enumeration. This is
an algorithm-design lever on `gate.rs`'s own construction, not a library import. *Citation:* Dechter
(1996), "Bucket Elimination: A Unifying Framework for Probabilistic Inference," *UAI 1996*
(verified via search; journal version in *Constraints* 2(1), 1997, and *Learning in Graphical
Models*, 1999).

**C5. SCC condensation.** Collapses each strongly-connected component of a directed graph to a
single node, turning a cyclic graph into a DAG. *Problem shape:* cyclic feeding/bleeding between
rules. *Status:* **Not needed today, narrow candidate.** The one genuinely cyclic construct in this
repo — recursive endocentric compounding, where a `CompoundingRuleDef`'s own output part-of-speech
re-enters its input set (`conformance-staging/edge-cases/recursive-endocentric-compounding`) — is
handled by a **depth bound** instead
(`crate::capability::compounding_max_depth`/`crate::emit::build_compound_chain`, unrolling
`max_depth - 1` extra root levels; STAGING.md's "Task 4.1 pieces 2/3 addendum"), which is simpler
and already shipped. SCC condensation would become relevant only if a future construct exhibited
genuine *unbounded* mutual recursion across several distinct rule kinds (not one rule reapplying to
itself) — nothing in the reference or stress corpus looks like that today. *Citation:* Tarjan
(1972), "Depth-First Search and Linear Graph Algorithms," *SIAM Journal on Computing* 1(2).

**C6. Submodular selection.** Greedy algorithms with provable approximation guarantees for
selecting a subset that maximizes a diminishing-returns ("submodular") coverage function.
*Status:* **Dead end / no problem shape identified.** No PanGloss problem currently looks like "pick
k items from a large ground set to maximize marginal coverage" — the closest analog (which gated
features to eliminate first, C4) is small and exact enough that an approximation guarantee buys
nothing. Listed because the seed asked for it; do not build against this without a concrete
motivating case. *Citation:* Nemhauser, Wolsey & Fisher (1978), "An Analysis of Approximations for
Maximizing Submodular Set Functions," *Mathematical Programming* 14(1).

### D. Feature-set representation

**D1. BDDs / ZDDs.** Binary/zero-suppressed decision diagrams: canonical, often-compact
representations of Boolean functions or sets, with efficient set-algebra operations (union,
intersection, subsumption). *Problem shape:* this is the single most promising **not-yet-tried**
technique in the whole catalogue, because it targets a named, permanent architectural sore point
directly. MPR feature sets/groups are represented as plain `Vec<bool>` bitset keys
(`GateGroupSpec::key`, `plan.rs:230-233`), and `MprGroup::Overwrite` is a genuinely **non-monotone**
update (a later rule's output *replaces* rather than unions the accumulated feature state,
`pg_grammar::model::mpr_add_output`'s own doc) — this is why `MprGroupOverwrite` is a **permanent,
unconditional `Refuse`** for a monotone-accumulation admission filter, confirmed in every reference
grammar (`docs/benchmark-matrix.md`: "structurally unsound for a monotone admission filter... the
ADR 0005 override is the only on-ramp"). A BDD/ZDD-backed representation of the *reachable
feature-assignment state space* would let the capability layer answer overlap/subsumption queries
directly instead of the current all-or-nothing verdict — precisely reopening the case ADR 0001
names when it explains why a naive filter is closed by default: "a naive FST filter that silently
omits, e.g. history-dependent `MprGroup::Overwrite`" (ADR 0001, "Confirm-only by default"). *Status:*
**Candidate — the strongest recommendation in this catalogue.** *Citation:* Bryant (1986),
"Graph-Based Algorithms for Boolean Function Manipulation," *IEEE Transactions on Computers* C-35(8)
(BDD); Minato (1993), "Zero-Suppressed BDDs for Set Manipulation in Combinatorial Problems," *30th
ACM/IEEE Design Automation Conference* (ZDD). *Rust reference:* `biodivine-lib-bdd`
(github.com/sybila/biodivine-lib-bdd, pure Rust, used in published formal-methods/model-checking
research from the BioDivine group; verified live on crates.io) and `rsdd`
(github.com/SHoltzen/rsdd, Rust BDD+SDD "knowledge compilation," research-grade but real and
maintained; verified live) are both legitimate starting points — neither is as battle-hardened as
mature C libraries (CUDD, BuDDy), but both are genuine, usable pure-Rust implementations, not
vaporware.

**D2. Bitset / roaring compression.** Compressed representations of large sparse bitsets. *Status:*
**Partially in use, no upgrade needed at current scale.** `GateGroupSpec::key`/
`ReplaceCascadeSpec::group_key` are already plain `Vec<bool>` (`plan.rs:230-233`, `289-293`) — cheap
and correct given `DEFAULT_GROUP_BUDGET = 64` (`compose_budget.rs:100-111`) and today's real gated-
subrule counts (Indonesian 1, Amharic 3). Roaring-style compression would only start mattering if
that budget were raised by an order of magnitude for a real grammar — not observed. *Rust
reference:* `roaring` (pure Rust) and `croaring` (FFI to CRoaring) both verified live and widely
used (33k+ monthly downloads for `croaring`).

**D3. Interval encodings.** Interval trees/skip lists for efficient overlap queries over many
ranges. *Status:* **Dead end / no fit.** Every bound in this codebase (quantifier `max`, compounding
depth) is a single independent scalar checked in isolation — there is no workload of *many
simultaneous overlapping ranges* that would justify an interval-tree structure. Listed because the
seed named it; nothing in this codebase's shape calls for it.

### E. Search and pruning

**E1. Beam search.** Keep only the top-`k` partial candidates during a search, discarding the rest
before they are fully expanded. *Status:* **Not built; distinguish carefully from `ComposeBudget`.**
`compose_budget.rs` is a hard, all-or-nothing resource **cap** (state/arc/tuple/group/line
ceilings, checked "before the expensive part") — it does not rank partial plans and keep the
cheapest-looking `k`; it admits or refuses each candidate independently.
`selection.rs`'s own doc is explicit that today's selector "builds each one via
`build_controllable`... and pick[s] the minimum" — every admissible candidate is built and measured
in full before ranking (`rust/crates/pg-foma/src/selection.rs:1-31`). A genuine beam (prune
low-promise partial plans *before* fully building them) does not exist. *Problem shape:* would
matter once plan enumeration produces many more candidates than today's handful (a second
`ComposeStrategy`, more gate orderings) — anticipated, not yet needed. *Citation:* Lowerre (1976),
*The HARPY Speech Recognition System*, PhD thesis, Carnegie Mellon (origin of beam search); Graefe
(1995)'s own Cascades framework (G1 below) already includes an analogous cost-lower-bound pruning
during its memo search.

**E2. A* / best-first search.** Beam search's admissible-heuristic generalization: expand the
most-promising node first, guided by a cost-to-go estimate that never overestimates. *Status:*
**Candidate**, same family and same non-urgency as E1. *Citation:* Hart, Nilsson & Raphael (1968),
"A Formal Basis for the Heuristic Determination of Minimum Cost Paths," *IEEE Transactions on
Systems Science and Cybernetics* 4(2).

**E3. Magic sets / tabling.** Rewrites a top-down logic/query evaluation to push filters down early
and avoid recomputation, classically for Datalog. *Status:* **Dead end, measured and killed
already.** This is exactly the shape of idea a "pre-filter" census in this repo's own history
already tested and rejected: per the `fst-only-decision-criterion` design record, a pre-filter
replacement was census-killed (merged NO-GO at `571b8a3`) because "validity-gate share [is only]
0–3% of failing time, cascade dead-ends [are] 91–98%" on every grammar measured. The dominant cost
lives *deep inside* confirm's own restricted reparse (a dead-end reached only after real rule
(un)application attempts), not in a shallow validity check a magic-sets-style rewrite could hoist
earlier — so pushing filters down earlier buys almost nothing here, measured, not guessed.
*Citation:* Bancilhon, Maier, Sagiv & Ullman (1986), "Magic Sets and Other Strange Ways to
Implement Logic Programs," *PODS 1986*.

**E4. Memoization.** Cache the result of a pure computation keyed by its inputs. *Status:* **In
use, with a documented near-miss worth citing as a cautionary tale.** In use: `plan.rs`'s
content-addressed `NodeId` interning is exactly memoization of identical plan subtrees — "measured
once, stored once" (`plan.rs:32-37`, `Plan::add_node`'s dedup). Also in use: `pg-rules/src/cache.rs`'s
`RuleCache` memoizes per-rule owning-table resolution (referenced by
`segment-natural-class-table-binding`'s STAGING.md). **Near-miss, explicitly on record:** a
cross-word `(MRuleId, Shape)` synthesize memo was tried during FST optimization and found
**unsound** — a real correctness trap, not merely slow (`[[fst-only-decision-criterion]]` design
record: "(MRuleId,Shape) synthesize memo = UNSOUND trap"). The lesson generalizes: memoizing across
call boundaries in this codebase needs an explicit soundness argument for what varies and what does
not between calls, not just "same inputs, cache it" — plan-node content addressing works because
`NodeId` genuinely captures everything the compiled artifact depends on (design.md D1's soundness
invariant, resolved at task 1.4, `plan.rs:252-293`'s `ReplaceCascadeSpec` doc); the synthesize memo
failed because it didn't. *Citation:* Michie (1968), "'Memo' Functions and Machine Learning,"
*Nature* 218.

### F. Semantics for non-monotone updates

**F1. Flag diacritics.** Multichar symbols (`@P.X.Y@`/`@R.X.Y@`/etc.) that set/test named registers
along a transducer path, used classically to encode long-distance agreement/exception classes
without blowing up automaton size. *Status:* **In use in the vendored engine generally; a
documented, specific dead end for one composition context.** In use: vendored
`foma-0.4.2/src/apply.rs`/`flags.rs` fully implements UNIFY/CLEAR/POSITIVE/NEGATIVE/DISALLOW/REQUIRE
flag semantics, and PanGloss exercises this directly (`tests/f0_viability.rs` F0.3,
`tests/pk2_eliminate_flag_oracle.rs`). **Tried and reverted** for the one place they looked like the
obvious tool: `rust/crates/pg-foma/src/gate.rs`'s module doc records that flags were prototyped for
MPR/POS subrule gating inside a rewrite cascade and hit three independent toolkit defects in this
vendored port before being abandoned as "three surprises deep on one technique" — (1) a flag literal
inside a replace rule's own `||` context corrupts the compiled network (`gate.rs:14-23`); (2)
`fsm_compose` does not treat flags as epsilon-transparent by default, so a flag-bearing net composed
with a flag-free one can silently return **empty** (`gate.rs:24-33`); (3) a Kleene-star flag-gated
workaround built to route around (1) was itself order-fragile (`gate.rs:34-49`). The shipped
replacement is a static, flag-free lexical partition (`crate::gate`, `PlanNodeKind::Gate`). This is
an unusually well-documented "verified not to work here, and exactly why" entry — a genuine dead
end for the specific replace-rule composition context, not a blanket rejection of flag diacritics
(they remain fine, and used, *outside* a `->` construct). *Citation:* Koskenniemi (1983), *Two-Level
Morphology: A General Computational Model for Word-Form Recognition and Production*, University of
Helsinki (origin); Beesley & Karttunen (2003), *Finite State Morphology*, CSLI Publications (the
standard flag-diacritics reference, matching this repo's own rule-exception use case closely).
*Rust reference:* the vendored `foma` crate is the flag-diacritic implementation in hand, with the
composition-context caveat above.

**F2. Belnap four-valued logic / bilattices.** A truth-value lattice with four values (true, false,
both, neither) supporting reasoning about conflicting or incomplete evidence, rather than collapsing
to a binary verdict. *Problem shape:* today's capability disposition per characteristic is
effectively ternary (`Admit`/`ConfirmOnly`/`Refuse`) but not a lattice with meet/join over *partial or
conflicting* evidence — a bilattice could in principle model "feature X is asserted true by rule A
and false by rule B, and only that specific overlap needs confirm" more precisely than today's
all-or-nothing `FailClosed` disposition for `MprGroupOverwrite`. *Status:* **Candidate, conceptual —
a possible companion to D1, not an independent action.** No PanGloss code implements or needs this
today; flag as a design spike only if D1's BDD-based state representation turns out to need a
richer-than-boolean truth model. *Citation:* Belnap (1977), "A Useful Four-Valued Logic," in
*Modern Uses of Multiple-Valued Logic*, Reidel.

**F3. Antichains.** Represent the state of a subset-construction/universality/inclusion computation
implicitly as an antichain (a set of pairwise-incomparable elements under the subset order),
avoiding full explicit determinization. *Status:* **Candidate — and already named as the intended
future direction by this repo's own architecture decision**, which is an unusually strong citation
to have in hand. `docs/adr/0001-honest-capability-boundary.md`'s "Considered and rejected" section
states outright: interaction coverage's "right shape is **tree-structured node/subtree fuzzing**
over the reified compilation plans... and apply **covering-array minimization** over
composition-types (not raw knobs) to cover legal co-occurrences absent from the authored corpus."
Today's `plan_interaction_coverage.rs::retired_interactions` is a **hand-picked, two-entry**
antichain of proven-orthogonal pairs (mpr-append × unordered-application; gate-group sibling
reordering) — a real antichain algorithm over the adjacency-tuple/characteristic-tag structure would
compute the *minimal* required fuzz-case set mechanically instead of by hand, which is exactly what
ADR 0001 asks for and nothing yet builds. *Citation:* De Wulf, Doyen, Henzinger & Raskin (2006),
"Antichains: A New Algorithm for Checking Universality of Finite Automata," *CAV 2006*, LNCS 4144
(verified via search). *Rust reference:* none found for this specific covering-array-over-plan-DAG
use case; the instances here are small enough to hand-roll directly against
`plan_interaction_coverage.rs`'s existing `AdjacencyTuple`/`CharacteristicsProfile` types.

### G. Planning

**G1. Volcano/Cascades cost-based enumeration.** Enumerate legal transformation-equivalent plans,
rank by a cost model, cache the winner; the canonical shape of a modern query optimizer. *Status:*
**In use, by explicit design — this repo's own architecture.**
`docs/adr/0002-cost-based-compilation-planner.md` names the pattern directly: PanGloss "selects among
[compilation plans] like a cost-based query optimizer fused with profile-guided autotuning." The
content-addressed AND-OR DAG (`plan.rs`) is this architecture's "memo" structure by name
(`plan.rs:1-2`: "Volcano/Cascades lineage"); `selection.rs` implements the ADR's v1 objective:
filter by capability, then minimize `states + arcs`, tie-broken by content address
(`selection.rs:1-31`, `choose`, lines 187–216). *Citation:* Graefe & McKenna (1993), "The Volcano
Optimizer Generator: Extensibility and Efficient Search," *ICDE 1993*; Graefe (1995), "The Cascades
Framework for Query Optimization," *IEEE Data Engineering Bulletin* 18(3), pp. 19–29 (verified via
search). *Rust reference:* no generic "Cascades-in-Rust" library exists — this pattern is normally
hand-built per system, exactly as PanGloss has done; `plan.rs`/`selection.rs` are themselves the
in-house Rust reference at this point. Explicitly **not yet built**, per `selection.rs`'s own doc:
"no projected-cost model with error bounds, no committed-plan cache, no profile-guided autotuning" —
parked at the (not-yet-landed) `add-compilation-cost-planner` change, ADR 0002's own next
increment.

**G2. Differential/multi-plan oracle (an addition to the seed list, load-bearing here).** Build
≥2 independently-derived over-approximations of the same grammar and use their *disagreement* as a
designed-in correctness oracle, rather than trusting either one alone. *Status:* **In use.**
`oracle.rs`'s `differential_oracle` (design.md D4) is this exactly: "building >=2
independently-derived over-approximations of one grammar and using their disagreement as a
designed-in correctness oracle" (`rust/crates/pg-foma/src/oracle.rs:1-6`), backed by
`permute_gate_groups`'s second topology generator. This is the same family as N-version
programming/diverse redundancy, applied to compiler correctness instead of runtime fault tolerance.
*Citation:* Avizienis (1985), "The N-Version Approach to Fault-Tolerant Software," *IEEE
Transactions on Software Engineering* SE-11(12) (the general diverse-redundancy argument this
technique specializes). *Practical implication for the skill:* scoring three candidate models
against each other is **also** a free opportunity to run this check between them — if two
capability-passing candidates ever disagree on a word's proposed set, one has a capability-envelope
bug, independent of which one would otherwise win on cost. The skill should require this check as
part of scoring, not as an afterthought.

---

## Part 2 — Dead ends for this shape (consolidated)

| Technique | Why it doesn't fit here |
|---|---|
| Hyper-minimization (A4) | Violates the 100%-recall invariant by definition — it *is* a controlled language change. |
| DAWG / suffix automaton (A7) | No substring-indexing workload exists at any layer this repo owns. |
| Aho–Corasick failure transitions (A9) | No literal multi-pattern raw-text scan exists; composed FSTs over a feature alphabet already generalize this. |
| Graph partitioning / METIS (C1) | Optimizes a cut-size proxy with no correctness meaning; the only sound partitions are dictated by grammar semantics (lexical disjointness), which a generic partitioner cannot know. |
| Submodular selection (C6) | No diminishing-returns subset-selection problem has been identified anywhere in this codebase. |
| Interval encodings (D3) | Every bound here is a single independent scalar; no overlapping-range query workload exists. |
| Magic sets / tabling (E3) | The general shape (push filters down early) was tried as a pre-filter and measured dead: validity-gate share is 0–3% of failing time; 91–98% is cascade dead-ends discovered deep inside confirm, which an earlier filter cannot reach. |
| Flag diacritics for replace-rule subrule gating (F1) | Specifically, not generally: three independent toolkit defects in the vendored port when a flag lives inside a replace rule's own context, fully documented in `gate.rs`. Flags remain in active use elsewhere. |

Not dead ends, but already solved a simpler way — do not rebuild without new evidence:

| Technique | Simpler thing already shipped |
|---|---|
| SCC condensation (C5) | Recursive compounding (the one cyclic construct) uses a depth bound instead. |
| Roaring bitmaps (D2) | Plain `Vec<bool>` is fine under `DEFAULT_GROUP_BUDGET = 64`. |
| ε-removal (A5) | Inherited free from the vendored engine's own compose/minimize pipeline. |

---

## Part 3 — Scoring three candidate models against each other

### Step 0 — the non-negotiable gate (not scored, never traded off)

Every candidate must pass **capability disposition** (`Refuse` excludes a candidate outright,
mirroring `selection.rs`'s own D3 filter-before-rank order:
"an `Admit` candidate and a `ConfirmOnly` candidate are equally admissible... D3 draws the
admissibility line at `Refuse`") **and** empirically measured **recall parity** against the full
HermitCrab oracle over the candidate's target corpus. A cheaper candidate that loses recall is never
in competition with a faithful one — this is not a dimension to weigh, it is a filter to apply first.

- Tool for the disposition half: `capability::characterize` + the per-backend selector
  `backend_selection::select_backends` (`pg-foma::backend_selection`), which `pangloss
  pack`/`make-report` actually enforce; `capability_entry::best_case_across_backends` is the
  separate, advisory-only whole-grammar join `characterization.rs` reads for its health findings, and is
  the wrong tool for a real accept/reject decision (see that function's own doc). **Computable
  today.**
- Tool for the recall half: a `differential_oracle`-style parity check (`oracle.rs`'s own pattern) or
  the corpus-parity-harness pattern `tests/f3_parity.rs` establishes — build or reuse an equivalent
  check for your own candidate set (that specific file is owned by another workstream; do not run or
  modify it directly). **Computable today**, as a pattern to copy, not a shared instrument to reuse
  directly.

### Step 1 — five scalar dimensions

| Dimension | What to measure | Tool today | Gap |
|---|---|---|---|
| **Artifact size** | `(states, arcs, bytes)` triple, kept separate until Step 2 | `PlanMeasure` (`selection.rs`, states+arcs from `build_controllable`'s net); `pangloss fst-health`/`make-report` for `PayloadBytes` (`health.rs`'s R6 decimal-byte bands) | No size measurement exists for the black-box/composite emission path — ADR 0001 itself names this ("the compiled size is unobservable... cost is enumeration-proxy + runtime only") as a reason to migrate constructs onto the controllable path. |
| **Build time** | Wall-clock, in-process, from grammar source to a loadable analyzer | `pangloss make-report`'s own in-process timing (`make_report.rs`'s "Latency methodology"/build-time section) | `pangloss batch`'s TSV does **not** surface `compile_ms`/`grammar_load_ms` even though `pg-cli/src/main.rs:963-1117` computes them internally — named as an open gap in `docs/benchmark-matrix.md`'s own "What would make this table complete" list. `make-report` does not have this gap; use it, not `batch`, for build-time comparisons. |
| **Per-candidate apply cost** | p50/p90/p99, median-of-repeats over a corpus | `rust/tools/typology-speedup.sh` + `typology_speedup.rs`; `make-report`'s own latency section for a single grammar/word-list | None — this is the one dimension with no measurement gap. |
| **Proposer looseness** | See below | `pangloss fst-health <grammar> <words.txt>` (shallow); `cargo run -p pg-foma --release --example deadend_census <grammar>` (deep, attributed) | The deep tool exists but is a manual workflow (`.claude/skills/dead-end-census`), not wired into automatic health output — see Part 4. |
| **Faithfulness/coverage** | Disposition per touched construct + conformance pass/fail + interaction-coverage tuple status | `capability::characterize`/`backend_selection::select_backends`; the conformance suite (`cargo test --workspace`, both `machine/conformance` and `conformance-staging`); `plan_interaction_coverage::compute_interaction_coverage` | The interaction-coverage check only sees candidates expressed as a real `crate::plan::Plan` value — an ad hoc candidate built outside the reified-plan machinery gets **no** automatic interaction-coverage check at all. |

**Proposer looseness, defined concretely** (the metric this catalogue was specifically asked to find
or define): primary measure is **candidates-proposed-per-confirmed-analysis**,
`candidates_generated / confirmed`, over a corpus — the inverse framing of `fst-health`'s existing
`RejectionShare = (candidates_proposed - confirmed) / candidates_proposed`
(`rust/crates/pg-cli/src/fst_health.rs`, `confirmation_work_findings`). Report zero-yield words
(`confirmed == 0`) separately, per the census's own finding that they can dominate overgeneration
cost disproportionately (`docs/fst-plan/foma-fst-plan.md`'s Sena `cinacemerwa`/`cinagumanika`/
`kamatamisa` finding: "these are words where the full engine ALSO finds zero analyses... [so]
overgeneration finds and rejects many false candidates before returning empty"). The deeper,
more *actionable* form of looseness is the dead-end-census's own **d1–d6 attribution plus the
time-share counterfactual under the real batched `confirm_batch`** — it says *where* the looseness
comes from, which is what licenses a specific encoding choice rather than a generic "make it
tighter." Run **both** the shallow ratio and the deep census on **every** candidate, not only the
current baseline: comparing d1–d6 profiles across candidates is what shows whether a candidate
actually tightened the proposer or merely moved the same cost to a different bucket.

### Step 2 — why not one weighted score, and what to do instead

A single invented weighted sum (`0.3 * size + 0.2 * time + ...`) is exactly the "invented, not
citable" shortcut this task was asked to avoid — and this repo has a concrete cautionary tale about
it: the runtime FST precision knob was torn down specifically because it decayed into "an auction"
of ad hoc tradeoffs with no stable meaning across grammars
(`[[fst-precision-knob-spec]]` design record: "no runtime tuning surface, no auction, no presets");
its own measured example is damning on its own terms — "the torn-down knob's `AllFlags` moved
precision 0.0504→0.0506 at 8.4× compile cost." Use **Pareto dominance** instead — a citable,
weight-free way to compare more than one objective. Candidate **X dominates** candidate **Y** iff X
is at least as good as Y on every one of the five scalar dimensions above and strictly better on at
least one (`build time`/`apply cost`/`looseness` lower-is-better; `faithfulness/coverage` treated as
a pass/fail gate already cleared in Step 0, not a scalar to dominate on). Compute the
**non-dominated (Pareto-optimal) set** among the candidates that survive Step 0. *Citation:* Pareto
efficiency is standard in multi-objective optimization; for the modern algorithmic formalization used
here (non-dominated sorting over a small candidate set), see Deb, Pratap, Agarwal & Meyarivan (2002),
"A Fast and Elitist Multiobjective Genetic Algorithm: NSGA-II," *IEEE Transactions on Evolutionary
Computation* 6(2), or Deb (2001), *Multi-Objective Optimization Using Evolutionary Algorithms*,
Wiley, for the general treatment.

If exactly one candidate survives Pareto filtering, that is the answer — subject still to the two
gates below. If more than one survives (the common case: e.g. one candidate is smaller but slower to
build, another proposes tighter but costs more per candidate), the choice among the Pareto set is
made by two further **gates**, not by blending them into the same scalar space:

**Generality gate.** Prefer the Pareto-surviving candidate whose fix addresses a *class* of
grammars over one that helps only the grammar that motivated the change — the user's own explicit
ask. Concretely, computable today by composing existing tools, though no script does this
automatically yet: re-run `capability::characterize` (or the dead-end census's d1–d6 attribution) not
just on the target grammar but across the **full reference-plus-stress-grammar corpus**
(`pg_conformance_fixtures::discover()`'s own corpus), and count how many *other* grammars exhibit
the same `CharacteristicKind`/dead-end class the candidate targets. This is the identical computation
the dead-end census's own per-grammar go/no-go bar already performs for one grammar; generality is
that same computation run corpus-wide and reported as a count or fraction. **Gap:** nothing
aggregates this across the corpus automatically today — it is a small, mechanical loop over code
that already exists, not a new measurement technique. The dead-end-census's own headline lesson is
the concrete cautionary tale for skipping this: "E1–E4 were planned in advance and *missed the class
that actually dominated two of three grammars*" (d5, which had no encoding planned at all) — a
generality claim asserted instead of measured across the corpus is exactly how that happened.

**Regression-risk gate.** Disqualify, or heavily discount, any Pareto-surviving candidate that has
only been checked against the grammar it was built for. Concrete, already-existing tools: (a) the
full conformance suite (`cargo test --workspace`, both roots) — zero new divergences beyond
`known-conformance-divergences.txt`; (b) `plan_interaction_coverage_gate.rs`'s build-breaking
assertion — zero new `Uncovered` adjacency tuples; (c) a differential-oracle-style parity check
against every *other* reference grammar, not only the one under change (build/run the equivalent of
`oracle.rs`'s pattern — do not touch `f3_parity.rs` itself). This gate is not a nicety: this repo has
two concrete, documented incidents proving a change can pass every existing test and still be wrong.
First, the shared-`constructs.txt`-id inheritance defect
(`docs/conformance/shared-construct-id-analysis.md`): `MprGroupOverwrite`'s coverage reported
`Covered` purely by riding its coarser sibling `MprGroupAppend`'s evidence, with the refusal itself
never actually exercised, until a dedicated structural-witness check (G8) closed it — the general
shape of "a change that looks locally fine silently invalidates a coverage claim made elsewhere."
Second, the eleven-site `TableId(0)`-default defect in the confirm engine's rule matching
(`conformance-staging/edge-cases/segment-natural-class-table-binding/STAGING.md`): it was invisible
to the *entire* conformance suite because every existing multi-table fixture built its rules' natural
classes only from the one construct kind (`FeatureNaturalClass`) that happens to be table-agnostic by
construction, and was caught only by a unit test written specifically to exercise the table-dependent
path (`pg-rules/src/cache.rs`'s `owning_table_tests`). Both incidents are the same lesson: the test
suite's own coverage has blind spots correlated with exactly the kind of structural reallocation this
skill asks engineers to consider. The gate is therefore two-part: run the full suite, **and** ask
whether the change reaches a construct/table/interaction combination no existing fixture actually
discriminates — and if so, author a new fixture that would fail under the *old* behavior and pass
under the *new* one (the "discriminating power" proof `segment-natural-class-table-binding` itself
models directly, with a before/after `apply_up` comparison), rather than relying on a green suite that
was never built to notice this class of change.

### Step 3 — the procedure, tied together

1. Enumerate ≥3 candidates, at least one of which reallocates grammar structure into new
   construction steps (not merely a parameter tweak on the existing one) — the skill's own
   requirement.
2. Apply Step 0 (capability + measured recall parity). Disqualify anything that fails it, no
   exceptions, no scoring trade-off.
3. Measure the five Step-1 dimensions for every surviving candidate with the tool table above.
4. Compute the Pareto-optimal subset (Step 2).
5. Break remaining ties with the generality count and the regression-risk gate, including authoring
   a new discriminating fixture where the gate's second question says one is needed.
6. **Record the losing candidates and why**, in the same document/PR as the winner — mirrors ADR
   0002's own governance ("a tuning run *proposes* a plan diff; committing it is an explicit
   human-reviewed action") and `docs/conformance/multitable-shared-representation-design.md`'s own
   worked example of writing down *why the obvious candidate was withdrawn* rather than only
   recording what shipped. A scoring exercise that records only the winner throws away exactly the
   information a future re-scoring (new grammar, new corpus, new trigger) would need to redo the
   comparison instead of re-deriving it from nothing.
7. Pin the result with a conformance fixture that would fail under every losing candidate's
   behavior where that is distinguishable — not merely under the pre-change behavior — per the
   skill's own requirement and the regression-risk gate's discriminating-power standard.

---

## Part 4 — What's automatically detectable today, briefly

Full treatment (including which of the five trigger conditions this bears on) is in
`.claude/skills/fix-a-grammar/NOTES-research.md`. The short version: capability `Refuse` (trigger e)
is the only one of the five triggers with a fully automatic, build-relevant severity signal today
(`characterization.rs`'s `semantic_uncertainty_finding`, always `Critical`). Artifact size (trigger b) has
automatic readiness banding (`health.rs::severity_for_size_bytes`, Ideal through Error). Build time, per-word
apply cost, and proposer looseness (triggers a, d, c) all have **numbers** computed by existing tools
but **no automatic threshold** — `Metric::ElapsedMillis`/`RejectionShare`/`ProposalCandidateCount`
findings are always emitted at a flat `Severity::Info`, never escalated by magnitude
(`health_evaluator.rs`, `fst_health.rs`) — so recognizing that a build is "too slow" or a proposer
"too loose" is a human judgment call against a named target (e.g. the sub-10 ms/word target in
`[[build-for-full-scale-grammars]]`), not something the tooling flags on its own yet.
