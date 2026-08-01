# Tasks: cleanup-and-recipe-parity

Waves are the dispatch unit: tasks within a wave touch disjoint files and may run in parallel
within measured machine headroom; waves are sequential. Luna implementation uses medium effort or
higher and Luna research uses xhigh. Every build/test goes through `rust/tools/pg.ps1`.
All work on branch `cleanup-and-recipe-parity` (one worktree), never on `main`.

## 1. Wave 1 — independent fixes (parallel-safe)

- [x] 1.1 Hygiene: delete `ComposeStrategy::Lazy`/`LazyLookahead` (enum, labels in
      `plan_diagram.rs`/`plan_interaction_coverage.rs`, panic guards in `build.rs`/`oracle.rs`,
      doc refs in `enumerate.rs`/`build.rs`); assert-and-document `SearchAccounting.pruned == 0`
      in production; family-id `pub const`s next to `SEEDS` consumed by
      `recipe_optimize.rs` decision sites; route inline zeroed `Score` literals through
      `build_failed`. Files: `plan.rs`, `plan_diagram.rs`, `plan_interaction_coverage.rs`,
      `build.rs` (guard only), `oracle.rs` (guard only), `recipe_registry.rs`,
      `recipe_optimizer.rs`, `recipe_runtime.rs`, `recipe_report.rs`, `pg-cli/recipe_optimize.rs`.
- [x] 1.2 Search efficiency: declared-not-searched tie families with `--search-all-families`
      opt-in and `declared_not_searched` report count; hoist oracle ground truth + exclusion
      latch into a run-scoped cache; lazy emission report computed only in the `PlanComposed`
      arm (kills the surface-probe double emit); score-invariance test on a pinned fixture.
      Files: `recipe_registry.rs` (materialize), `recipe_runtime.rs`, `pg-cli/recipe_optimize.rs`,
      new tests. (Serialize 1.2 after 1.1's registry/report edits merge, or assign both to one
      agent — same files.)
- [x] 1.3 Objective: wire `raw_paths` from `ProposalDiagnostics` → `FomaWordDiagnostics` →
      `Score` (`#[serde(default)]`); key becomes `(confirmation_steps + raw_paths, confirmation,
      proposals, states+arcs, id)`; pinned synthetic Sena-shape preference test + unchanged
      dominant-winner test. Files: `analyzer.rs`, `composite.rs`, `confirm.rs` (plumbing only),
      `recipe_optimizer.rs`, tests.
- [x] 1.4 Routing: template-bearing applicability so `token-cascade-morphology` (or a sibling
      family) offers `TemplatedUnderlyingTokens` to templated phonology-free grammars; gate test
      that a templated fixture is not uflexc-only; conformance suite at exact baseline.
      Files: `recipe_registry.rs` (additive seed/predicate — coordinate with 1.1/1.2 owner),
      `recipe_runtime.rs` dispatch untouched, new gate test.
- [x] 1.5 Docs: superseded header on `large-lexicon-proposal-explosion.md`; historical banner on
      `four-grammar-recipe-evidence-2026-07-28.md`.

## 2. Wave 2 — integration and gates (after wave 1 merges)

- [x] 2.1 Budget banking: child appends per-candidate JSONL progress; supervisor folds completed
      rows into `partial-report.json` on deadline kill; non-certifying semantics pinned by the
      existing timeout test plus a new banked-data assertion. Files:
      `pg-cli/recipe_optimize.rs`, `recipe_optimize_timeout.rs` test.
- [x] 2.2 Cross-compiler equivalence gate: confirmed-multiset agreement + proposal-ratio
      tripwire + non-vacuity, over pinned synthetic fixtures spanning the three pipelines.
      New test file in `pg-foma/tests/`; read-only use of `build.rs`/`emit.rs`/
      `templated_compile.rs` APIs.
- [ ] 2.3 Full managed test pass on the branch (`pg.ps1 -Mode test` + `-Mode corpus-test` where
      corpus inputs exist); fix fallout; conformance at exact baseline.

## 3. Wave 3 — measurement (sequential, release profile, out-of-band)

- [ ] 3.1 Rebuild release; rerun the three corpus slices (indonesian full, amharic 20, sena 5)
      with the new registry/objective; verify: Amharic completes inside 600 s; Sena's winner is
      not dominated; Indonesian winner unchanged; record in the evidence doc.
- [ ] 3.2 Aweti: single-threaded oracle-pathology sweep over the corpus word list; calibrate
      `oracle_step_cap`/`oracle_word_timeout` defaults from the distribution; full-corpus main
      run (foreground, long tool timeout); record certified scope honestly.
- [ ] 3.3 Four-corpus no-dominated-winner check for the D4 key; if violated, apply the
      documented fallback ordering and re-measure; update
      `docs/fst-plan/recipe-parity-plan-2026-07-30.md` scoreboard.

## 4. Round 2 — research then implement (scoped by round-1 results)

- [x] 4.1 Research (subagents, read-only): junction/deletion facts as composed natural-class
      filter rules for the token-cascade path (`structural_allomorph.rs` pattern); exact rule
      inventory per corpus shape; expected proposal reduction on Amharic/Aweti shapes.
      Outcome (2026-08-01): NO-GO for production implementation before the plan→emitter seam.
      Sena has no applicable rules; Amharic has one narrow pure-deletion candidate; Aweti's two
      floating-marker deletions are not evidenced as its dominant proposal source. Preserve the
      supplied synthetic RED-test design as a later spike, not a licensed production change.
- [x] 4.2 Research (subagents, read-only): plan→emitter seam — strategy-parameter object
      signature, stage boundaries for splitting `emit_with_budget_profiled`, blast radius.
      Outcome (2026-08-01): GO for a default-preserving `emit.rs`-only seam. Introduce a small
      surface-emission strategy object containing only derivation and root-scope policy; retain
      the current wrapper/default behavior and existing compile-stage boundaries. NO-GO on new
      searched behavior or corpus-improvement claims in this refactor.
- [ ] 4.3 Implement the higher-leverage of 4.1/4.2 (single owner for `emit.rs`/
      `templated_compile.rs`); oracles: conformance exact baseline + corpus-slice improvement.
- [ ] 4.4 Re-run the dead-end census against the post-routing Sena path; decide E5 go/no-go from
      fresh attribution (not the 2026-07-17 numbers).
- [x] 4.5 Divvun task 6.1 (flag/replace scoping) can run alongside 4.1/4.2 — it is cheap,
      decisive, and touches nothing round-2 owns.

## 5. Round 3 — cleanup and consolidation

- [ ] 5.1 Implement the remaining of 4.1/4.2 if round-2 evidence supports it; otherwise the next
      census-licensed encoding (E5 if licensed by 4.4, or Divvun 6.2/6.3 if 6.1 passed).
- [ ] 5.2 Code-quality pass over everything this change touched (simplify/reuse/altitude), with
      the full managed test suite green.
- [ ] 5.3 Final four-corpus measurement + scoreboard update; reconcile
      `docs/fst-plan/recipe-parity-plan-2026-07-30.md` and this change's evidence; branch ready
      for user-driven merge (rebase + `--ff-only`).

## 6. Divvun-derived proposer-precision experiments (owner-supplied 2026-07-31)

Source: Divvun/Giella research pass (`docs/research/divvun/00`–`17`; read
`ideas-worth-borrowing.md` first once it lands — citations live there). All three branch off
main, keep HermitCrab confirm untouched, buy SPEED (fewer candidates reaching confirm), stay
conformance-gated. Ordered by decisiveness, not value. Out of scope by prior analysis:
Anywhere-mode co-occurrence filters (2^k bound, achieved), non-reachability-provable MPR
Overwrite (4^k), twolc emit/consume, unbounded-copy reduplication.

- [x] 6.1 Scope the flag/replace defect (cheap, decisive, FIRST — settles 6.2's design space).
      Claim under test: `gate.rs:1-20`'s "-> and flags do not mix safely, full stop" is
      over-scoped. Evidence: replace calculus is flag-blind (zero `flag` hits in
      foma-rs rewrite.rs and upstream rewrite.c); apply treats flags as zero-width
      (foma apply.c:1084); collision requires a flag in a MATCHED role inside a `||` context
      (NotContain construction, rewrite.rs:383 gated on `rewrite_contexts.is_some()`).
      Build: compile Divvun's exact idiom under pinned foma-rs 0.4.2 —
      `"@D.Der1.TRUE@" "@D.Der2.TRUE@" … "@P.Der1.TRUE@" "+Der1" <- "+Der1"` (note `<-`, NO
      `||` clause; flags as pure inserted output, never matched; lang-sme runs 1,118 flags in
      production on this shape). PASS: derivation-ordering constraint compiles, rejects
      Der2-before-Der1, accepts ascending. If PASS: narrow gate.rs's module doc to "a flag in
      a MATCHED role" and reopen the flag path for morphotactic legality gating. If FAIL:
      file upstream foma-rs defect with minimal repro (it breaks Divvun's own production
      idiom). Caveat to carry: @P/@R flags are NOT eliminable by foma-rs's flag_build table
      (PK2 finding, precision.rs; Divvun likewise never runs `eliminate flag`) — consumers
      must interpret flags at apply time (foma-rs has crates/foma/src/flags.rs).
      Outcome/evidence (2026-08-01): The repaired managed test preserves the original
      `apply_down` contract: for parsed `A <- B`, A is upper, B is lower, and `apply_down`
      consumes A and emits B; it accepts exactly `+Der1+Der2` and rejects `+Der2+Der1`.
      The original relation's descending `apply_up` consumes visible B and emits A; because A's
      flags are zero-width, its flags-obeyed fail-open output is pinned to the exact
      `BTreeSet` {`+Der2+Der1`}. `fsm_invert` produces the inverse relation `B <- A`;
      its `apply_up` exact ascending/descending sets pass, as does the
      `apply_set_obey_flags(false)` causality control. The earlier uncommitted
      `flag_twosided` observation has no committed construction, managed command, memory
      cap/units, peak source/measurement, or failure-phase provenance; no numeric claim is used
      as evidence and the dangerous probe was not rerun. For foma-rs 0.4.2, `mem.rs` says the
      C globals moved to `FomaOptions`; the default is `flag_is_epsilon: false` at
      `options.rs:83`, consumed at `constructions/products.rs:214`.
- [ ] 6.2 Trigger diacritics for long-range MPR gating (highest value). Divvun idiom: a
      distinguished unused symbol attached to the AFFIX requiring a phonological effect on
      the STEM rides the tape across arbitrary intervening material; a later rule matches it
      in context at distance; a cleanup rule deletes it at cascade end. Use the lang-kal
      ORDERED REPLACE CASCADE precedent (our formalism): `%^GEM`/`%^GEMS`/`%^T` etc.,
      declared phonology.xfscript:49, deleted :50,356-357, worked example :240-249
      (`%^GEMS` several segments right of the gemination site, in the rule's own
      right-context). This is functionally what HC MPR features ARE — the trigger as tape
      symbol instead of engine state; gate.rs's static partition cannot express a gate whose
      trigger is arbitrarily far from its effect without enumerating the in-between. Build:
      ONE HC MPR-gated rule via trigger diacritic instead of static partition. PASS: (a) gate
      fires correctly across intervening material AND (b) compiled net does NOT exceed the
      static-partition baseline in states — (b) is the real test; alphabet size, not lexicon
      size, is where compile cost lives. Report state/arc counts either way (never measured).
- [ ] 6.3 Alpha-variables: enumerate the domain, join disjuncts with `,,` (parallel replace —
      no inter-disjunct ordering hazard), then `.o.` cleanup/placeholder-deletion after.
      Best artifact: lang-crk phonology.xfscript:477-483 (twolc original in comments above
      the live hand-translation; the Cree rule IS bounded reduplication — placeholders d1/d2
      matched against stem-initial consonant then deleted; bounded CV-template reduplication
      without compile-replace, which foma-rs lacks). Status in our code: `replace.rs::
      resolve_alpha_tuples` already enumerates (slots per VarId, cross-product, agreeing
      tuples) but replace.rs is an explicit prototype (only examples/p6_replace_prototype.rs
      calls it); the `,,` join is the missing piece and the PARSER SUPPORTS IT (verified
      2026-07-31, foma-0.4.2/src/regex.rs:624 — each `,,`-separated block becomes one
      rewrite_set node) — an emitter change, not a blocker. Watch the right axis: matched-rule
      domains are 2-10 members (enumeration doesn't bite); it bites when the ALPHABET is
      large (the 417-segment case) — orthogonal, do not conflate.

## 7. Executable subrecipes — generic mechanism foundation

Authoritative reviewed execution plan:
`docs/superpowers/plans/2026-08-01-grammar-compiler-and-recipe-parity.md`. The earlier
`executable-subrecipes-foundation.md` is retained as a superseded design record; its direct
mechanism extractor and separate executable-artifact direction must not be implemented.

- [x] 7.1 Reject every corpus evaluation containing any oracle-capped or oracle-timed-out word;
      a certifiable subset must never be labeled `FullHcConfirmed` (`7bcbafb`, focused oracle gate
      plus 53-test oracle regression set).
- [ ] 7.2 Make D4's Pareto relation deterministic over the componentwise vector
      `(confirmation_steps, raw_paths, confirmation, proposals, states, arcs)`; exclude timing and
      uncertified candidates; recompute and validate serialized frontier/winner decisions.
- [ ] 7.3 Rework the six language-name-free mechanism types: `Morphotactics`, `StaticPartition`,
      `OrderedPhonology`, `StructuralAllomorph`, `CopyProcess`, and terminal `BoundaryCleanup`.
      Nodes own typed semantic requirements/guarantees, edges own dependency/order, and candidate
      bindings own execution disposition. Delete duplicate wire provenance and unproved blanket
      contracts from the initial vocabulary commit.
- [ ] 7.4 Derive mechanism providers only from the shared `GrammarSemantics`; no provider may reread
      `Grammar` to decide applicability. Require typed source references, canonical graph identity,
      and byte-identical fresh-load projection; inert hints may not create mechanisms.
- [ ] 7.5 Make the Registry the sole constructor of a validated `ExecutableCandidate` binding a
      stable semantic digest, portable round-trippable Plan document/digest, exact lowering adapter,
      existing runtime requirements, mechanism graph/bindings, and certification scope. Reject
      bypass/corruption; never use FNV Plan roots as artifact identity or execute an implicit fallback.
- [x] 7.6 Maintain one research dossier per mechanism with scope, invariants, ≥2 language/family
      anchors, chosen/rejected architectures, complexity, evidence log, and explicit
      fits/refines/splits/adds triggers. Contract and six dossiers landed in `a80cae0`; all concrete
      model-ID/counter/cap evidence remains canonically unmeasured and blocks implementation claims.
- [ ] 7.7 Prove the first `Morphotactics → BoundaryCleanup` vertical slice with two independent
      complete-template exercises and two cleanup exercises, exact analysis/root/multiplicity
      parity, cleanup idempotence, and no language-name routing.
- [ ] 7.8 Exercise the remaining orthogonal basis at least twice where possible: template
      order/co-occurrence, cascade/strata, lexical class, allomorph priority, bounded copy,
      unbounded peeled copy, bounded metathesis, interdigitation, feature/POS/MPR gates,
      compounding, and zero morphology. A language may compose any number of mechanisms.
- [ ] 7.9 Run the full managed pg-foma and corpus gates with zero oracle exclusions, then obtain a
      fresh xhigh Sol review before treating the foundation or a wide-reaching mechanism decision
      as settled.
- [ ] 7.10 Re-measure Indonesian, Sena, Amharic, and Aweti at their honest full eligible corpus
      scopes. Record raw/source hashes, deterministic exclusions, all candidates, certification,
      Pareto frontier, and remaining unsupported constructs. Four-language parity is not achieved
      until all four pass these evidence gates; synthetic construct coverage alone is insufficient.
- [ ] 7.11 Introduce one immutable typed `GrammarSemantics::derive(&Grammar)` owner and migrate
      capability, registry applicability, recipe-space accounting, and later mechanism providers to
      projections over it. Delete all other authoritative semantic grammar walkers.
- [ ] 7.12 Define versioned `CorpusSnapshot` and reuse `pg-assess::AnalysisIdentity` v1 set equality
      as the sole public recipe/cross-engine identity. Bind profile, authority, source/model revision,
      semantic digest, options, occurrence order, normalization, exclusions, and oracle completeness;
      retain duplicate/guessed evidence separately, reject supplied roots, and exclude trace parity.
- [ ] 7.13 Add portable Plan serialization/SHA-256 identity, exact adapter lowering, and run-scoped
      lowered-candidate reuse; delete `CandidatePlan`, `EmissionStrategy`, positional baseline state,
      duplicate runtime artifacts, implicit fallback, and fake zero measurements.
- [ ] 7.14 Run the mandatory second Luna/xhigh cleanup audit after 7.11–7.13 and managed package gates,
      but before any mechanism becomes selectable. Resolve every duplicate-owner/claim-level blocker.
- [ ] 7.15 After all six mechanisms have two exercises where possible, run 2–4 orthogonal Luna/xhigh
      reviews and a fresh Sol/xhigh adjudication/implementation pass before four-language certification.
