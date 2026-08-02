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
- [x] 7.2 Make D4's Pareto relation deterministic over the componentwise vector
      `(confirmation_steps, raw_paths, confirmation, proposals, states, arcs)`; exclude timing and
      uncertified candidates; recompute and validate serialized frontier/winner decisions.
      Deterministic frontier/report validation landed in `d619999`; `2e8b07d` additionally prevents
      a selectable confirmed candidate from being hidden by deleting its score (12 report tests).
### Wave 3 evidence that scopes 7.3–7.8 (measured 2026-08-01/02, full corpora)

Read this before touching the mechanism vocabulary. Full record:
`C:\tmp\pangloss-wave3-results-2026-08-01.md`.

1. **Plan-shape permutation varies nothing — now confirmed on a real corpus, not just fixtures.**
   On Sena's 250-word probe, `ordered-morphophonology|topology=baseline` and
   `specialized-branch|topology=partition-bisect` — two *different* families with two *different*
   transforms — produced **bit-identical** networks and proposal behaviour: 2044 states, 21114 arcs,
   14,826,003 proposals, 16,831,797 raw paths. This is the eight-fixture minimization finding
   reproduced at corpus scale. Any mechanism vocabulary that treats plan shape as a varying axis is
   modelling something that does not exist.
2. **The compiler (`EmissionStrategy`) is the decisive axis, across three languages.** Different
   grammars genuinely want different compilers — that, not plan topology, is what a mechanism graph
   must be able to express and select between.
   **AMENDED 2026-08-02, and the amendment is itself the lesson.** This fact originally read
   "Indonesian's winner is `plan-composed`". That is now DEAD: once the compound loop (`97d0ef7`)
   let `plan-composed` emit the compound paths it was structurally incapable of, its Indonesian
   network grew 3.9x (693 -> 2683 states+arcs) and it lost the tiebreak. **Its win had been bought
   by a recall bug** — the network only looked small because it could not represent compounds.
   Current winners: Indonesian -> `templated-underlying-tokens`, Amharic -> `tuned-surface-probed`,
   and **`plan-composed` wins nowhere**. The axis survives — two whole-grammar compilers still win
   two languages — but note the rival reading stays live until task 38 closes: "one compiler is best
   and the split is itself a defect artifact." Do not build a vocabulary that hard-codes any
   particular winner.
3. **A cheaper candidate can be a wrong candidate.** Amharic's
   `@templated-underlying-tokens` was ~2.2x cheaper than the winner and `identity-mismatch`ed. Any
   candidate ranking that is not gated on the parity relation will prefer the fast wrong answer.
4. **Recall gap to root-cause first — FIXED 2026-08-02 by `97d0ef7`, kept here because the episode
   is the canonical worked example.** On Sena `ndimwe`, `plan-composed` under-generated: oracle 8
   distinct identities, candidate 6, `0 candidate-only`, the two missing differing only in
   `root_index`. Root cause: `uflexc`'s continuation graph had no arc back to the root class, so it
   was structurally single-root and could propose NO compound at all. Pinned by RED-1 (`a1736a8`,
   un-ignored at `a7572ae`) and RED-2 (`e98c488`, the sharper root_index-only fixture).
   The durable lesson, and the thing a mechanism node's guarantee type must be able to express:
   `Disposition::ConfirmOnly` is defined as "recall-preserving ONLY IF the proposer proposes the
   superset" — a **per-proposer fact, never a grammar fact**. Treating it as a grammar fact is
   precisely how a whole-construct hole survived in a compiler holding a certification. A guarantee
   that cannot name *whose* guarantee it is, is this bug.
   Still open, same shape, same file: `uflexc` cannot propose `RealizationalMorphology` at all
   (task 33) — accounted for as the only `CannotRepresent` row, not fixed.

**Scoping consequence.** 7.3–7.5 must be re-grounded on what measurement says varies (which compiler,
and the construct-dependency facts that decide whether a compiler can represent a grammar faithfully)
rather than on plan-shape families. 7.8's "exercise the orthogonal basis" is still right, but an
exercise only counts if it varies a mechanism that demonstrably changes an outcome — a family label
over an erased transform is not an exercise.

- [ ] 7.3 Rework the six language-name-free mechanism types: `Morphotactics`, `StaticPartition`,
      `OrderedPhonology`, `StructuralAllomorph`, `CopyProcess`, and terminal `BoundaryCleanup`.
      Nodes own typed semantic requirements/guarantees, edges own dependency/order, and candidate
      bindings own execution disposition. Delete duplicate wire provenance and unproved blanket
      contracts from the initial vocabulary commit. **Re-grounded by the Wave 3 evidence above:** the
      typed requirements/guarantees must be the ones that decide compiler admissibility and recall,
      and no node may exist solely to name a plan-shape permutation.
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
      Slice note (2026-08-02): `pg_foma::grammar_semantics::GrammarSemantics` now exists and owns
      `prules_in_order`, the gated-subrule set, the entry partition (deterministically ordered),
      the existence/cardinality facts, and -- memoized -- `capability::characterize`'s profile.
      Migrated: `capability::compose_envelope`, `capability_entry::evaluate_capability`,
      `preflight::preflight_findings`, `selection::select_plan`, `readiness_verdict::certify`,
      `recipe_registry::Applicability` + every `Registry` instance/materialize entry point,
      `recipe_space::{GrammarFacts, characterize}`, `junctions::PhonologyProbe`'s existence gate,
      `plan_interaction_coverage`'s assembly glue (its local `prules_in_order` copy is deleted),
      `plan_diagram::build_plan_document{,_for_plan}`, and `pangloss make-report`/`pack`/
      `recipe-optimize`. Measured: one `make-report` invocation on the refused path characterizes
      **once, down from five** (its preamble, `certify`, and THREE inside one
      `build_plan_document` -- that function alone ran `plan_and_profile` twice and
      `compose_envelope` once, discarding one of the two plans); `select_plan` characterizes once
      instead of once per candidate plan. Explicitly NOT done, each for a stated reason:
      `conformance_coverage.rs`'s four `grammar_has_*` witnesses stay an independent second
      derivation (`tests/structural_witness_gate.rs` exists to exploit that independence);
      `capability::characterize`'s own 7500-line internals stay as-is (the owner owns its RESULT --
      making it consume the owner would be circular, and the only sub-facts it shares already route
      through one authority); `gate::compile_gated_grammar_*` and `emit.rs`'s own
      `compound_chain_depth_and_budget_check` stay `&Grammar`-parameterized compile paths; and
      `recipe_optimize.rs`'s three `compose_envelope` calls are deliberately left re-deriving,
      because its `StageMeasurement::capability` samples that stage's wall time and a shared
      memoized profile would render every pilot sample as a near-zero measurement of a stage that
      really does work once. `e2_infix_probe.rs` was NOT deleted: `docs/superpowers/specs/
      2026-07-17-better-proposing-fst-plan.md` lists E2 as "BUILD after E5", so it is a parked
      build-ready probe, not dead code.
- [ ] 7.12 Define versioned `CorpusSnapshot` and reuse `pg-assess::AnalysisIdentity` v1 set equality
      as the sole public recipe/cross-engine identity. Bind profile, authority, source/model revision,
      semantic digest, options, occurrence order, normalization, exclusions, and oracle completeness;
      retain duplicate/guessed evidence separately, reject supplied roots, and exclude trace parity.
      Slice note (2026-08-01): the recipe runtime now carries transitional requested/included/
      excluded counts, deterministic word-list hashes, and per-row exclusion reasons on its existing
      truncated outcome. This does not implement or mark complete the versioned snapshot/scope and
      identity migration; that remains the follow-up owner for this task.
      Slice note (2026-08-01, second): the recipe runtime's COMPARISON is now `AnalysisIdentity` v1
      deduplicated set equality per occurrence, replacing full `WordAnalysis` structural equality and
      vector multiplicity (`pg-foma::parity`, `recipe_runtime::certify_word`/`certify_corpus`). The
      projector moved to `pg-parse` (`pg_parse::identity`) and `pg-assess` re-exports it, so it is one
      shared definition with no `pg-foma -> pg-assess` dependency. Duplicate paths and guessed/
      supplied annotations are retained as separate typed evidence
      (`parity::IdentityEvidence`, `WordEvidence::{expected,actual}_identities`), supplied roots and
      guessing are refused as typed non-selectable faults, and a projection failure is a typed fault
      rather than a mismatch. Still NOT done, and still owned here: the versioned `CorpusSnapshot`/
      `CertificationScope` schema binding profile, authority, source/model revision, semantic digest,
      options, and normalization; the transitional `CorpusCompletenessEvidence` remains in place
      unchanged.
- [ ] 7.13 Add portable Plan serialization/SHA-256 identity, exact adapter lowering, and run-scoped
      lowered-candidate reuse; delete `CandidatePlan`, positional baseline state, duplicate runtime
      artifacts, implicit fallback, and fake zero measurements.
      **AMENDED 2026-08-02 — this task used to order the deletion of `EmissionStrategy`, which is the
      one axis Wave 3 proved decisive** (see the evidence block before 7.3, three sections above).
      Instead: replace `EmissionStrategy` with a typed adapter identity on `ExecutableCandidate`
      that PRESERVES its role as the selection axis, and delete the enum only once the adapter axis
      expresses everything the enum selected between. If Fable checkpoint 2 weakens the
      compiler-axis thesis, revisit this wording again rather than treating it as settled.
- [ ] 7.14 Run the mandatory second Luna/xhigh cleanup audit after 7.11–7.13 and managed package gates,
      but before any mechanism becomes selectable. Resolve every duplicate-owner/claim-level blocker.
- [ ] 7.15 After all six mechanisms have two exercises where possible, run 2–4 orthogonal Luna/xhigh
      reviews and a fresh Sol/xhigh adjudication/implementation pass before four-language certification.

## 8. Sequencing and scheduled reflective reviews (added 2026-08-02)

### Re-entry trigger for the deferred architecture program

7.3–7.8 were moved OFF the parity critical path on 2026-08-01 after a meta review found the
architecture program had been inserted in front of measurements that were three tasks away. That
deferral is sound but it needs a re-entry condition, because unbounded deferral with no trigger is
how the *previous* program failed, in mirror image.

**7.3–7.8 resume when tasks 20–22 close** (deterministic eligibility; per-candidate apply/proposal
budget; the plan-composed root-position defect). **7.12 goes next after that**, because it consumes
both task 20's eligibility semantics and the salvaged minimal `ModelRevision`.

Scope cap while deferred: tasks 20–22 are a classification change plus two evidence fields, one
budget knob plus a typed verdict, and a bug hunt. If any of them starts growing new provenance
schema beyond that, the old failure mode has returned and the answer is no.

### Scheduled Fable reflective checkpoints

Reserved-tier reviews are normally escalations, which means they only fire once something has
already gone wrong — a program drifting *quietly* never trips one. These three are scheduled in
advance, sited where a **premise becomes checkable** rather than where a task finishes. Each asks,
in these words: **"Are we doing what we think we are doing?"** (do the artifacts support the claims,
at the scope claimed) and **"Are we getting off track?"** (measured against the ORIGINAL objective —
four-language recipe parity — not against this task list, which drifts with the work).

Each reviewer has authority to say the program is fine; a checkpoint that must manufacture criticism
teaches everyone to schedule fewer. Record each verdict beside this plan, and write the next
checkpoint's trigger before closing the current one.

- **Checkpoint 1 — is the evidence foundation real?** Fires when task 24 (Morpher determinism), task
  20 (all four requirements), and the legible-skip/RED work have landed, AND Amharic has been
  mechanically re-certified from the raw 673 with in-band exclusions, AND Indonesian has been
  re-measured at its honest full eligible corpus. Deciding questions: did determinism CHANGE any
  number, and did Indonesian survive — given it is certified on a compiler that cannot propose
  compounds while its grammar declares two POS-unrestricted compounding rules?
- **Checkpoint 2 — is the compiler-axis thesis real, or did we fix one bug?** Fires when the uflexc
  compound loop, strategy-aware capability accounting, and the per-candidate budget have landed, and
  Sena is certified or definitively blocked. Deciding question: after the compound fix, does ANY
  language still prefer a different compiler? If they all converge on one winner, the
  `EmissionStrategy` thesis collapses and the 7.3–7.5 re-grounding above must itself be re-grounded.
  That would be a major reversal and must be surfaced, not absorbed.
- **Checkpoint 3 — should the deferred architecture resume as designed, and is this mergeable?**
  Fires when 7.11, the model-identity salvage, and 7.12 have merged — i.e. immediately before 7.3–7.8
  restart in earnest and before any merge to `main`. Last point at which the deferred program can be
  re-scoped cheaply rather than half-built. Also the merge gate: is every claim in these plan docs
  supported by *merged code*, per this repo's rule to update plans only for facts proven by merged
  code and broad-enough evidence?
