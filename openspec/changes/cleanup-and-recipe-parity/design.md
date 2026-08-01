# Design: cleanup-and-recipe-parity

## Context

Evidence base: `docs/fst-plan/recipe-parity-plan-2026-07-30.md` (seven-agent research pass,
2026-07-30). Four-corpus state: Indonesian — recipe beats hand-spun on every metric; Sena — the
plan-composed candidate comes from `uflexc` (self-looping affix chains, documented as
non-generalizing to templated grammars) and the template-aware candidate was never offered
(`HasPhonology` gate, Sena has zero prules); Amharic — 600 s budget exhaustion is mostly
grammar-invariant work recomputed per candidate plus four provably-tying permutation candidates;
Aweti — first certified candidate exists but only over a 6-word pilot slice.

Constraints from STAGING.md: `emit.rs` and `replace.rs`/`gate.rs` are single-owner hotspots;
synthetic-only fixtures; never weaken recall/budget/supervisor gates; corpus measurements stay
out-of-band (gitignored data, managed `pg.ps1` entry points only).

Execution model for this change: all work happens on one worktree branch off `main`
(`cleanup-and-recipe-parity`), implemented by Luna subagents at medium effort or higher (xhigh for
research; concurrency bounded by measured headroom and `Enter-BuildSlot`), reviewed
change-by-change by the coordinating session against the oracles:
conformance suite, the four corpus slices, hand-spun results, and Rust code-quality judgment.
Three rounds: (1) the items below; (2)–(3) research-then-implement follow-ons (junction filter
rules, plan→emitter seam, E5) gated on round-1 measurements.

## Goals / Non-Goals

**Goals**
- Templated grammars get a template-aware underlying candidate; `uflexc` stops being their only
  underlying model.
- A 7-candidate Amharic-scale run completes inside the budget by not repeating grammar-invariant
  work and not searching provable ties.
- The ranking key prices propose-side work; winner selection is validated on all four corpora.
- A cross-pipeline containment gate catches emitter mis-routing as a failing test.
- The hygiene items land (Lazy removal, pruned-field honesty, family-id constants, failure-score
  constructor, doc banners).
- Aweti certification extends toward the full corpus with calibrated oracle caps.

**Non-Goals (round 1)**
- No changes to `replace.rs`/`gate.rs` semantics, no MPR-overwrite construction, no E5 encoding,
  no `emit_with_budget_profiled` split — these are round-2/3 candidates behind fresh
  measurement.
- No new plan node kinds; the Plan language stays closed.
- No wall-clock terms in the ranking key.
- Not deciding the two-pipeline ship question (owner decision, recorded as open).

## Decisions

**D1 — Routing via registry applicability, not build_controllable surgery.** Add a
template-bearing predicate (`Applicability::HasTemplatedMorphotactics` or widening
`token-cascade-morphology`'s gate to `HasPhonology ∨ HasTemplates`) so the existing
`TemplatedUnderlyingTokens` dispatch in `recipe_runtime.rs` serves templated grammars. Rejected
alternative: teaching `build_controllable` to consume `emit.rs`'s template-aware structural
functions — that is the round-3 seam refactor, far larger, and unnecessary to close the Sena gap
because the whole-grammar strategy dispatch already exists and is measured working
(`mpr-gated-exception` win, Aweti candidate).

**D2 — Tie families: skip at materialization, keep declaration and evidence.** The registry keeps
the four permutation families in `SEEDS` with their recorded tie evidence; `materialize_distinct`
(or the CLI materialize loop) skips them unless an explicit `--search-all-families` opt-in is
passed. Report gains a `declared_not_searched` count so the ledger stays honest. Rejected:
post-minimize network-hash dedup — it still pays full build cost per permutation before
discovering the tie; acceptable later as defense-in-depth, not as the primary fix.

**D3 — Hoist grammar-invariant work into the evaluator, not a batch refactor.** Keep the
candidate-at-a-time evaluation loop (budget checks between candidates depend on it). Give
`Evaluator` lazy, run-scoped caches for (a) oracle ground truth + the corpus-wide exclusion
latch, (b) the whole-grammar emission report, computed only when a `PlanComposed` candidate
needs it (move the `emit(grammar)` call inside the `PlanComposed` arm). Rejected: restoring
multi-plan batching in `evaluate_plans_marked` — bigger surface, defeats per-candidate budget
decisions, and D3 captures the same savings.

**D4 — Objective: leading term becomes `confirmation_steps + raw_paths`.** Both are
deterministic counts of "one unit of adjudication work" (one HC step; one raw proposer path
decoded). Key becomes `(confirmation_steps + raw_paths, confirmation, proposals, states+arcs,
id)`. On measured shapes: Indonesian's dominant winner is unchanged; Sena flips to the
lower-total-work candidate. Wire `raw_paths` from `ProposalDiagnostics` through
`FomaWordDiagnostics` into `Score` with `#[serde(default)]`. Validation is the four-corpus
no-dominated-winner oracle from the spec, run before the key change is accepted onto the branch.
Rejected alternatives: promoting `states+arcs` (a size proxy, not work; punishes legitimately
large precise networks like hand-spun Sena — the opposite of intent); proposals-first (punishes
recall-required breadth; post-dedup count undercounts traversal). Risk acknowledged: unit
commensurability between HC steps and raw paths is asserted 1:1 — if a corpus shows the sum
mis-ranking, fall back to keeping steps-first and adding `raw_paths` as term 2, and record the
measurement either way.

**D5 — Equivalence gate compares confirmed sets and containment, not networks.** Byte-level
network equality across pipelines is false by design (different symbol spaces: literal
orthography vs PUA tokens). The stable observables are: proposal multisets decoded to tag
sequences on a fixed word list, and oracle-confirmed analysis multisets. Gate asserts confirmed
multiset equality with the oracle per pipeline, plus a proposal-ratio tripwire (a pipeline
proposing > K× the oracle-confirmed count on a pinned templated fixture fails with the ratio in
the message; K chosen from the measured uflexc blowout with headroom). Non-vacuity asserted.

**D6 — Budget banking via supervisor-readable progress.** The child process appends each
completed candidate evaluation to an incremental JSONL progress file; on deadline kill, the
supervisor folds completed rows into `partial-report.json` (still `budget-exhausted`,
`winner: null`, exit non-zero). Rejected: in-process graceful shutdown — the child is hard-killed
by design and mid-candidate preemption is exactly what the current architecture avoids.

**D7 — Hygiene is mechanical and test-pinned.** Lazy variants deleted workspace-wide (enum, two
label renderers, two panic guards, doc references); `pruned` gets a production assertion +
schema doc note (wiring a real bound is deferred — no admissible estimate exists yet);
family ids become `pub const` items next to `SEEDS` used by the CLI decision sites; failure
`Score`s route through `build_failed`.

**D8 — Aweti breadth as measurement first.** Sweep the corpus for oracle-pathological words
(bounded `Morpher` with the existing caps, single-threaded, word-timeout per repo hazard doc),
calibrate `oracle_step_cap`/`oracle_word_timeout` from the observed distribution, then run the
main loop (not the pilot) over the full word list. Deliverable is evidence + calibrated defaults,
not a certification claim beyond what `measure_and_certify` states.

**D9 — Recipes compose typed linguistic mechanisms, not language labels.** Preserve the closed
`Plan` relation algebra and its compilers as the physical layer. Above it, extract a validated
grammar-derived mechanism graph with six deep modules: `Morphotactics`, `StaticPartition`,
`OrderedPhonology`, `StructuralAllomorph`, `CopyProcess`, and terminal `BoundaryCleanup`. A grammar
may use any subset and multiple instances. Edges carry explicit provided/required contracts for
symbol space plus active table, identity, multiplicity, dynamic state, stratum, copy bounds, and
execution disposition. Incompatibility is typed and fail-closed; compiler strategy is a lowering
adapter, never a linguistic family. Productive unbounded copying is peeled rather than falsely
claimed as an ordinary one-way FST. Each mechanism owns a living research dossier and at least two
independent conformance exercises where possible. The four target languages remain integration and
scale gates, not proof of genericity. Rejected: adding more registry labels whose implementation is
only identity/permutation, branching on language/fixture names, or treating a surviving certified
corpus subset as full parity.

## Risks / Trade-offs

- [Sum key mis-weights units] → four-corpus no-dominated-winner validation before accept;
  documented fallback ordering; the synthetic Sena-shape fixture pins intent.
- [Skipping tie families under-explores a non-canonicalizing grammar] → skip is gated on the
  compositional-topology signal the registry already computes; opt-in flag restores full search;
  declared-not-searched count keeps the omission visible.
- [Caching changes observable scores] → spec requires exact score invariance on a pinned fixture;
  the latch (capped-oracle exclusion) is computed once and shared, which is the current semantics
  already.
- [Routing widens candidate sets and slows runs] → it adds at most one whole-grammar candidate
  per templated grammar while D2/D3 remove four candidates' redundant cost; net strongly negative
  on wall-clock.
- [Parallel agents collide on hotspot files] → wave plan assigns disjoint file sets; `emit.rs`
  and `templated_compile.rs` have a single owner per wave; all builds through `pg.ps1`
  (memory spawn-gate, job objects, build slots) so four agents cannot OOM the machine.
- [Aweti full-corpus run is long] → run in a foreground call with a generous tool timeout
  (never a polled background job, per repo rules), on the release profile, single run.

## Migration Plan

All work lands on branch `cleanup-and-recipe-parity` in a dedicated worktree; `main` is
untouched until the user merges (rebase + `--ff-only` per repo policy). Report-schema additions
are `#[serde(default)]`; the CLI gains only additive flags (`--search-all-families`). Rollback
is dropping the branch. Winner flips caused by D4 are expected and recorded as observations in
the evidence doc.

## Open Questions

- Two-pipeline ship question (is `recipe-optimize` selecting a future default engine?) — owner
  decision, does not block this change.
- Whether `pruned` ever gets a real admissible bound — deferred until a cost model exists
  (`add-compilation-cost-planner` is the parked owner).
- Exact K for the proposal-ratio tripwire — set from round-1 measurements with headroom.
