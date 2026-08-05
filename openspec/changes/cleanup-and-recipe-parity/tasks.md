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
      **TWO KNOWN REDS BLOCK THIS, both PRE-EXISTING — not fallout from any of the 25 merged
      commits** (measured 2026-08-03; neither is in the `-Mode test` half, which passed 1876/1876):
      (a) two fixtures ABORT THE PROCESS via unbounded recursion — task 64, also the sole blocker
      of 7.9; (b) `analyzer::budget_tests::aweti_trips_enumeration_budget_fast_with_typed_error`
      flakes by construction, its allowance of 500 sitting inside the observed trip spread
      404–532 — task 78. Earlier notes here said "exactly ONE failure"; that was the `pg-foma`
      package alone, and the corpus half adds the second.
      **AND the corpus half has not actually exercised anything yet**: it reported
      `executed 0 corpus case(s) across 0 label(s)`, so the corpus-required gate found no declared
      inputs. Until that reports a non-zero count, 2.3 cannot claim anything about corpus
      coverage — a green corpus run over zero cases is the exact failure mode `-Mode corpus-test`
      exists to refuse, so treat a zero count as a third red, not as a pass.

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

### 5.4 Work banked by the comment sweep (added 2026-08-04)

The mass comment sweep was comment-only by construction, so everything it found that needs a **code**
change was logged rather than fixed. Verdicts and evidence are in `docs/doc-code-mismatch-ledger.md`,
which lives on **`main`**, not on this branch — it arrives here at the pre-merge rebase. Each item
below therefore restates enough evidence to be actionable without it.

**Sweep outcome:** 1,606 markers → **0 in all five categories**, across 159 `.rs` files and 3 `.ps1`,
with the whole change set verified comment-only (zero non-comment lines changed across
`d49bed5..HEAD`). No crate sets `#![deny(missing_docs)]` or denies a rustdoc lint, so comment-only
edits structurally cannot fail a build — which is why no build was needed to trust this.

**Ordering rule for what follows: by who is misled and how badly, not by size.** The sweep found the
same defect — project state written into a permanent artifact — at four layers, and only the top one
was ever covered by a gate:

| Layer | Instances | Who is misled | Was it caught? |
|---|---|---|---|
| Comments / doc comments | 1,606 → 0 | maintainers | yes, the ratchet |
| Production string literals | 18 (5.4b) | **end users**, via diagnostics | no |
| Test assertions | 1 (5.4c) | CI, and whoever trusts it | no |
| A guard comment the code later violated | 1 (5.4a) | anyone reasoning about capability | no |

So 5.4a leads (it is the only behavior change and needs a decision), then 5.4b (only item whose
audience is the user), then the rest.

- [x] 5.4a **`compile_metathesis_rule`'s lowering scope — owner decided: KEEP the widening, correct
      the comments** (`6df640d`, comment-only). `replace.rs:2063` stays on
      `PatternLowerScope::RewriteRuleCompile`, so a word-edge `Anchor` compiles as a ConfirmOnly swap
      superset rather than being refused as unsupported.

      **This item was over-charged when banked, and re-deriving it rather than trusting the ledger is
      what found that.** The claim was "unowned, untested, no characteristics row" — wrong on all
      three. `2639067a` moved `capability.rs:964` in lockstep (the very disagreement the comment
      feared), rewrote `phase_c_metathesis.rs`'s test to
      `metathesis_anchor_pattern_compiles_as_confirm_only_swap_superset` asserting the net compiles
      and `qp → pq` fires, and `CharacteristicKind::Metathesis` already tracks admission through the
      function that flipped. Six comments were stale, not four (add `capability.rs:959` and
      `phase_c_metathesis.rs:959-965`, whose header said the shape "stays honestly unsupported" six
      lines above the test asserting it compiles). `Baseline` remains live via `lower_span`.

      What survives as the real defect: the guard's *stated reason* was false while the guard was
      fine — no gate here catches being right for a wrong reason.
- [ ] 5.4b **Rewrite the 18 plan references that ship to users.** The checker reads comment lines
      only — correctly, since a plan path in a string literal is often a file the code opens — so it
      cannot see diagnostic and error *text* that cites internal openspec folders: `capability.rs` (5),
      `coverage_ledger.rs` (5), `plan_interaction_coverage.rs` (2), and one each in `make_report.rs`,
      `analyzer.rs`, `compose_budget.rs`, `conformance_coverage.rs`, `morphotactics.rs`,
      `recipe_registry.rs` (production only; `tests/`, `examples/`, `cfg(test)` excluded). Example,
      `analyzer.rs:98` — *"(openspec/changes/cover-unordered-morph-rules) rather than silently
      truncated."* A user can be shown a pointer to a plan they cannot read. **Ranked above the
      remaining items because it is the only one whose audience is the end user rather than a
      maintainer.** Fix per message: say what the reader should DO, keep the construct name. Consider a
      companion check scoped to production string literals.
- [ ] 5.4c **Fix the test that freezes a date while claiming not to.**
      `pg-foma/tests/subrecipe_dossier_contract.rs:241-248` is named
      `subrecipe_dossier_logs_links_and_decision_triggers_are_dated` but asserts
      `log.contains("| 2026-08-01 |")`. A dossier updated to a later date, or a new dossier added,
      fails a test whose stated contract it satisfies — and the message will say "needs a dated
      research-log row" about a log that has one. Match `| 20\d\d-\d\d-\d\d |` instead. Ranked here
      because a test looks authoritative and gates CI, so a frozen date does the most damage there.
- [ ] 5.4d Fix the stale assertion **message** in `capability.rs`'s
      `compose_envelope_meet_correctness_two_confirm_only_constructs` (`:7633`), which calls the
      `Overwrite`-output `MprGroup` "the Refuse-worthy half". The test's `ConfirmOnly` expectation is
      correct — `MprGroupOverwrite`'s disposition is `ConfigPredicate`, not `FailClosed` — so only the
      message and the function name mislead. A string literal, hence out of a comment-only pass.
- [ ] 5.4e Consider whether `Score::scalar_objective()` should exist. It returns bare `states + arcs`
      — the objective this change explicitly rejected — and has **zero consumers**. Already a Stage 4
      instance in the grill agenda; listed here so it is not lost if the grill defers. Last because
      it is the only item that is purely a deletion judgement, with nothing depending on it.
- [ ] 5.4f **Decide whether the hygiene gate should fail CI, not just report in `doctor`.** Newly
      worth asking, because the ratchet is no longer a ratchet: every category is now **0**, so it is
      already a zero-tolerance gate in effect. It is also the one gate on this branch verified by
      falsification rather than assertion (`dfa0ca2`: injecting one marker of each category exits 1 and
      names all five; the lowercase `stage 1`/`phase a` line does not trip it). Counter-argument to
      weigh: `doctor` deliberately keeps this non-fatal because a documentation finding that blocks
      every managed build is the gate shape this repo has already watched get switched off. Failing
      *CI* is a different lever from failing *every local build* — decide which.

**Rebase note, because this one will conflict and the resolution is not obvious.** The same checker
fix landed on both sides (branch `dfa0ca2`, `main` `eb9f5ac`), so at the pre-merge rebase:

- `rust/tools/comment-hygiene.ps1` — same content both sides; expect "already applied" or a trivial
  conflict, resolved either way.
- `rust/tools/comment-hygiene-baseline.json` — **will conflict, and the branch side is correct.** Take
  all zeros. `main`'s non-zero baseline describes `main`'s tree, which still carries the backlog; this
  branch is where the sweep cleared it. Taking `main`'s numbers would silently re-authorise 1,456
  markers to reappear.

After resolving, re-run `rust/tools/comment-hygiene.ps1` and confirm it still exits 0 against the
zeros — the gate is only meaningful if the baseline matches the tree it is checking.

**Done during the sweep, recorded so the ordering above still reads correctly:**

- [x] 5.4g Make the checker's counts honest, then re-baseline. PowerShell `-match` ignores case, so
      `Stage \d` matched "stage 1" and `Phase [A-Z]` matched "phase a" — this codebase's own words for
      propose/confirm structure, not project state. Scoped to `(?-i:...)` on those two patterns only,
      deliberately not a blanket case-sensitive match, so a capitalised task number is still caught.
      Removed 36 false positives on `main`'s tree and the last 2 here. Branch `dfa0ca2`, `main`
      `eb9f5ac`; header alignment `9084d40`.

### 5.5 Round 2 of comment hygiene — claims, not words

Round 1 drove five project-state categories to zero and **missed a live defect entirely** (5.4a: six
comments contradicting the code for eight days). Round 2 adds three categories that sort comments by
what a machine can falsify, on the finding that length is not the axis that rots — a one-line claim
about another entity rots identically. Rules and rationale: `.claude/skills/code-comments/SKILL.md`.
Checker and measured baseline: `78a7ee8`.

| Category | Baseline | Target |
|---|---:|---|
| `cross-reference-claim` | 96 | **0** — pure defect: a behavioral claim about another entity with nothing checking it |
| `docs-link-broken` | 0 | **hold at 0** |
| `comment-block-too-long` | 2041 | ratchet down opportunistically; **not** a mandate to reach zero |
| `long-blocks-anchored` | 1338 | watch it: rising while the row above falls means the escape hatch is becoming the norm |

### 5.6 Round 2 outcome, and what it uncovered

**Comment categories reached: `cross-reference-claim` 96 → 0, `docs-link-broken` 0, `dead-citation` 0,
`comment-block-too-long` 2041 → 1974 (ratchet, not a target).** Four Sonnet agents on disjoint slices;
every agent claim re-derived centrally rather than relayed, which caught one bad link an agent had
added (`[`affixes::phon_context_nodes`]`, unresolvable) and one over-broad claim.

**The doc-link gate is live** (`1608ae8`): `[workspace.lints.rustdoc] broken_intra_doc_links = "deny"`,
all 19 crates opted in, `pg.ps1 -Mode doc`. It found 11 real defects on its first two runs, including a
doc comment pointing at `Grammar::allomorphs` — a field renamed to `allomorph_owners` long ago.

**Three limits of that gate, each measured rather than assumed** — do not "improve" these without
re-measuring:
- `cargo doc` has no `--tests` and rejects `--all-targets`, so a link in `tests/*.rs` is
  **structurally unvalidatable**. Use a `pinned by `<test>`` citation there instead; `comment-hygiene`
  checks those everywhere.
- Adding `--examples` produced **501 false "unresolved link" errors in the lib doc**, each noting the
  link would resolve with `--document-private-items` — naming a target set stops that flag applying to
  the lib. Strictly worse than the default.
- You cannot link *through* a private module from outside its parent; `--document-private-items` makes
  such items documented, not nameable. This answers the open question left in round 1.

- [ ] 5.6a **Decide the fate of `Disposition::FailClosed` — the gate that pins it cannot fail.**
      `grep '=> Disposition::FailClosed'` returns **nothing**: all three former FailClosed
      characteristics are now `ConfigPredicate`. So `build_ledger`'s G8 branch,
      `EvidenceRequirement::RefusalWitness` and `ContainmentEvidenceKind::RefusalWitness` have no live
      row exercising them — and `fail_closed_row_is_covered_via_refusal_witness_regardless_of_passing_set`
      asserts `ConfigPredicate` + `Dedicated` under a name promising FailClosed + RefusalWitness, while
      passing `fully_covered_constructs()` where its doc claims "a completely empty passing-fixture
      set". **It would pass with its own fix reverted.** Either retire the machinery as dead, or restore
      a genuine FailClosed characteristic and make the test assert what its name says. Full derivation
      in `docs/doc-code-mismatch-ledger.md` Tier 4c.
- [ ] 5.6b Fix the false rationale string at `coverage_ledger.rs:421` ("FailClosed … proves
      compose_envelope genuinely **Refuses**") — the cited witness asserts `ConfirmOnly`. Production
      string, so it belongs with 5.4b's layer, not the comment layer. Needs a golden re-check.
- [x] 5.6c Dead test citations fixed (3): `overwrite_group_composes_to_refuse` (×2),
      `right_to_left_predicate_refuses_quantifier_shaped_rule`, and a false *"PORTED as
      `guesser_gate.rs::analyze_word_can_guess_returns_correct_analysis`"* claim where neither file nor
      test exists. `dead-citation` now catches this class automatically.
- [ ] 5.6d Two checker precision items left open: scoring is per **physical line** (a wrapped claim
      counts twice), and `plan-reference` misses `plan §5.3`/`§6.3`/`C# #446` spellings, so that
      category's **0 is narrower than it reads**. Neither is load-bearing; both should be closed before
      anyone treats the five zeros as complete.

- [x] 5.5a **Enable `rustdoc::broken_intra_doc_links`, or the anchor rule is decorative.** This is a
      hole in the design as shipped, not a nice-to-have: the whole justification for letting a long
      comment buy its length with an intra-doc link is that *rustdoc checks the path resolves*. The
      lint warns by default, but nothing in this workspace denies it and **nothing in the build ever
      runs rustdoc**, so today an anchor is validated by nobody. Needs three things together, or it
      is worse than absent — a gate that looks enabled and is not: `[workspace.lints.rustdoc]` in
      `rust/Cargo.toml`, `[lints] workspace = true` in every member crate (a bare `[workspace.lints]`
      with no opt-in does *nothing*), and a doc build in the managed path so it actually executes.
      Verify by falsification: break one link on purpose and require a non-zero exit.
- [ ] 5.5b Drive `cross-reference-claim` to 0. Each hit resolves one of three ways, in preference
      order: convert the claim to a test and cite the test; make the entity an intra-doc link; or
      delete it if the code below already says it. **Do not blanket-delete** — see 5.5d.
- [ ] 5.5c Sweep `comment-block-too-long` where it is cheap, treating the 2041 as a ratchet. Roughly
      1200 are `///`/`//!` doc blocks and 800 are `//` implementation blocks; the second group is the
      cheaper and higher-value half, since the first is often a struct/field doc doing its job.
- [ ] 5.5d **Do not let this pass gut the interface docs.** Ousterhout's objection is correct and is
      recorded in the skill: without an interface comment there is no abstraction, and reading a
      function tells you what it does but never what it must *never* do. A negative constraint has no
      representation in Rust except a test, so deleting the comment makes it invisible rather than
      cheap. Convert or keep; never merely drop.
- [ ] 5.5e Re-measure both new counts on `main`'s tree after the merge. The figures above are the
      branch's; `main` still carries round 1's backlog, so its numbers will be higher and the
      baseline files will conflict at the rebase exactly as round 1's did (see the rebase note above).

### 5.7 NEXT ROUND — structure and comments as one pass, before the subrecipe build-out

**Owner framing, and it is the organizing principle: cleaning up structure and cleaning up comments
are the same job, because they need the same context.** A comment that has rotted past three lines is
usually describing a function that does too much; you only pay the cost of understanding the module
once, so do both in the same visit. Goal is the cleanest possible base before the subrecipe /
mechanism-graph build-out (§1's owner decision) begins.

**DELETE BEFORE POLISH.** This ordering is not stylistic — it was worth real time twice today. The
`FailClosed` removal erased whole comment blocks that a sweep would otherwise have carefully rewritten,
and the link-policy reversal invalidated ~90 link "fixes" made four hours earlier. Any module slated
for deletion must be deleted first, or its cleanup is thrown away.

- [ ] 5.7a **Take the Stage 3 cut first — but the grill agenda's list is WRONG in two places, verified
      by reference count before touching anything.** Do not delete from the agenda's list directly.

      | Symbol | Code refs | Verdict |
      |---|---:|---|
      | `ExecutableCandidate` | 6, in 2 files | **delete** |
      | `PortablePlan` | 22, in 3 files (incl. its own gate) | **delete** |
      | `executable_candidate.rs` + `executable_candidate_gate.rs` | — | **delete** |
      | `CertificationScope` | **0** | already gone; the agenda names a symbol that does not exist |
      | `ExecutionDisposition` | 21, in 4 files | **KEEP — the agenda is wrong.** It is *defined* at `recipe_mechanism.rs:680` and consumed by `recipe_mechanism_graph.rs`: it belongs to the mechanism-graph substrate, which §1's owner decision makes the **build-out target**, not a cut candidate. Deleting it cuts into what we are keeping. |
      | `LoweringAdapter` | 55, in 12 files | **extract and keep**, as the agenda says |

      The corrected cut is therefore materially smaller and safer than the approved-in-principle one.
      Needs one real build. `Registry::executable_candidate`'s single crate-wide caller (its own gate)
      still holds — that part of the agenda checks out.
- [ ] 5.7b **Per-module structure+comment passes**, in this order. These are the modules where the
      backlog and the architectural weight coincide, and every one of them is on the path the
      subrecipe work will touch:

      | Module | Long blocks | Why it leads |
      |---|---:|---|
      | `pg-foma/capability.rs` | 141 | 7.4k lines; the capability spine the subrecipes gate on |
      | `pg-foma/emit.rs` | 138 | the lexc emitter; largest single source of composite complexity |
      | `pg-rules/rewrite.rs` | 79 | phonological rewrite core |
      | `pg-rules/morph.rs` | 72 | morphological cascade |
      | `pg-foma/recipe_runtime.rs` | 68 | **the subrecipe substrate itself — clean this before building on it** |
      | `pg-rules/stratum.rs` | 63 | stratum analyzer |
      | `pg-foma/replace.rs` | 61 | relational rule compiler |
      | `pg-parse/morpher.rs` | 54 | the parse entry point |
      | `pg-cli/main.rs` | 46 | 2.2k-line CLI; mostly extractable |
      | `pg-foma/lower.rs` | 44 | pattern lowering |

      Ten files hold 766 of 3,272 blocks (23%) — this is a **long tail, not a hotspot**, so treat the
      list as "where the value is", never as "when we are done". Per module: read it, delete what is
      dead, extract what is doing two jobs, and let the comments fall out of that — a block that
      cannot be got under three lines usually marks the seam worth extracting.
- [ ] 5.7c Finish `cross-reference-claim` at 0 and hold it. Ratchet `comment-block-too-long` down as
      5.7b lands; it is **not** a target of zero and never was.
- [ ] 5.7d Residue from round 2, each verified, none blocking:
      - `EvidenceProvenance::Behavioral` is now unproducible — all 12 predicates return `Structural`.
        Follow `FailClosed` out, or give it a producer.
      - **A real check was lost with the vacuous test.** The deleted
        `fail_closed_refusal_witness_resolves_to_an_actual_test` graded zero rows, but it uniquely
        asserted a cited identifier lives in one of its own cited files and is preceded by `#[test]`.
        The survivors only check the name exists somewhere. Generalising the strict form to all ~20
        live citations would be a genuine strengthening.
      - Five fixture rationales (`preflight.rs:456`, `selection.rs:424`, `pg-cli/pack.rs:524`,
        `main.rs:1919`, `make_report.rs:1035`) describe an `Overwrite` MprGroup while the
        `include_str!` points at `simultaneous-subrule-genuine-overlap`. A fixture swap landed without
        its prose. Prose corrected; the swap itself is unreviewed.
      - `COVERAGE_CLI_SCHEMA_VERSION` still `1` after JSON fields were removed.
      - Dead `FailClosed` vocabulary survives in five `docs/`+`openspec/` files.
- [ ] 5.7e Two checker precision items, both making a reported **0 narrower than it reads**: scoring is
      per *physical line* (a wrapped claim counts twice), and `plan-reference` misses `plan §5.3`,
      `§6.3`, `C# #446` spellings. Close these before anyone treats the zeros as complete.

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

- [x] 7.3 Rework the six language-name-free mechanism types: `Morphotactics`, `StaticPartition`,
      `OrderedPhonology`, `StructuralAllomorph`, `CopyProcess`, and terminal `BoundaryCleanup`.
      Nodes own typed semantic requirements/guarantees, edges own dependency/order, and candidate
      bindings own execution disposition. Delete duplicate wire provenance and unproved blanket
      contracts from the initial vocabulary commit. **Re-grounded by the Wave 3 evidence above:** the
      typed requirements/guarantees must be the ones that decide compiler admissibility and recall,
      and no node may exist solely to name a plan-shape permutation.
      Landed: `MechanismNode` owns typed source references, a `SymbolSpace`, an optional stratum,
      and `construct_requirements: BTreeSet<CharacteristicKind>` -- the re-grounding, expressed in
      exactly the key `strategy_coverage` is indexed by so it RESOLVES through that table instead of
      restating it. `MechanismEdge` is now a bare `(producer, consumer)` pair; every compatibility
      check is COMPUTED from the two endpoint nodes, so an edge can no longer assert a property its
      endpoints lack. `MechanismBinding` is the only type that can express what a compiler
      delivers: private fields, `derive(node, strategy)` the sole constructor, so an
      `ExecutionDisposition` cannot be written down without naming its `EmissionStrategy` (the
      `uflexc`/`Compounding` inheritance bug, made inexpressible). Deleted as duplicate provenance:
      `MorphotacticsSpec::{strata,rules}`, `OrderedPhonologySpec::stratum`,
      `StructuralAllomorphSpec::{rule,allomorphs}`, `CopyProcessSpec::rule`,
      `BoundaryCleanupSpec::table`, and both contract halves' `stratum`/`symbol_space`. Deleted as
      unproved blanket contracts: `Identity{Guarantee,Requirement}`,
      `Multiplicity{Guarantee,Requirement}`, `CopySpan{Guarantee,Requirement}`, `DynamicState`,
      `PartitionPredicate`, `OrderedRuleAtom::swap_construction_attempted`,
      `StaticPartitionSpec::stable_for_lifetime`, `CopyProcessSpec::{kind,max_span,
      max_chain_depth}`, `StructuralAllomorphSpec::bounded_local_shape`, and the whole
      `InterfaceContract`/`ProvidedInterface`/`RequiredInterface` triple. Identity and multiplicity
      are the parity relation, measured against an oracle; Amharic's 2.2x-cheaper
      `identity-mismatch` candidate is the measured reason a declared `Preserved` was a false
      comfort. `BoundaryState` is derived from the mechanism kind (only cleanup removes), so "all
      boundary-consuming consumers run before cleanup" is structural rather than declared.
- [x] 7.4 Derive mechanism providers only from the shared `GrammarSemantics`; no provider may reread
      `Grammar` to decide applicability. Require typed source references, canonical graph identity,
      and byte-identical fresh-load projection; inert hints may not create mechanisms.
      Landed as `pg_foma::mechanism_provider::derive_mechanism_graph(&GrammarSemantics) ->
      MechanismGraph`. The signature IS the enforcement: no `&Grammar` parameter, no `&Grammar`
      front end, and `GrammarSemantics::grammar()` is never called in the module. Attribution joins
      `CharacteristicObservation::kind` (-> `mechanism_kind_for`, exhaustive, no catch-all) with
      `::location` (-> `MechanismSource`, via the `From<ModelLocation>` impl that already existed).
      Inert hints create nothing for free: `characterize` uses the structural
      `rhs_has_true_reduplication`, so a non-`Implicit` `ReduplicationHint` on a
      non-reduplicating allomorph raises no observation, hence no source, hence no `CopyProcess`
      node -- the provider never re-decides the question. Canonical identity: nodes in
      `MechanismKind::COMPOSITION_ORDER`, edges chaining the present ones, `BTreeSet` requirements
      and sources, sorted partition members, authored rule order; `canonical_projection()` is
      byte-identical across independent loads. Five additive `GrammarSemantics` projections
      (`prule_ids_in_order`, `template_ids`, `char_table_count`, `primary_table`,
      `primary_table_boundary_symbols`) are all grammar-only, so the memo is not re-keyed and the
      strategy-inheritance trap is not reopened. Nodes are grammar-wide (`stratum: None`): placing a
      rule-located observation in a stratum needs a rule->stratum map `GrammarSemantics` does not
      own, and inventing one here would mean the grammar re-walk this task forbids. Reachable from
      no routing, applicability or candidate path -- deriving a graph changes no outcome.
- [x] 7.5 Make the Registry the sole constructor of a validated `ExecutableCandidate` binding a
      stable semantic digest, portable round-trippable Plan document/digest, exact lowering adapter,
      existing runtime requirements, mechanism graph/bindings, and certification scope. Reject
      bypass/corruption; never use FNV Plan roots as artifact identity or execute an implicit fallback.
      Landed as `pg_foma::executable_candidate`, whose sole constructor is
      `Registry::executable_candidate`. **Sole construction is enforced by a type, not a
      convention**: `seal` requires a `recipe_registry::RegistryAuthority` whose only field is
      private to that module, so no other module in this crate can produce one, and it is neither
      `Copy` nor `Clone` so one cannot be kept and reused. `ExecutableCandidate`'s fields are
      private and it implements neither `Serialize` nor `Deserialize` -- deserialization would be a
      second constructor that skips every check, which is the bypass the task forbids; the
      PORTABLE part is `PortablePlan`, which round-trips and re-verifies.
      **Identity is domain-framed SHA-256, never the FNV root.** `plan.rs` documents `NodeId` as an
      unseeded 64-bit FNV-1a that is "not collision-resistant"; that is fine for interning and
      cannot ground a persisted artifact or a certification. Two projections
      (`pangloss.foma.plan-document/v1`, `pangloss.foma.candidate-semantics/v1`) are
      length-prefix-framed into the preimage, the same contract `pg_assess::digest::
      digest_projection` uses -- computed locally because `pg-foma` deliberately does not depend on
      the assessment layer (7.12's own slice note). The semantic digest's preimage is
      `MechanismGraph::canonical_projection()`, i.e. task 7.4's byte-identical fresh-load
      projection. The FNV addresses still appear INSIDE the document as its structure, and
      `PortablePlan::decode` recomputes every one of them from the decoded content, so a tampered
      id, payload, child list, or descendant is a typed refusal rather than a silent repair; a
      `Gate` arity error is likewise refused BEFORE interning, because `Plan::add_node` guards it
      with a `debug_assert!` that panics in debug and is silent in release.
      **The typed refusal that replaces an implicit fallback** is
      `CandidateConstructionError::MechanismRefused`: if any mechanism this grammar requires is
      `CannotRepresent` for the candidate's adapter, construction refuses naming the mechanism, the
      adapter, and `strategy_coverage`'s own citation -- it never reaches for a compiler that
      happens to work. Model ids travel as `recipe_mechanism::WireModelId`, so a `PRuleId` read
      back where a `LexEntryId` belongs is a typed decode error rather than a coerced integer.
      `LoweringAdapter` is 1:1 with `EmissionStrategy` in both directions (task 7.13 replaces the
      enum with it and cannot until that holds); `RuntimeRequirement` is derived from the adapter
      and the plan, never declared. `CandidateCertificationScope` is deliberately named apart from
      7.12's corpus-side `CertificationScope` and states no identity/multiplicity guarantee -- those
      ARE the parity relation, measured against an oracle. Reachable from no routing, applicability
      or evaluation path: `instances_for_*`, `materialize*` and `recipe_runtime`'s dispatch are
      untouched, and `tests/executable_candidate_gate.rs` pins that a sealing refusal changes
      neither the offered instances nor the materialized candidates. Migrating consumers onto it is
      7.13's.
- [x] 7.6 Maintain one research dossier per mechanism with scope, invariants, ≥2 language/family
      anchors, chosen/rejected architectures, complexity, evidence log, and explicit
      fits/refines/splits/adds triggers. Contract and six dossiers landed in `a80cae0`; all concrete
      model-ID/counter/cap evidence remains canonically unmeasured and blocks implementation claims.
- [x] 7.7 Prove the first `Morphotactics → BoundaryCleanup` vertical slice with two independent
      complete-template exercises and two cleanup exercises, exact analysis/root/multiplicity
      parity, cleanup idempotence, and no language-name routing.
      Slice note (2026-08-03, **authored but NOT yet executed** — the box stays unchecked until the
      gate has actually run green; see the "what to run" list at the end of this note):
      `rust/crates/pg-foma/tests/morphotactics_boundary_cleanup_slice.rs` joins two ends per
      fixture — the derived `MechanismGraph` (a `Morphotactics` node, a terminal `BoundaryCleanup`
      node, and a directed path between them) and what the engine actually produces for that same
      grammar's pinned words, projected through `parity::OccurrenceIdentities`. A gate asserting only
      the graph would assert a description of a pipeline; one asserting only the analyses would not
      have touched the slice.
      **Every expected number is READ OUT of a committed `words.yaml`, never hand-derived.** The four
      exercises are existing staged fixtures whose signatures were measured by their authors:
      `template-category-sharing` (cross-template OVER-generation: two impossible mixes must have
      empty identity sets), `optional-template-composite` (zero-exponence UNDER-generation: a
      mandatory-but-silent slot must contribute a second distinct identity for `monu`),
      `recipe-strata-generic` (a boundary PRODUCED by morphotactics — the compounding seam — with no
      boundary consumer in the grammar at all) and `recipe-ordered-generic` (a boundary CONSUMED by
      `mrComplexMeta`'s `BoundaryMarker`, with no compounding at all). The cleanup pair is
      independent two ways: neither grammar can exercise the other's mechanism. The template pair is
      independent in its falsifiers — each has one the other cannot detect — but NOT against a defect
      in the shared `ApplyMorphologicalRules(input).Concat(ApplyTemplates(input))` interleaving, and
      that limit is stated rather than papered over.
      **Which relation each assertion uses is named at every use site**, because a relation chosen
      for convenience is how the v1 scope was once made invisible: the program's parity relation
      (deduplicated `AnalysisIdentity` SET equality) carries the language-rename invariance check;
      7.7's additional MULTIPLICITY ask is carried by `raw_analyses()` against the committed
      `parses:` row count, since `words.yaml` is sorted-but-NOT-deduped and a repeated signature
      there is a measured multiplicity; full `WordAnalysis` equality is used nowhere. Distinct-identity
      counts are bounded by the committed record from both sides (distinct morpheme-JOIN count below,
      row count above) and pinned exactly only where those bounds meet — so the gate cannot assert a
      cardinality its fixture did not record.
      `root_index` load-bearing is pinned by `head-ambiguous-compounding`'s `dakimo` (a witness, not a
      fifth exercise): the full relation must keep two identities, and the root-BLIND projection of
      that same set must collapse to one, STRICTLY fewer — so the test cannot pass if root position is
      ignored. Idempotence is pinned on the cleanup relation built exactly as `build.rs`'s own
      `boundary_cleanup_net` builds it, applied twice via `apply_down`, with the adjacent-doubled
      boundary as the input a once-only or context-restricted deletion fails on, plus non-vacuity and
      an identity-on-boundary-free-input companion. No-language-name-routing reloads each fixture with
      its `<Language><Name>` replaced and requires byte-identical `canonical_projection()` and
      unchanged per-word identity sets AND multiplicities.
      Two findings worth carrying forward. (1) The morphotactics dossier's designated exercise 2,
      `recipe-template-generic`, is one of the fixtures that ABORT the test process, so it cannot host
      a gate at all — its scale characterization stays parked with that defect rather than being
      folded into a green gate. (2) The cleanup dossier's designated exercise 1, the Sena-shaped
      all-boundary `^0+` allomorph, exists only as inline XML inside
      `boundary_marker_epsilon_collapse_gate.rs` and has no committed `words.yaml`; asserting over it
      would require hand-deriving signatures, so staging it (and measuring those signatures) is owed
      and is NOT done here. Nothing in this slice touches an optimizer internal, a proposal ceiling,
      or a clock.
      **What to run:** `rust/tools/pg.ps1 -Mode test -Package pg-foma -Filter
      morphotactics_boundary_cleanup_slice` for the slice itself, and
      `rust/tools/pg.ps1 -Mode test -Package pg-parse -Filter conformance_fixtures_gate` to confirm
      the five fixtures still replay (this slice edits no `grammar.xml`/`words.yaml`, only their
      `STAGING.md`, so that gate should be unchanged and is the cheap one to run first).
      Costing note, because three agent batches on 2026-08-02/03 spent their whole budget here and
      produced zero measurements: `-Filter` is appended to `cargo nextest run` as a positional
      test-NAME filter (`pg.ps1` line ~564), so it narrows EXECUTION only. `-Package pg-foma` still
      compiles and links all ~75 of that crate's test binaries plus the vendored `foma` C library, and
      `pg.ps1` exposes no `--test <target>` passthrough that would narrow compilation instead. There
      is no cheaper path to verifying this file; budget a cold pg-foma build, or batch it with other
      pg-foma work in one run.
- [x] 7.8 Exercise the remaining orthogonal basis at least twice where possible: template
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
      Slice note (2026-08-03, **compiled but the gates have NOT been run** beyond the three new
      tests and pg-foma's 555 unit tests — box stays unchecked; the exact commands are at the end of
      this note). Four of the six items landed, one is reported satisfied already, one is
      deliberately NOT done.
      **Exact adapter lowering + `CandidatePlan` deleted.** `enumerate::CandidatePlan` becomes
      `enumerate::LoweredCandidate`, whose compiler field is the typed `LoweringAdapter` task 7.5
      already seals onto `ExecutableCandidate` rather than a second `EmissionStrategy` kept in
      correspondence by hand; `recipe_runtime`'s dispatch, `build_candidate`'s refusal,
      `finished_net_digests`, `realize_accuracy_proposer` and `executable_candidate::seal` all now
      match on that one value. `EmissionStrategy` SURVIVES, per the amendment, as the reported
      selection axis (`RuntimeEvaluation::realized_strategy`, `winner_strategy`,
      `strategy_coverage`, `compose_envelope_for_strategy`, `MechanismGraph::bind`), reached through
      the `LoweredCandidate::strategy()` projection; `executable_candidate`'s
      `every_strategy_has_exactly_one_adapter_and_back` remains the compiler-checked proof the
      correspondence is total and injective, which is the amendment's stated precondition for ever
      deleting the enum. Zero `CandidatePlan` references remain crate-wide.
      **Positional baseline state deleted, in both of its forms.** `evaluate_plans` derived
      `is_baseline` from POSITION (`i == 0`) and `evaluate_plans_marked*` took it as a parallel
      `&[bool]` guarded only by a length `assert_eq!` whose own message admitted the hazard. Both
      are gone; the fact is `LoweredCandidate::role` (`CandidateRole::{Baseline, Alternative}`),
      carried by the candidate it is a fact about, and the registry DERIVES it (`SafeTransform::
      Identity` under the plan-interpreting adapter hands back the baseline plan verbatim and is
      therefore the default compilation) rather than declaring it. The three `_marked` entry points
      are deleted. **This is a behaviour change, not a pure refactor, and it is the point**:
      `materialize_distinct` orders candidates by FAMILY ID and `ordered-morphophonology` sorts
      after `bounded-metathesis`, `class-exception-cascade`, `complete-template` and `copy-branch`,
      so element zero was NOT the default compilation on any grammar those apply to — every caller
      that passed `(0..n).map(|i| i == 0)` was holding an arbitrary alternative to the baseline's
      marker-fallback rule. `recipe_runtime_net_is_queryable_gate` was one such caller (its
      `index == 0` now reads `role.is_baseline()`); `boundary_marker_epsilon_collapse_gate` and
      `parity_divergence_census` evaluate whole registry batches and may move for the same reason.
      **Duplicate runtime artifacts deleted.** `RecipeOptimizationReport` inlined the full text of
      `baseline.plan.json`, `baseline.plan.mmd`, `winner.plan.json` and `winner.plan.mmd` beside the
      `*_path` fields naming those same already-written files. `validate()` never read them and no
      test did either. The `*_path` fields (used by `markdown()`) stay. `report.json` therefore
      loses four fields; `RECIPE_REPORT_SCHEMA_VERSION` was NOT bumped, because the removed fields
      were redundant copies rather than a semantic change and nothing deserializes them.
      **Fake zero measurements deleted, in the pilot.** A pilot candidate the capability envelope
      REFUSED recorded `build: 0, evaluation: 0` for stages that never ran, and `summarize_pilot`
      folded those literal zeros into the build/evaluation percentiles — which are summed into
      `PilotCosts` and DECIDE WHICH SEARCH STRATEGY the run uses, so this was not cosmetic. Both
      fields are `Option<u64>`, the quantiles are taken over rows where the stage executed, and a
      new `PilotSummary::executed_samples` names the honest denominator. `materialize`/`capability`
      stay unconditional: a refused row genuinely paid both.
      **Run-scoped lowered-candidate reuse: reported ALREADY SATISFIED for the expensive half, and a
      third layer deliberately not built.** `RunEvaluationCache`'s net-level dedup (landed
      2026-08-03) keys a whole measurement on `(grammar identity, corpus hash, observed mode,
      finished-net digest)` and serves it to any later candidate whose finished network is identical
      arc for arc, which is the case plan-shape recipes routinely produce (spread 0 across 8
      fixtures). What is NOT reused is the LOWERING itself — every candidate still compiles its own
      network, deliberately, because the digest is not knowable until it has. A pre-build key of
      `(grammar identity, plan-document digest, adapter)` would close that, and it is not built for
      two reasons: `pg_cli` already drops root+strategy duplicates before evaluation so it would
      almost never fire in production, and a third cache keyed differently from the existing two is
      how caches silently miss.
      **Implicit fallback: NOT deleted, and this is a disagreement stated rather than worked
      around.** The remaining fallback is that a `Baseline` whose plan needs marker subtrees
      `build_controllable` cannot build is realized by the whole-grammar tuned adapter. Deleting it
      outright would report `BuildFailed`/`Truncated` for the default compilation of every templated
      grammar, and the measured cost is on record in `recipe_runtime`'s own comment: 133 states /
      3307 arcs controllable-only against 6376 / 68693 from the tuned path, which proposed correctly
      where the controllable net proposed nothing for 19 of 20 words. So it is made EXPLICIT instead
      — a declared `CandidateRole` on the candidate, dispatched on the adapter, reported through
      `realized_strategy` — and the deletion is left to whoever can supply the replacement evidence.
      The related `ResourceBreach`-relabelled-`Unsupported` defect on that same path was identified
      and NOT fixed: `recipe_runtime_net_is_queryable_gate` currently REQUIRES `Unsupported` there,
      so unpicking the relabel is a gate change that belongs with the filed
      declining-allowance/`--confirmation-work` work rather than bundled here.
      **New gates, each verified to fail with its own mechanism reverted (sabotage run 2026-08-03).**
      `net_dedup_gate::the_plan_document_identity_is_canonical_across_two_independent_constructions`
      — the Plan identity is byte-stable across two independent grammar LOADS and two independent
      enumerations, and discriminates two grammars. Placed beside the RED `grammar_identity` pin so
      the contrast in PREIMAGE is readable in one file: substituting a derived `Debug` projection
      makes it fail, and the failure output shows the live defect (`symbol_index: {"fRoundMinus": 1,
      "fRoundPlus": 0}` in one load, the other order in the other). `recipe_registry::
      the_baseline_role_follows_the_baseline_plan_and_never_the_position` — exactly one Baseline, it
      carries the baseline plan verbatim under the plan-interpreting adapter, and every Alternative
      differs in plan or compiler. `recipe_space::
      a_stage_that_never_ran_does_not_contribute_a_zero_to_its_quantiles`.
      **Not done, and owed.** The end-to-end falsifier for the marker-fallback ROUTING (evaluate an
      Alternative alone and require it is not rescued as a baseline) needs a fixture measured to
      fail on the controllable net with unbuildable markers; none was identified, so the role change
      is pinned structurally at the registry rather than through the evaluator. `grammar_identity`
      stays RED — it is not the Plan identity this task names, and fixing it means a canonical
      `Grammar` serialization, which is the queued persistent-oracle-cache owner's work.
      **What to run** (warm tree, `--test <target>` narrows COMPILATION, `-Filter` does not):
      `$env:PANGLOSS_EXTRA_ARGS = '--lib --test net_dedup_gate --test recipe_runtime_net_is_queryable_gate'`
      then `pg.ps1 -Mode test -Package pg-foma`; then the wider set
      `'--test cross_compiler_equivalence_gate --test recipe_accuracy_gate --test parity_divergence_census --test boundary_marker_epsilon_collapse_gate --test recipe_promoted_fixtures --test deletion_reduplication_exception_fixture'`;
      then `pg.ps1 -Mode test -Package pg-cli`.
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

**TRIGGER TIGHTENED 2026-08-03** — the condition above now reads as SATISFIED ON A TECHNICALITY:
tasks 20–22 are all closed (22 fixed, 21 reverted as inert rather than completed, 20 landed), so
7.3–7.8 would resume automatically while the assessment loop is still measured in hours. That is the
same shape as the failure this deferral was created to fix, inverted: an architecture program
re-entering ahead of the thing it depends on. **7.3–7.8 additionally require the loop-speed bar
below to be MET AND MEASURED**, not merely worked on. A closed task list is not a fast loop.

### The two loops, and the two different bars (added 2026-08-03)

Owner objective, verbatim: *"We should be able to assess a rough pass whether it works in 5 minutes
or less. That is the whole purpose of what we're doing... the whole point of this set of tasks is to
have an under-5-minute, ideally under-30-second assessment time."* One long pass for a baseline is
fine — and that baseline is **already paid for**, so no new long run is needed to establish it.

These are TWO loops with TWO achievable floors, and conflating them is what produced a two-hour
verification of a two-line change:

| | Loop A — developer verification | Loop B — recipe assessment |
|---|---|---|
| Question | "did my change break anything?" | "is this candidate any good?" |
| Instrument | the Rust test suite | `recipe-optimize` |
| Achievable bar | **~5 min** | **sub-30s** |
| Why not lower | an incremental Windows build + link of `pg-foma` alone exceeds 30s | — |
| How | filtered gates, **available today with zero code**: `pg.ps1 -Mode test -Package pg-foma -Filter <targeted>` (pg.ps1:101–102, 561–571) | ~25–50 word subset x ~3 *distinct* nets after dedup x cached oracle ≈ 15–25s |
| Nature of the fix | **discipline, not code** — no optimization task addresses it | algorithmic |

Loop B breaks only on an explosive candidate: Sena's plan-composed at ~59k proposals/word makes even
10 words ≈ 54s, so a shape screen — or an honest deterministic *"exploded at N proposals, no
verdict"* — is part of the minimum set, not an extra. **Aweti is out of scope for any assessment-speed
bar** until the July `apply_up` truncation plan lands; no loop optimization changes that.

### Critical-path ordering for the loop-speed objective

Reordered 2026-08-03. The previous order buried the one task that delivers the number in fifth place.

1. **Epsilon-loop reguard** — lands because it is a regression *we* introduced, not because it is on
   this path.
2. **Split the accuracy verdict from the ranking** — the intersection answers "did we undergenerate"
   in milliseconds. THE task that delivers the owner's number. It alone is not sufficient: the run
   still pays compile and apply for every candidate, including duplicates and explosives.
3. **Net-level dedup** — measured: all five plan-composed permutations bit-identical on two real
   corpora, so a 7-candidate run does ~2.3x the distinct work it needs to.
4. **Persistent cross-run oracle cache** — the oracle is a pure deterministic function of
   (grammar digest, word, step cap, memory ceiling); recipe churn holds the grammar FIXED, so every
   parse after the first run of a given grammar+config recomputes a known value. Turns 15–25s from
   *edge* into *margin*.
5. **Shape-based screen** — the only way to identify an explosive net *before* paying its apply cost.

Deferred until the bar is measured missed: cross-candidate memoization and confirm-call fusion. Both
are real constant-factor wins and neither is load-bearing; they also carry the one recorded ordering
hazard (a memo hit recording zero makes `Score::key` order-dependent and hands the win to whichever
candidate was evaluated second), so step 2 must precede them.

Loop A discipline, separate and cheap: fix the red CI first — with no machine-side backstop, agents
plausibly over-verify locally to compensate — then make the minimum-sufficient-gate rule a hook
rather than a policy, on the `block-bare-cargo.py` pattern. A prohibition a model can reason its way
around under pressure is not a control; that is that hook's own argument, and it applies here.

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
