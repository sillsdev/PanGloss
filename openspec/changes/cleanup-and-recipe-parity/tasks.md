# Tasks: cleanup-and-recipe-parity

Waves are the dispatch unit: tasks within a wave touch disjoint files and may run in parallel
(max 4 agents); waves are sequential. Every build/test goes through `rust/tools/pg.ps1`.
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
- [ ] 2.2 Cross-compiler equivalence gate: confirmed-multiset agreement + proposal-ratio
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

- [ ] 4.1 Research (subagents, read-only): junction/deletion facts as composed natural-class
      filter rules for the token-cascade path (`structural_allomorph.rs` pattern); exact rule
      inventory per corpus shape; expected proposal reduction on Amharic/Aweti shapes.
- [ ] 4.2 Research (subagents, read-only): plan→emitter seam — strategy-parameter object
      signature, stage boundaries for splitting `emit_with_budget_profiled`, blast radius.
- [ ] 4.3 Implement the higher-leverage of 4.1/4.2 (single owner for `emit.rs`/
      `templated_compile.rs`); oracles: conformance exact baseline + corpus-slice improvement.
- [ ] 4.4 Re-run the dead-end census against the post-routing Sena path; decide E5 go/no-go from
      fresh attribution (not the 2026-07-17 numbers).
- [ ] 4.5 Divvun task 6.1 (flag/replace scoping) can run alongside 4.1/4.2 — it is cheap,
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

- [ ] 6.1 Scope the flag/replace defect (cheap, decisive, FIRST — settles 6.2's design space).
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
