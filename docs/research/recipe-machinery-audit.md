# Recipe machinery audit — what exists to choose an FST construction, and what actually chooses

Research report, read-only. Worktree: `cleanup-and-recipe-parity`. Scope: `pg-foma/src/{recipe_runtime,
recipe_registry, recipe_mechanism, mechanism_provider, recipe_optimizer, recipe_report, recipe_space,
recipe_accuracy, selection, plan, enumerate, strategy_coverage, lowering_adapter}.rs`,
`pg-cli/src/recipe_optimize.rs`, and the design/status docs that describe them. All line numbers were
read directly or reported by parallel sub-agents that read the named files in full; every claim below is
either a direct citation or explicitly marked "per sub-agent report" with the file:line it gave.

**Headline finding, stated up front because it governs how to read everything else below:** a real
`pangloss parse`/`batch --engine=foma` invocation does not run any of this machinery. It calls
`FomaAnalyzer::new` → `FomaProposer::new` → `emit::emit_with_budget_profiled(g,
PrecisionConfig::Strip, ...)` unconditionally (`pg-cli/src/main.rs:851,1237`; `pg-foma/src/
composite.rs:445-457`; `pg-foma/src/analyzer.rs:253-257,297-373,322-327`). `Registry`,
`evaluate_plans`, `recipe_optimizer`, `EmissionStrategy` selection, and `MechanismGraph` never appear
on that call path. Every one of the four real grammars (Sena, Indonesian, Amharic, Aweti) is served by
that one hardcoded strategy today, regardless of what any offline tool measures for it. This is stated
as an **open, unresolved owner decision** in the repo's own status doc
(`docs/fst-plan/recipe-parity-plan-2026-07-30.md:134-139`): *"Is the recipe optimizer selecting the
future default engine, or a permanently separate offline tuner? … every shipped subcommand uses
`emit.rs`."*

---

## Part A — the recipe/mechanism layer as it actually is

### A1. What abstractions exist, and what each concretely varies

| Abstraction | Defined | Concretely varies in the emitted FST |
|---|---|---|
| `EmissionStrategy` (3 variants: `PlanComposed`, `TunedSurfaceProbed`, `TemplatedUnderlyingTokens`) | `enumerate.rs:355-363` | **Yes, genuinely** — three different compiler entry points: `build::build_controllable` (interprets a `Plan`), `emit::emit` + `FomaProposer::new` (whole-grammar surface probe with synthesized composites), `emit::emit_underlying_templated` + a compiled rewrite cascade (whole-grammar, composite-free). Doc at `enumerate.rs:340-354` states these "reach the same upper tape... which is what makes them comparable... and, before this type existed, they had never been compared, because only one of them was ever offered as a candidate." |
| `LoweringAdapter` (3 variants, 1:1 with `EmissionStrategy`) | `lowering_adapter.rs:12-19` | Same axis as above, named as "which of this crate's compilers lowers a candidate into a network" (module doc, line 1) rather than left as a scattered `match`. `for_strategy`/`strategy` are proven total and mutually inverse by `every_strategy_has_exactly_one_adapter_and_back` (`lowering_adapter.rs:57-79`). |
| `Plan` / `PlanNodeKind` (5 variants: `Leaf`, `Compose`, `Union`, `Gate`, `Replace`) | `plan.rs` | Describes **assembly-tree shape**, not compiler choice. Content-addressed (`NodeId = hash(kind, children, config)`), so identical subtrees dedup by construction. |
| `Registry` / `RecipeFamily` / `Applicability` (9 seeded families) | `recipe_registry.rs` | Applies `SafeTransform` rewrites (`Identity`, `GatePermutation`, `UnionPermutation`, `PartitionBisect`, `PartitionFanOut`) to the baseline `Plan`, or (for 2 of 9 families) swaps the whole-grammar `LoweringAdapter` instead of touching the plan at all. Applicability gated by 8 `Applicability` predicates (`Always`, `HasGatedExceptions`, `HasTemplates`, `HasMorphology`, `HasReduplication`, `HasMetathesis`, `HasMultipleStrata` — unused, see A2 — `HasSplittableGateGroup`, `HasPhonology`, `HasPhonologyOrTemplates`), each a projection of `GrammarSemantics` (`recipe_registry.rs:75-133`). |
| `GrammarSemantics` | `grammar_semantics.rs` | Not a construction-choice type; a single memoized owner of grammar-derived facts (module doc, lines 1-71) that `Applicability`, `recipe_space::GrammarFacts`, and the capability gate all read instead of independently re-walking the grammar. |
| `MechanismGraph` / `MechanismNode` / `MechanismBody` (6 mechanism kinds: `StaticPartition`, `Morphotactics`, `StructuralAllomorph`, `CopyProcess`, `OrderedPhonology`, `BoundaryCleanup`) | `recipe_mechanism.rs` | **Nothing.** Explicitly, by its own module doc (`recipe_mechanism.rs:61-67`): "there is no node, body field, or edge attribute here that names a family, a topology, or a permutation. The order mechanisms compose in is a single canonical spine (`MechanismKind::COMPOSITION_ORDER`), not an axis." `MechanismBinding::derive` (`recipe_mechanism.rs:685-714`) only *classifies* — per mechanism, per the 3 fixed `EmissionStrategy` values — whether that strategy can represent a required construct, via a lookup into `strategy_coverage::representation_of`. It builds nothing. |
| `selection::select_plan` / `choose` | `selection.rs` | A real, working algorithm — filters by `compose_envelope_for_strategy` (excludes `Refuse`), then minimizes measured `states+arcs` of a real `build_controllable` net, tie-broken by content-addressed `NodeId` (`selection.rs:149-246`). Its own module doc (lines 43-47) states: "A library capability, not a production compile path... this module's own existence does not imply that flip happened." |
| `strategy_coverage` (3×22 `(EmissionStrategy, CharacteristicKind)` table) | `strategy_coverage.rs` | A hand-curated coverage table (`Represents`/`RepresentsWithKnownGap`/`CannotRepresent`), exhaustively matched so adding a strategy or a construct is a compile error if a row is missing. Feeds `capability::compose_envelope_for_strategy`, which real capability-gating (`selection.rs`) depends on. It answers "can strategy S represent construct K," never "which strategy should I pick." |
| `recipe_optimizer::Score` / `Certification` | `recipe_optimizer.rs` | The optimizer's ranking dimensions (see A4). |

**The recurring, measured fact underlying this whole table**: plan-shape variation (`Gate`/`Union`
rewrites) is erased by minimization. Three independent citations:

- `enumerate.rs:330-336`: "Measured on eight marker-free synthetic fixtures, varying only [plan]
  shape leaves `states`, `arcs`, `proposals`, and `confirmation` bit-identical across candidates —
  the assembly ends in a minimization step that canonicalizes the difference away... So plan shape
  alone cannot express a better compilation."
- `recipe_registry.rs:879-882`: "on eight marker-free fixtures all of [the plan-rewrite families]
  produced bit-identical states/arcs/proposals and differed only in build time, upward."
- `docs/research/pg-foma-recipe-runtime-design-notes.md:47-50`: "Plan-shape recipes are erased by
  minimization — measured spread 0 across 8 fixtures, with every Indonesian plan-composed
  permutation landing on identical states/arcs and identical proposals."
- `recipe_mechanism.rs:62-64`: "on Sena two different families with two different transforms
  produced bit-identical networks (2044 states / 21114 arcs), and on Indonesian all five
  plan-composed permutations scored bit-identically."

The **only** axis in the whole layer demonstrated to produce a genuinely different compiled network
is which compiler runs at all — `EmissionStrategy`/`LoweringAdapter`, 3 values.

### A2. Reachable from production versus test-only

Below, "production" means reachable from a real CLI subcommand or a library entry point a real
subcommand calls (`pg-cli/src/main.rs`'s dispatch, or `pg-cli/src/recipe_optimize.rs`, which is
itself a shipped-but-offline subcommand — see A3). "Test-only" means every caller found is
`#[cfg(test)]`, `tests/`, `examples/`, or `benches/`.

**IMPLEMENTED AND REACHABLE from the shipped `recipe-optimize` tool** (not from `parse`/`batch` —
see A3 for why that distinction matters):
- `EmissionStrategy` (all 3 variants), `LoweringAdapter` (all 3 variants) — dispatched on in
  `recipe_runtime.rs`'s evaluator (lines ~1727-1735, 2098-2124).
- `Registry::seeded`, `materialize_with_semantics`/`materialize_distinct`, `instances_for_grammar`/
  `instances_for_semantics`/`instances_for_search` — called from `recipe_optimize.rs:407` onward.
- `Plan`/`PlanNodeKind` (all 5 variants), `enumerate_default` — the real baseline-plan builder,
  called by `recipe_optimize.rs:405` and `capability_entry.rs:76`.
- `recipe_optimizer::{Score, Certification, select_confirmed, pareto_frontier, optimize_with_evaluator,
  choose_strategy_with_policy}`, `recipe_report::RecipeOptimizationReport` — the optimizer's own
  ranking core, reached by `recipe_optimize.rs`.
- `strategy_coverage::representation_of` and all 3 `StrategyRepresentation` variants — reached via
  `capability::compose_envelope_for_strategy`, consulted by the real capability-gating path.

**IMPLEMENTED BUT UNREACHABLE FROM PRODUCTION** (built, exercised only by this crate's own tests or
integration tests, never called from `pg-cli` or from any other crate's `src/`):
- `recipe_mechanism.rs`'s entire public surface below the type definitions themselves:
  `MechanismGraph::{validate, bind, refusals, canonical_projection}`, `MechanismBinding::derive`,
  every `MechanismGraphError` variant. Zero calls outside `recipe_mechanism.rs`'s own
  `#[cfg(test)]` and `pg-foma/tests/recipe_mechanism_graph.rs`. (Sub-agent report.)
- `mechanism_provider::derive_mechanism_graph` — the sole production *producer* of a
  `MechanismGraph`. Called only from `pg-foma/tests/morphotactics_boundary_cleanup_slice.rs` and
  `pg-foma/tests/mechanism_provider_gate.rs`. Zero calls from any `src/` file, any binary, or
  `pg-cli`. (Sub-agent report.) Its own module doc says as much: "It does not register anything or
  feed selection... Deriving a graph changes no outcome; it only makes one describable"
  (`mechanism_provider.rs:45-48`, quoted by sub-agent).
- `selection::{select_plan, choose, PlanMeasure, SelectionOutcome, CandidateReport,
  ChosenReport}` — real algorithm (A1), called only from `selection.rs`'s own tests and two `tests/`
  integration files (`grammar_semantics_owner_gate.rs`, `strategy_aware_capability_gate.rs`). Zero
  references anywhere in `recipe_optimizer.rs` or `pg-cli`. (Sub-agent report, independently
  confirmed by a second sub-agent's separate grep.)
- `enumerate::enumerate_candidates` — appends a `"gate-group-permuted"` candidate to the baseline.
  Called only from `enumerate.rs`'s own tests and `selection.rs`'s test module. Production's real
  candidate generator is `Registry::materialize_distinct`, a separate, larger system. (Sub-agent
  report.)
- The entire `recipe_accuracy.rs` module (`AccuracyCounters`, `AccuracyVerdict`, `AccuracyMiss`,
  `check_occurrence`, `verdict_from`, `candidate_admission_key`) and its sole call site
  `recipe_runtime::assess_accuracy_with_cache` (`recipe_runtime.rs:1938`) — that function's *only*
  caller anywhere in the tree is `pg-foma/tests/recipe_accuracy_gate.rs`. `pg-cli/src/
  recipe_optimize.rs` never imports `recipe_accuracy` at all. (Sub-agent report; see A4 for why this
  matters — it is the one mechanism built to separately measure recall/undergeneration, and it is
  disconnected from the shipped tool.)
- `Score::scalar_objective()` (`recipe_optimizer.rs:423-425`) — only caller anywhere is
  `recipe_optimizer_calibration.rs:133`. Production ranks via `Score::key`, never this. (Sub-agent
  report.)
- `NodeId::as_u64` (`plan.rs`) — zero callers anywhere in the repo. (Sub-agent report.)
- `FragmentSpec::GuardAutomaton` (`plan.rs`) — appears in exhaustive match arms
  (`plan_diagram.rs:315,340,520`; `plan_interaction_coverage.rs:193`) but is **never constructed**
  anywhere, production or test. An unconstructable variant. (Sub-agent report.)
- `Provenance::Gate` and `Provenance::Template(TemplateId)` (`plan.rs`) — matched only in
  `plan_diagram.rs:351-352`'s label function; never constructed anywhere. Two more unconstructable
  variants. (Sub-agent report.)
- `Applicability::HasMultipleStrata` (`recipe_registry.rs:40`) — defined and matched in
  `matches_semantics` (`recipe_registry.rs:112`, `stratum_count() > 1`), but no `RecipeFamily` in
  `SEEDS` is gated on it (`FAMILY_LAYERED_MORPHOLOGY`, the one family whose doc used to cite
  multiple strata, is gated on `HasSplittableGateGroup` instead per its own comment at
  `recipe_registry.rs:858-861`: "Applicability moves from `HasMultipleStrata` to
  `HasSplittableGateGroup` because what this transform needs is a splittable partition, not
  multiple strata"). The variant is reachable code (it will fire if ever wired to a family) but
  currently gates nothing — verified directly by reading `SEEDS` (`recipe_registry.rs:810-897`):
  no entry names `Applicability::HasMultipleStrata`.
- `Certification::MultiplicityMismatch` (`recipe_optimizer.rs:286-290`) — doc comment states
  explicitly: "No longer produced (kept for deserializing old reports)... Do not reintroduce a
  producer for this variant." A verified-dead variant, kept for backward wire-compatibility only.
- `CandidateState.exact_objective` — hardcoded to `None` at both production call sites in
  `recipe_optimize.rs` (lines 481, 549); pinned by `pruned_is_structurally_zero_in_
  production_shaped_run` (`recipe_optimizer.rs:1044-1069`). Consequence: `BranchAndBound`'s pruning
  logic can never actually prune in production, so `SearchAccounting.pruned` is structurally always
  `0` even though that strategy is a live, selectable option. (Sub-agent report; independently
  corroborated by `docs/fst-plan/recipe-parity-plan-2026-07-30.md:117-121`, which calls this exact
  fact a "phantom/inert knob... looking like a real signal.")

**TEST-ONLY**: `Plan::contains` (called only from `plan.rs`'s own `#[cfg(test)]`), `Plan::is_empty`
(no located caller outside doc text in the grepped set).

**Already resolved / no longer an issue** (checked directly against a status doc's claim, and the
doc is stale — an example of the ground-rule "trust code over prose where they disagree"):
`docs/fst-plan/recipe-parity-plan-2026-07-30.md:117-118` states "`ComposeStrategy::Lazy`/
`LazyLookahead` are full enum variants... yet nothing can construct them — delete." Reading
`plan.rs:110-113` directly today: `ComposeStrategy` has exactly one variant, `Static`, with a
comment ("Manual discriminant hash... the only strategy in use") confirming the dead variants were
already removed since that doc was written (consistent with the recent `66054ff "cut: delete the
ExecutableCandidate/PortablePlan subsystem"` commit, which describes exactly this kind of cleanup).
The remaining axis — `ComposeStrategy` itself — is real but single-valued: a placeholder for a
distinction (`Static` vs. some future lazy/incremental composition) that has never had a second
member.

### A3. What actually selects a strategy today, end to end, for a real `pangloss analyze` run

**Nothing does. The path is hardcoded, with zero runtime branching on grammar properties.**

There is no `analyze` subcommand; the real single-word entry point is `parse`, and the corpus one is
`batch` (`pg-cli/src/main.rs:151-243` dispatches on `args[1]`; `"parse"` at line 170, `"batch"` at
line 156; both take `--engine=default|foma`, `Engine::parse` at `main.rs:114`).

Trace, `--engine=foma`:
1. `run_parse` (`main.rs:737`) / `run_batch` (`main.rs:1050`) construct
   `FomaAnalyzer::new(&grammar)` (`main.rs:851` and `main.rs:1237`).
2. `FomaAnalyzer::new` (`pg-foma/src/composite.rs:445-457`) calls `FomaProposer::new(g)?`
   unconditionally — no branching, no strategy parameter.
3. `FomaProposer::new` (`pg-foma/src/analyzer.rs:253-257`) → `new_with_budget` →
   `new_with_budget_and_profile` (`analyzer.rs:297-373`), which unconditionally calls
   `emit::emit_with_budget_profiled(g, crate::precision::PrecisionConfig::Strip, enum_budget, ...)`
   (`analyzer.rs:322-327`).
4. The resulting lexc source is parsed and compiled to an `ApplyHandle`.

This is the same code path `recipe_runtime.rs::evaluate_via_tuned_emit_mode` calls under the label
`EmissionStrategy::TunedSurfaceProbed` (`recipe_runtime.rs:1372-1419`, confirmed by direct reading
above) — so production's hardcoded path *is* one of the three named strategies, permanently, with no
mechanism to route to either of the other two. `Registry`, `evaluate_plans`,
`recipe_optimizer`, and `EmissionStrategy`-based dispatch never appear on this call chain (verified
by grep across the whole `rust/` tree). Every other real production/library entry point that builds
an FST proposer — `pg-cli/src/pack.rs:788`, `diagnostics.rs:58`, `make_report.rs:838`, `assess.rs:
533,816,1358`, `fst_health.rs:66`, `pg-wasm/src/lib.rs:187`, `pg-ffi/src/grammar.rs:57` — calls
`FomaAnalyzer::new`/`FomaProposer::new` directly, with no `Registry`/strategy involvement either.

`recipe_mechanism.rs`/`mechanism_provider.rs` (the newer "executable subrecipes" work) are likewise
never referenced from `emit.rs`, `analyzer.rs`, or `composite.rs`.

### A4. What the optimizer measures, how it scores, and whether that is the right objective

`Score` (`recipe_optimizer.rs:314-337`; note: not defined in `recipe_runtime.rs`, only consumed
there): `states, arcs, build, apply, proposals, confirmation, confirmation_steps, raw_paths` — all
`u64`.

`Score::key` (`recipe_optimizer.rs:413-421`), the actual ranking function, used via `min_by_key` in
`select_confirmed` (`recipe_optimizer.rs:887-893`):

```rust
pub fn key(&self, id: &str) -> (u64, u64, u64, u64, String) {
    (
        self.confirmation_steps.saturating_add(self.raw_paths),
        self.confirmation,
        self.proposals,
        self.states.saturating_add(self.arcs),
        id.to_owned(),
    )
}
```

Strict lexicographic order: (1) `confirmation_steps + raw_paths`, (2) `confirmation` call count,
(3) `proposals`, (4) `states + arcs`, (5) id as a deterministic final tiebreak. `build` and `apply`
(wall-clock nanoseconds) never appear in the key — deliberately: the module doc
(`recipe_optimizer.rs:351-412`) cites measured wall-clock noise (15-50% spread on `build`, 6-20% on
`apply` across ten reruns) against zero spread on the deterministic-work fields, and a case where
ranking by time flipped the winner run-to-run on identical input.

**The gate is categorical, never a blend.** `Certification::selectable()` returns `true` only for
`FullHcConfirmed { .. }`. `select_confirmed` and `pareto_frontier` both filter on `selectable()`
*before* comparing `Score::key` at all (`recipe_optimizer.rs:887-911`) — pinned by
`only_full_hc_confirmed_candidates_enter_frontier_or_win` (`recipe_optimizer.rs:1282-1330`, which
exercises all non-confirming `Certification` variants against a tied score and asserts `None`) and
`uncertified_candidate_cannot_dominate_a_certified_frontier_member` (`recipe_optimizer.rs:1761-1797`,
a candidate 10× cheaper but uncertified cannot even join the Pareto frontier next to an expensive
certified one). `Score::key`'s own doc states this directly: "Fewer proposals could mean an
under-generating network. It cannot be selected: only a `selectable()` candidate may win... Work-
minimization operates strictly behind that gate" (quoted from sub-agent's direct reading,
`recipe_optimizer.rs:373-376`).

**So the objective is: cheapest deterministic confirmation work among candidates already proven,
by a full-HC pass over the whole eligible corpus, to confirm every occurrence with zero identity
mismatches.** This is correctness-gate-then-speed, never a weighted tradeoff.

**Is that the objective that matters for proposal quality?** Two gaps, both evidenced in-repo:

1. **No term prices proposal cost on the accepted side beyond `raw_paths`+`proposals`, and this was
   itself found to be a live problem, not hypothetical.** `docs/fst-plan/
   recipe-parity-plan-2026-07-30.md:43-48` (item 3 of "root causes found"): "The ranking key cannot
   see propose-side cost [before `raw_paths` was folded in]... Chunk fusion... absorbs excess
   proposals into shared oracle calls, which is why 575 proposals cost only 42 [confirmation] calls
   and steps look tied on Sena." The same doc records this as an unresolved **owner decision**
   (item 1, line 132): "admit propose-side cost via `raw_paths` and/or promote `states+arcs`... Sena
   flips depending on this choice" — i.e. the ranking's tie-break order is itself contested, not
   settled science.
2. **The one mechanism purpose-built to separately measure recall/undergeneration cheaply
   (`recipe_accuracy.rs` — "assess ACCURACY... with ZERO full-HC confirmation calls," module doc
   lines 1-18) is production-dead** (A2): its sole caller, `assess_accuracy_with_cache`, is called
   only from a test. `recipe_accuracy.rs`'s own doc states the point of the split directly: "any
   attempt to replace confirmation with a cheaper check does not speed the same answer up, it
   redefines what 'best' means. That is why such attempts were rejected" — but having built the
   *complementary* signal and then never wired it into the shipped tool means the shipped
   `recipe-optimize` never actually asks "did this candidate undergenerate," independent of the
   full-corpus confirmation gate.

Net: correctness is gated hard and soundly (a real strength — nothing unconfirmed can ever win).
Past that gate, the ranking measures a proxy for propose+confirm *cost*, not proposal *quality* in
the recall/precision sense, and the repo's own contemporaneous notes (`recipe-parity-plan-2026-07-
30.md`, `recipe_accuracy.rs`) already record this as an open, unresolved gap rather than settled.

---

## Part B — the fit question

### B1. Can the current abstractions express a per-grammar choice of construction technique at all?

**Only at the granularity of "which of 3 whole-strategy compilers runs," never at the granularity of
"this grammar has property P, so use technique T for its templates/its phonology/its copying."**

The registry's applicability predicates (`Applicability`, 8 variants) look, on the surface, like
exactly the "grammar has property P → offered technique" mechanism the question asks about. But
of the 9 seeded families gated by those predicates, 7 vary only `Plan` shape (a `SafeTransform`) and
are proven to collapse to the baseline network after minimization (A1's four citations). Only 2 of
the 9 families vary the actual compiler (`FAMILY_SURFACE_PROBE_MORPHOLOGY` →
`TunedSurfaceEmit`; `FAMILY_TOKEN_CASCADE_MORPHOLOGY` → `TemplatedUnderlyingEmit`, gated on
`HasPhonologyOrTemplates`). So "property P → technique T" already exists today for exactly one real
distinction (template/phonology-bearing → try the templated-token compiler as well as the baseline),
expressed as a menu of size 2 (plus the always-offered baseline), not a general technique-selection
mechanism.

The newer `MechanismGraph`/subrecipe design (`recipe_mechanism.rs`,
`docs/superpowers/specs/2026-08-01-executable-subrecipes-design.md`) was explicitly conceived to go
further — six typed, language-name-free mechanism kinds, each independently owning "chosen
architecture and rejected alternatives" per the dossier template
(`docs/superpowers/specs/2026-08-01-executable-subrecipes-design.md:237-253`). But as built today
(A2), it is a *description and validation* layer: `MechanismGraph::validate` checks structural
well-formedness of a graph someone already built by hand or via `derive_mechanism_graph`;
`MechanismBinding::derive` classifies, per mechanism, whether each of the same 3 fixed
`EmissionStrategy` values can represent it. Nothing in this layer *constructs* an FST differently
depending on what it finds. Its own module doc says so without qualification: "Nothing in this
module ranks or selects" (`recipe_mechanism.rs:46`). The six subrecipe dossiers
(`docs/fst-plan/subrecipes/*.md`) all independently confirm this status in their own "Implementation
status" sections, in near-identical language: "grammar-derived extraction and production
[mechanism] materialization are not claimed complete... Current status: research-ready,
implementation incomplete" (morphotactics.md:190-196, static-partition.md:158-163,
ordered-phonology.md:147-153, structural-allomorph.md:144-151 — the last one narrower still: "a
known correctness gap, not completed coverage"). `boundary-cleanup.md:198-203` and
`copy-process.md:168-173` are the least-incomplete of the six, and even they stop at "repository
evidence exists, unified executable-recipe routing is incomplete" and "implementation still bounded
by the existing confirmation and budget evidence" respectively — neither claims a live
selection path.

### B2. What is the actual axis of variation?

**The compiler (`EmissionStrategy`/`LoweringAdapter`), not the plan.** Restating A1's evidence in
one place because it is the load-bearing fact for the whole "custom-tuned FST per language" question:
eight marker-free synthetic fixtures and the Sena/Indonesian real-grammar comparisons all show
`states`/`arcs`/`proposals`/`confirmation` bit-identical across every plan-shape permutation tested;
only wall-clock `build` time moved, and only upward (`enumerate.rs:330-336`). The mechanism that
explains *why*, structurally: `build_controllable` folds every `Gate` group's network with a
*commutative* `union_checked` and always finishes with `minimize_checked`
(`recipe_registry.rs:668-670`, `oracle.rs`'s own module doc per sub-agent), so group **order**
provably cannot survive minimization, and even group **membership** refinement (`PartitionBisect`/
`PartitionFanOut`, the two genuinely-new families added after the 3-family collapse was first
measured) was found, per `recipe_partition_refinement_gate.rs`'s own module doc, to move build time
only, never the final network shape, on the fixtures actually measured.

So: **if two different recipes produce the same FST after minimization, that is exactly the observed
case for the whole plan-rewrite half of the registry — confirmed, not hypothetical, with named
numbers (2044 states/21114 arcs on Sena; identical scores across five Indonesian permutations).**
The real, surviving axis is a menu of exactly 3 compilers.

### B3. What would have to change

Three concrete gaps, each already named in the repo's own contemporaneous planning docs (cited so
the reader can verify these are the owners' own words, not this report's inference):

1. **Open the plan→emitter seam.** Today the axes that plausibly *would* matter per-grammar — e.g.
   derivation-chain depth, root-eligibility breadth — are hardcoded literals inside `emit.rs`'s
   958-line `emit_with_budget_profiled`, which panics on any `ComposeStrategy` other than `Static`
   (`build.rs:706-712`, `oracle.rs:555-559` per `recipe-parity-plan-2026-07-30.md:114-116`). The
   plan's own item 9 (line 95-99): "Introduce a strategy-parameter object threaded into
   `emit_with_budget_profiled` instead of hardcoded literals... This is what makes future axes
   searchable at all." Nothing currently threads such a parameter; there is no second value to
   choose even if a selector existed.
2. **Wire mechanism-graph *materialization* into an actual compile decision**, not just a
   post-hoc coverage classification. The subrecipe dossiers' own delivery order
   (`docs/superpowers/specs/2026-08-01-executable-subrecipes-design.md:258-268`) puts this at steps
   3-8 ("Route complete template-aware morphotactics through the executable-recipe contract,"
   "Implement real per-stratum pipelines," etc.) — none of which is reported done in any dossier's
   "Implementation status" section (B1). Concretely: `mechanism_provider::derive_mechanism_graph`
   would need a caller in the actual emit/build path that *reads* a node's `ExecutionDisposition`
   and *branches* the compilation, where today it has none (A2).
3. **Resolve the two-pipeline owner decision.** Even if 1 and 2 were both done, nothing today
   connects the optimizer's per-grammar winner to what `parse`/`batch` run
   (`recipe-parity-plan-2026-07-30.md:134-139`, quoted in this report's header). A per-language
   custom-tuned FST cannot exist in production until either the optimizer's output is consumed by
   the shipped emit path, or the optimizer becomes that path.

None of these is a small fix; each is named in-repo as an open, undecided, or explicitly
not-yet-started item, not a wiring oversight.

### B4. Is the mechanism-graph abstraction earning its place?

**Not yet, on the evidence read.** It is real, carefully designed code — `recipe_mechanism.rs`'s own
module doc (lines 4-40) gives a considered argument for why this vocabulary (typed sources, a
mandatory-strategy `MechanismBinding`, no per-edge hand-written contract) is safer than the shapes it
replaced, backed by a concrete measured regression it closes (a candidate 2.2× cheaper than the
winner was `identity-mismatch`ed at runtime despite a hand-written "identity preserved" declaration
that would have validated anyway — line 18-20). That is a genuine design improvement over what
existed before it (the deleted `ExecutableCandidate`/`PortablePlan` subsystem, ~1,978 lines, removed
in commit `66054ff` for having "no production consumer" — direct evidence this repo has cut exactly
this failure mode before).

But today, stripped of its careful typing, what it *does* reduces to describing and validating a
static, always-connected 6-stage pipeline (`COMPOSITION_ORDER`) and looking up, per stage, which of
3 fixed compilers can represent it — a description of the same menu-of-3 finding in B2, not an
extension of it. Its own doc says this outright ("no node, body field, or edge attribute here that
names a family, a topology, or a permutation," "nothing in this module ranks or selects") and every
one of its production entry points (`derive_mechanism_graph`, `MechanismGraph::validate`,
`MechanismBinding::derive`) has zero callers outside its own tests (A2). Until steps 3-8 of its own
design doc's delivery order are built — i.e. until *materializing* a mechanism graph actually changes
what gets compiled, rather than only describing what was already compiled by the unrelated
`Registry`/`enumerate_default` path — it is indirection over the same 3-compiler menu, with better
vocabulary for describing that menu's coverage gaps than the code it replaced had.

---

## Dead-or-unreachable inventory

Every item below has zero production callers (only `#[cfg(test)]`, `tests/`, `examples/`, or
`benches/`), or is a declared enum variant nothing anywhere can construct. "Own tests" means the
callers found are entirely within the same file's `#[cfg(test)]` module and/or that crate's
`tests/` directory.

| Item | File | Status | Only callers/producers found |
|---|---|---|---|
| `NodeId::as_u64` | `plan.rs` | Dead | None anywhere in the repo |
| `FragmentSpec::GuardAutomaton` | `plan.rs` | Unconstructable variant | Matched in `plan_diagram.rs:315,340,520`, `plan_interaction_coverage.rs:193`; never constructed |
| `Provenance::Gate` | `plan.rs` | Unconstructable variant | Matched only in `plan_diagram.rs:351-352`'s label fn |
| `Provenance::Template(TemplateId)` | `plan.rs` | Unconstructable variant | Same as above |
| `Plan::contains` | `plan.rs` | Test-only | `plan.rs`'s own `#[cfg(test)]` (line 520) |
| `Plan::is_empty` | `plan.rs` | Test-only / unreached in grepped set | none located outside doc text |
| `enumerate::enumerate_candidates` | `enumerate.rs` | Implemented, unreachable from production | `enumerate.rs` own tests, `selection.rs` test module |
| `selection::select_plan`, `choose`, `PlanMeasure`, `SelectionOutcome`, `CandidateReport` (selection.rs's own, distinct from `recipe_report::CandidateReport`) | `selection.rs` | Real algorithm, unreachable from production | `selection.rs` own tests; `tests/grammar_semantics_owner_gate.rs`; `tests/strategy_aware_capability_gate.rs` |
| `MechanismGraph::{validate, bind, refusals, canonical_projection}` | `recipe_mechanism.rs` | Implemented, unreachable from production | own tests; `tests/recipe_mechanism_graph.rs` |
| `MechanismBinding::derive` and all `MechanismGraphError` variants | `recipe_mechanism.rs` | Implemented, unreachable from production | same as above |
| `mechanism_provider::derive_mechanism_graph` | `mechanism_provider.rs` | Implemented, unreachable from production | `tests/morphotactics_boundary_cleanup_slice.rs`, `tests/mechanism_provider_gate.rs` |
| Entire `recipe_accuracy.rs` module (`AccuracyCounters`, `AccuracyVerdict`, `AccuracyMiss`, `check_occurrence`, `verdict_from`, `candidate_admission_key`) | `recipe_accuracy.rs` | Implemented, unreachable from production | own tests; `assess_accuracy_with_cache`'s sole caller is `tests/recipe_accuracy_gate.rs` |
| `recipe_runtime::assess_accuracy_with_cache` | `recipe_runtime.rs` | Implemented, unreachable from production | `tests/recipe_accuracy_gate.rs` only |
| `Score::scalar_objective()` | `recipe_optimizer.rs` | Dead in production ranking | `recipe_optimizer_calibration.rs:133` only |
| `CandidateState.exact_objective` | `recipe_optimizer.rs` | Structurally always `None` in production | hardcoded `None` at both production call sites, `recipe_optimize.rs:481,549` |
| `Certification::MultiplicityMismatch` | `recipe_optimizer.rs` | Verified dead by its own doc comment | "No longer produced... Do not reintroduce a producer for this variant" |
| `Applicability::HasMultipleStrata` | `recipe_registry.rs` | Reachable code, gates no family | Defined and matched; no `SEEDS` entry names it |
| `ComposeStrategy` | `plan.rs` | Single-variant placeholder axis | Only `Static` exists; a status doc's claim of `Lazy`/`LazyLookahead` dead variants is stale — already deleted |

---

## Ground-truth note on method

This report was assembled from four parallel sub-agent reads (each instructed to read named files in
full and grep the whole `rust/` tree, excluding `#[cfg(test)]`/`tests/`/`examples/`/`benches/`, for
every `pub` item and enum variant) plus direct reading of `recipe_runtime.rs`, `recipe_registry.rs`,
`grammar_semantics.rs`, `recipe_mechanism.rs`'s module doc and `MechanismGraph` definitions,
`lowering_adapter.rs`, `enumerate.rs`'s `EmissionStrategy`, `plan.rs`'s `ComposeStrategy`, the two
design-notes docs, the executable-subrecipes design spec, all six subrecipe dossiers'
"Implementation status" sections, and `docs/fst-plan/recipe-parity-plan-2026-07-30.md` in full. Every
sub-agent finding used above was either independently corroborated by a second sub-agent's separate
grep, by this report's own direct reading, or is attributed as "sub-agent report" where it could not
be independently re-verified within this task's time budget. One stale claim in a status doc
(`ComposeStrategy::Lazy`/`LazyLookahead`) was caught by direct re-reading and is flagged above rather
than repeated.
