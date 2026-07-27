## 1. The plan itself (this change)

- [x] 1.1 Read the ground truth: `pangloss coverage` output, `capability.rs`, `coverage_ledger.rs`,
      `conformance_coverage.rs`, `plan_interaction_coverage.rs`, `constructs.txt`, ADR 0001/0002/0005,
      `add-reference-hermitcrab-parity`'s `tasks.md`, the `conformance-grammars` skill, and
      `STAGING.md` — no number or claim invented (design.md's own "Ground truth" section)
- [x] 1.2 Define the promotion ladder once, distinguishing the mandatory `ConfirmOnly` rest from the
      optional, non-blocking `Admit` optimization (design.md D1)
- [x] 1.3 Produce the per-construct table for all 14 non-`Proven` kinds with disposition, split,
      closing evidence, fixture(s), and verdict (design.md D2)
- [x] 1.4 Report the `constructs.txt` ↔ `CharacteristicKind` mismatch shape in both directions
      (design.md D3)
- [x] 1.5 Specify the tree-bounded fixture-enumeration method (design.md D4)
- [x] 1.6 Name the upstream `constructs.txt` PR with proposed row text (design.md D5)
- [x] 1.7 Identify which promotions need the C# oracle harness vs. which can proceed against the
      in-repo confirm engine (design.md D6)
- [x] 1.8 Sequence the work and state a crisp definition of done (design.md D7)
- [x] 1.9 Add this change to `STAGING.md`'s spine (successor to Stage 2/3) and to its "Still open"
      section
- [x] 1.10 `openspec validate plan-construct-coverage-completion --strict` and
      `openspec validate --all --strict` both pass

## 2. Upstream constructs.txt task (hand-off, tracked here — not implemented by this change)

- [x] 2.1 **DONE 2026-07-25** — sillsdev/machine#465 against `conformance-framework`, adding all four
      rows (rewrite direction left-to-right and right-to-left, subrule-level required/excluded POS-or-MPR
      gating, multiple character-definition tables).
- [x] 2.2 **DONE 2026-07-25** — submodule pointer bumped to `4560e9e` and all four
      `construct_ids_for` arms mapped, taking `Unmappable` to zero.
- [ ] 2.3 **FRAGILITY TO CLOSE — re-point the submodule when #465 merges.** Verified 2026-07-25:
      `4560e9e` is **not** on `origin/conformance-framework` (which is at `dd8f95c`); it exists on the
      remote only as `refs/heads/g9-add-missing-construct-rows`, PR #465's own head branch. So the
      pointer is fetchable today but **will dangle the moment that PR is merged and its branch
      deleted**, which is the default on most merges. This is now load-bearing: task 6.2 made the
      coverage cross-check BUILD-BREAKING, and it needs those four `constructs.txt` rows to resolve —
      so a deleted PR branch would take the build down, not merely degrade a report. On merge, re-point
      the submodule at the merged commit on `conformance-framework`.

## 3. Fix the conformance-coverage gate's scope before flipping it (hand-off)

- [x] 3.1 **DONE 2026-07-25 (G8)** — `supported_kinds()` returns all 20 `CharacteristicKind`s; a new
      `EvidenceRequirement`/`evidence_requirement_for(Disposition)` encodes the per-disposition rule
      (`PassingFixture` for `Proven`/`ConfigPredicate`/`ConfirmOnly`, `RefusalWitness` for
      `FailClosed`), and `disposition` is carried on every `CoverageReportRow` so a flip can stage
      `Proven` first. This also fixed a real overclaim rather than only widening scope: `build_ledger`
      had been grading **every** row against the passing-fixture set, so `MprGroupOverwrite`
      (`FailClosed`) could report `Covered` purely because its sibling `MprGroupAppend` tagged the
      shared `"MPR features/groups"` id, with the refusal never exercised.

## 4. Close PROVABLE rows, one construct at a time (hand-off, Stage-2-style full kits)

- [ ] 4.1 `Compounding.recursive`: rule-graph reachability + depth-budgeted construction + containment
      test + new `edge-cases` fixture
- [ ] 4.2 `RightToLeftRewrite`: extend `compile_rtl_branch_net` to currently-excluded pattern shapes,
      one shape at a time, each with its own fixture
- [x] 4.3 `CircumfixOutputAction`: census which allomorph shapes fail `is_structural_rule`/
      `build_structural_composites` today — **DONE 2026-07-25**,
      `docs/conformance/circumfix-structural-composite-census.md`. Key finding: the *mechanism* is
      allomorph-complete (`struct_extend` delegates to `pg_rules::morph::synthesize`, `emit.rs:2272`);
      every gap is in candidate *selection*, and all fail over-refusing (honest/fail-closed, no
      overclaim). Three named shapes, split out below.
- [x] 4.3a **C1 — DONE 2026-07-26** — `rule_role` (`emit.rs:555-560`) classifies a rule by allomorph **0 only**, so a
      circumfix-shaped allomorph at index ≥ 1 never becomes a structural candidate (and the gap
      appears/disappears as an author reorders allomorphs). Fix: allomorph-wise `any` in
      `is_structural_rule`, without changing `rule_role`'s contract for its other callers. Fixture: a
      rule whose non-first allomorph is circumfix-shaped. **Highest priority of the three.**
      **Outcome:** done exactly that way — `rule_role` untouched. Pinned by
      `circumfix_allomorph_selection_is_order_independent`, which declares the same rule with its
      allomorphs in BOTH orders and requires identical selection, since order-dependence was the
      actual defect. Fixture `circumfix-non-first-allomorph-selection`.
- [x] 4.3b **C3 — DONE 2026-07-26** — `classify_affix`'s interior-action test (`emit.rs:434-440`) returns `Role::Infix`
      before the leading-AND-trailing test (`:441-453`) can return `CircumfixPrefix`, so a
      simultaneously-circumfixing-and-infixing RHS is routed to `preexpand` instead of the structural
      mechanism `emit.rs:1928-1934` says is required. Fix: let `CircumfixPrefix` win when both hold,
      with its own recall argument. Fixture: such an RHS.
      **Outcome:** leading-AND-trailing now tested before the interior-action test. Two independent
      reasons recorded rather than one — `Infix` is the wrong label regardless of consequences, AND
      `build_structural_composites`' `CircumfixPrefix` admission is unconditional whereas the
      `preexpand` `Infix` route would make the predicate refuse on grammars `preexpand` cannot serve.
      Mechanism hand-off checked explicitly, not assumed: `preexpand` selects on `rule_role` matching
      `Infix`/`Prefix`/`Suffix`, so it drops the rule cleanly rather than both mechanisms claiming it or
      both dropping it (`circumfix_infix_ownership_handoff_is_clean`). Fixture
      `circumfix-infix-interior-action-precedence`. C2 verified unaffected by the reordering.
- [ ] 4.3c **C2** — `classify_affix`'s reduplication test (`emit.rs:408-414`) likewise preempts the
      circumfix test. **Do NOT schedule independently** — which role wins decides which mechanism
      claims the allomorph, so this is a joint decision with row 11's `Reduplication` carve-out
      boundary and needs both recall arguments re-checked together.
- [x] 4.4a `MultiTable`: **DESIGN DONE 2026-07-25**,
      `docs/conformance/multitable-shared-representation-design.md`. **The disjoint-token-range
      encoding this task originally named is the WRONG fix** and is withdrawn: the token is a
      table-blind per-table index (`replace.rs:275-277`), so two tables sharing a spelling at
      *different* indices make a rule fail to fire on the other table's material — a
      **false-negative** (unrecoverable under propose-and-confirm), not the false-positive the
      predicate's doc describes. Range separation would entrench that. Recommended instead:
      **cross-table representation aliasing** — render an atom as a union over every table's token
      for the same normalized spelling, at `render_slots` only, leaving per-table resolution
      (`owning_table`) untouched. Recall-safe by construction (only ever adds alternatives), needs no
      `model.rs` change (so R1's frozen-model audit stays closed), and sidesteps PUA capacity
      entirely. Supporting fact: `bridge.rs:260-300` shows the oracle matches classes by feature
      lanes, not char-def identity, so the proposer should not enforce a distinction the oracle
      does not make.
- [ ] 4.4b `MultiTable`: build it — representation→`(TableId, CharDefId)` multimap (same NFD
      normalization as `CharDefTable::lookup`/`surface_variants`); thread `TableId` into
      `SegAlphabet` (no defaulted id — same mistake class as the `char_tables[0]` default
      `owning_table` removed); alias-expand in `render_slots`' `Fixed`/`Union` arms only, never in
      `class_members`; leave `encode_shape`/`encode_query` un-aliased; flip the predicate's
      shared-representation arm `Refuse`→`ConfirmOnly` and correct its "why disjointness is the
      proof obligation" doc section. Fixture: a two-table grammar sharing a spelling where a
      second-table rule must fire on first-table material — i.e. one that LOSES an analysis today
      (`bistratal-overlapping-segment-representation`'s recall-side counterpart).

## 5. Escalate NEEDS-DECISION and NEEDS-ORACLE items (hand-off)

- [x] 5.1 Record a human/architect decision (new short ADR or dated `STAGING.md` note) on: `Metathesis`'s
      from-scratch RTL swap construction (build it, or declare a permanent scope boundary?);
      `QuantifierPattern`'s genuinely-unbounded case (structurally infeasible, or simply unattempted?);
      whether either, if greenlit, also warrants a C#-oracle re-verification pass
      — **DONE 2026-07-25**, record: `docs/conformance/needs-decision-resolutions.md`. Both rows resolve
      **PROVABLE, build, no carve-out**; neither needs a C#-oracle precondition (`pangloss` is the oracle
      per the standing `conformance-grammars` rule). `SimultaneousRewrite`'s overlap case stays
      oracle-blocked (5.2), unswept. The two builds become 4.5 and 4.6 below.
- [ ] 4.5 `QuantifierPattern` unbounded (`max == -1`): `Slot::Repeat.max` → `Option<u32>`; `render_slots`
      emits `[inner]*` (min 0) / `[inner]^>{min-1}` (min ≥ 1) instead of `^{min,max}`;
      `MAX_QUANTIFIER_BOUND` applies to finite bounds only; audit every finite-max reader for a
      no-finite-max path (never a defaulted number). `slot_candidates` still refuses `Slot::Repeat`
      (unchanged, honest). Fixture: `unbounded-iterative-quantifier-expansion`
- [ ] 4.6 `Metathesis` `Dir::RightToLeft`: drop `compile_metathesis_rule`'s `Dir::LeftToRight` early
      return in favour of the mirror-and-reverse construction (`reversed_slots` + mirrored
      `left_switch`/`right_switch` remap + `fsm_reverse` + `fsm_union` with the plain net), mirroring
      `compile_rtl_branch_net`. Fixture: `right-to-left-metathesis-reversal`
- [x] 5.2 **DONE 2026-07-26** — `SimultaneousRewrite`'s overlapping-subrule configuration is now
      independently verified against `hc.dll`. The premise that this needed
      `add-reference-hermitcrab-parity` §§2-5 built first was **wrong**: `hc.dll` builds from the pinned
      submodule in ~3 s, and `machine/conformance/adapters/hc-dotnet-wrapper.sh` already bridges it to
      PROTOCOL.md §7's 3-arg contract, so the harness was substantially already there and the "zero
      code exists" note was stale. Fixture:
      `conformance-staging/edge-cases/simultaneous-subrule-genuine-overlap`, the repo's first with
      founding-oracle ground truth. `hc.dll` compiled the overlapping grammar cleanly and its
      `(word, signature)` output is byte-identical to `pg_parse::Morpher`'s on all 9 words — and the
      agreement discriminates resolution order (`be` analyzes, `de` does not) rather than being a
      shared silence on the underlying form. ADR 0001's oracle-trust blocker for this configuration is
      discharged; the proposer-side refusal stands unchanged, being a construction question.

## 6. The finish line (hand-off)

**Status 2026-07-25.** The cross-check now reports **20 rows / 20 Covered / 0 Uncovered / 0
Unmappable** — the four blocking rows were not missing evidence, their `exercises:` tags were
characteristic NAMES rather than `constructs.txt` row ids and so credited nothing (fixed, plus
`tests/exercises_tag_liveness.rs` to gate the class). `Unmappable` reached zero via
sillsdev/machine#465.

**The flip is deliberately NOT done yet, and 20/20 is not the reason to hold — this is:** four row
ids are each mapped by TWO characteristics, so the finer one can report `Covered` on its coarser
sibling's evidence. I hand-verified all four are genuinely evidenced today (record + citations:
`docs/conformance/shared-construct-id-analysis.md`), so the number is true, **but three of them are
not mechanically checkable and so can decay into a false claim with nothing failing.** A green
build-breaking gate that can silently start lying is worse than an advisory report, because the green
light is what gets cited — and it would sit next to a documented census of open circumfix gaps.

- [x] 6.0 **DONE 2026-07-25** — `rust/crates/pg-foma/tests/structural_witness_gate.rs`. All three
      predicates read the loaded `pg_grammar` model, not raw XML; the circumfix one calls the
      compiler's own `emit::classify_affix` over EVERY allomorph. Witnesses:
      `polysynthetic-stratal-derivation-chain` / `suffixing-vowel-harmony` /
      `fusional-realizational-morphology`, each confirmed structurally matching AND tagging the
      shared id on a *passing* word. Shared-id list computed from `construct_ids_for`; the MPR pair
      is excluded by derivation (it needs `RefusalWitness`), not by assertion. Proven non-vacuous
      four ways. Honest detail: narrowing the circumfix predicate to allomorph-0-only still passed,
      so census C1's preemption does not affect qualification today — the all-allomorph scan is
      correct-by-design rather than load-bearing.
- [x] 6.2 **DONE 2026-07-25 — THE FLIP.** `tests/conformance_coverage_gate.rs`'s
      `supported_construct_conformance_coverage_has_no_gaps` now fails the build on any `Uncovered`
      or `Unmappable` row, with per-disposition messages naming the two likely causes. Non-vacuity is
      itself asserted (the report must enumerate every `CharacteristicKind`), and the gate was proven
      to bite: sabotaging one `Proven` row's tag produced `COVERAGE REGRESSION (Proven):
      [SubruleGating] …`, reverted clean, all four coverage gates re-run green.
- [x] 6.1 **DONE 2026-07-26** — re-ran the report against the real corpus:
      `cargo test -p pg-foma --test plan_interaction_coverage_gate -- --nocapture` reports **7/7
      required tuples Covered, 0 Uncovered, 0 ContainsUnsupported**, `unexpected_tuples` empty, 2
      `retired_interactions()`. The 7-shape set's closure was re-verified against `plan.rs`'s 5
      `PlanNodeKind` variants (`Leaf`/`Compose`/`Union`/`Gate`/`Replace`) and `enumerate.rs`'s own
      "Shape" doc + actual node-construction call sites — `enumerate_default` still only ever builds
      the same 7 edges (no `GuardAutomaton` leaf is ever constructed, consistent with it being absent
      from the legal set). Both retirements re-checked against current code, not assumed: retirement 1
      (`mpr-group.append-output` × `unordered-application`) against `cover-mpr-groups design.md` D4's
      "load-bearing, not open" text (unchanged) plus `capability.rs`'s current disposition table
      (`MprGroupAppend` still `ConfirmOnly`, `MprGroupOverwrite` still `FailClosed`); retirement 2
      (Gate-group sibling reordering) against `gate.rs`'s "why the union is safe here" doc,
      `build.rs`'s `union_checked` call site, and `oracle.rs`'s `permute_gate_groups` +
      `differential_oracle_agrees_on_permuted_gate_groups_of_the_same_grammar` — all still present
      and unchanged. Fuzz slice (deliverable 5): 6 multi-group fixtures checked, all `Agree`, 22
      single/no-group fixtures skipped, 0 unloadable.
- [x] 6.3 **DONE 2026-07-26 — THE FLIP.** `tests/plan_interaction_coverage_gate.rs`'s
      `plan_interaction_coverage_has_no_uncovered_required_tuples` (renamed from
      `..._report_advisory`) now fails the build if `report.uncovered().is_empty()` does not hold,
      naming the specific uncovered `AdjacencyTuple`(s). Pre-flip criteria established, not assumed:
      (1) zero `Uncovered` required tuples, confirmed live (6.1 above); (2) non-vacuity —
      `report.required.len() == 7` and `report.retired.len() == 2` are pinned literal-count
      assertions (not derived from the functions under test, so a silent shrink/grow of either set
      fails loudly), plus `unexpected_tuples.is_empty()` and a non-empty discovered-fixture corpus,
      all already present and now paired with the new hard assertion; (3) no inherited/unfalsifiable
      coverage — analysed and found NOT to reproduce the sibling gate's shared-construct-id defect:
      every `AdjacencyTuple` is already this module's own finest-grained unit (no coarser sibling
      tuple exists for a finer one to borrow evidence from, unlike `constructs.txt`'s coarser ids),
      and `compute_interaction_coverage` credits a tuple only from an actual parent-child edge
      present in a caller-supplied, per-fixture reified `Plan` — never from mere co-presence of both
      node kinds in the same grammar. The one flagged judgment call (non-rule-keyed characteristics
      folding onto the single representative `Gate` node grammar-wide, not per-branch) only pushes
      classification toward `Uncovered`/`ContainsUnsupported`, never toward a false `Covered`, so it
      cannot make the gate lie in the dangerous direction. Full reasoning recorded in this module's
      own top-doc ("Why this report cannot silently start lying the way the sibling gate could").
      (4) Proven non-vacuous by sabotage: temporarily skipped crediting the
      `(Replace, Leaf/RewriteRule)` tuple in `compute_interaction_coverage`'s per-fixture loop →
      `COVERAGE REGRESSION: 1 required adjacency tuple(s) ... [AdjacencyTuple { parent_kind:
      "Replace", child_kind: "Leaf", child_detail: Some("RewriteRule"), compose_strategy: None }]`;
      reverted (`git diff` clean, no sabotage remnants); re-ran green (7/7 Covered again). Module
      top-doc and the gate test's own top-doc updated from ADVISORY-FIRST to BUILD-BREAKING; the
      stale `uncovered()` doc comment (which claimed `ContainsUnsupported` rows were included) was
      also corrected to match the method's actual `TupleStatus::Uncovered`-only filter.
- [ ] 6.4 Confirm the definition of done (design.md D7): zero un-evidenced ledger rows, zero
      `Unmappable` rows, zero unresolved NEEDS-DECISION rows, both gates build-breaking
