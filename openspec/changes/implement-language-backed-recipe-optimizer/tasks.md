## 1. Contracts and Test Fixtures

- [x] 1.1 Add failing contract tests for registry schema/version validation, stable family ids, applicability, parameter dependencies, and missing materializers in a new `pg-foma::recipe_registry` module.
- [x] 1.2 Add four promoted synthetic grammar fixtures that collectively exercise ordered rules, gate/class partitions, complete-template alternatives, specialized branches, copying, metathesis, and layered morphology; keep actual-language names and data out of fixture identifiers and payloads.
- [x] 1.3 For every promoted fixture that admits alternatives, add failing tests requiring the default plus at least one content-distinct buildable CandidatePlan; require an explicit eliminating-constraint report for single-candidate fixtures.

## 2. Extensible Recipe Registry

- [x] 2.1 Implement schema-versioned registry types, canonical serialization, validation, provenance, and typed materializer registration in `rust/crates/pg-foma/src/recipe_registry.rs`; export them from `lib.rs`.
- [x] 2.2 Seed the seven specified recipe families using existing Plan node kinds and CandidatePlan builders; do not edit semantic owners `replace.rs`, `gate.rs`, or `emit.rs`.
- [x] 2.3 Add unit and snapshot tests proving deterministic serialization, cross-family Plan deduplication by content address, and extension without optimizer-core changes.
- [x] 2.4 Run `cargo test -p pg-foma recipe_registry` and record the focused result before merging this independently reviewable unit.

## 3. Space Characterization and Pruning

- [x] 3.1 Implement grammar-fact extraction, dependency constraints, canonicalization keys, and overflow-safe `N_syntactic`, `N_attested`, and `N_static` accounting in `rust/crates/pg-foma/src/recipe_space.rs`.
- [x] 3.2 Implement exact feasible counting when budget permits and estimated/bounded `N_feasible` with method, sample size, and uncertainty otherwise.
- [x] 3.3 Implement deterministic pilot sampling and measurement of pruning yield plus p50/p95 materialization, capability, build, and evaluation costs.
- [x] 3.4 Add property tests showing every emitted candidate satisfies its hard constraints and every pruning count reconciles with generated, deduplicated, rejected, and retained totals.
- [x] 3.5 Run `cargo test -p pg-foma recipe_space` and review that this merge unit touches no STAGING semantic-owner hotspot.

## 4. Search Strategies

- [x] 4.1 Define the budget, termination, replay seed, candidate-state, and pluggable `SearchStrategy` contracts in `rust/crates/pg-foma/src/recipe_optimizer.rs`.
- [x] 4.2 Implement exhaustive feasible-space enumeration with the `0.5 * remaining_budget` measured-cost admission rule and exact-result labeling.
- [x] 4.3 Implement deterministic constraint-guided diverse beam search with quotas by recipe family and structural signature.
- [x] 4.4 From the four-fixture characterization, choose and document either memoized dynamic programming or constraint programming/branch-and-bound, then implement it as the third V1 strategy.
- [x] 4.5 Implement adaptive strategy selection from space bounds, constraint topology, pilot costs, and budget; keep successive halving disabled unless a checked-in calibration demonstrates predictive fidelity.
- [x] 4.6 Add seeded determinism, budget exhaustion, huge-raw/small-static, weak-pruning, strong-pruning, and misleading-cheap-fidelity tests.
- [x] 4.7 Run `cargo test -p pg-foma recipe_optimizer` and benchmark the three strategies on synthetic small, medium, and pruned-large spaces.

## 5. Evaluation, Confirmation, and Selection

- [x] 5.1 Implement the evaluation ladder by composing existing capability, build, profiling, resource-budget, and differential/full-HC oracle APIs; keep partial results explicitly non-certifying.
- [x] 5.2 Extend selection inputs without weakening `selection.rs` invariants so the default baseline is retained and only buildable, full-HC-agreeing candidates enter the Pareto frontier.
- [x] 5.3 Implement deterministic frontier construction and the specified lexicographic winner policy over states/arcs, build time, apply time, proposals, confirmation work, and NodeId.
- [x] 5.4 Add negative tests for lost analysis identity, changed multiplicity, timeout, truncation, unsupported constructs, and resource breach; assert none can select a winner.
- [x] 5.5 Run the relevant `pg-foma` oracle, selection, coverage-ledger, interaction-gate, hard-supervisor, completeness, and logical-budget tests.

## 6. Reporting and CLI

- [x] 6.1 Implement schema-versioned canonical `RecipeOptimizationReport` JSON and Markdown views in `rust/crates/pg-foma/src/recipe_report.rs`, including full search accounting and explicit exact/approximate status.
- [x] 6.2 Reuse `plan_diagram` to emit baseline and selected Plan JSON/Mermaid from executable Plans and real capability verdicts; add report links and metric deltas.
- [x] 6.3 Add an explicit offline/build-time optimizer command in `rust/crates/pg-cli`; include registry/tool/input hashes, seed, budgets, replay parameters, output paths, and typed nonzero failure modes.
- [x] 6.4 Add golden and round-trip tests proving deterministic reports, truthful unexplored-space statements, and replay equivalence.
- [x] 6.5 Run `cargo test -p pg-foma recipe_report` and focused `cargo test -p pg-cli` command tests.

## 7. Four-Grammar Measurement Run

- [x] 7.1 On a quiet machine, characterize all four promoted grammars and check in raw/static/feasible counts, pruning waterfall, pilot distributions, chosen algorithms, budgets, and environment metadata.
- [x] 7.2 Build and fully confirm at least two distinct recipes per grammar where admissible; record every candidate's analysis-level/multiplicity verdict, size, build/apply cost, proposal count, confirmation work, and termination status.
- [x] 7.3 Compare exhaustive results on tractable reduced fixtures against approximate strategies to measure regret, coverage, and runtime; use the evidence to calibrate pilot size, beam width, reserve fraction, and the third-strategy selector.
- [x] 7.4 Publish the most informative grammar as a detailed Markdown case study with baseline/winner diagrams, Pareto frontier, eliminated alternatives, and a precise non-optimality statement where applicable.
- [x] 7.5 Add a synthetic adversarial interaction fixture combining deletion, reduplication repair, and lexical exceptions, then verify full-HC identity and multiplicity for all surviving recipes.

## 8. Integration and Certification

- [x] 8.1 Run `cargo fmt --all -- --check`, focused clippy for touched crates, and the full relevant Rust test suites without weakening any gate or increasing a budget merely to pass.
- [x] 8.2 Run PanGloss conformance and HC coverage gates for every promoted grammar and confirm the optimizer changes only compilation recipe/cost, never supported-language or recall claims.
- [x] 8.3 Verify normal parsing performs no optimization, saved winners are content-addressed/replayable, and rollback to `enumerate_default` requires no grammar migration.
- [x] 8.4 Run `openspec validate --change implement-language-backed-recipe-optimizer`, review the diff for unrelated files, and obtain code review before merge.
