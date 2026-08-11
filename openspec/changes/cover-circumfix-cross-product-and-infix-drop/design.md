# Design — cover-circumfix-cross-product-and-infix-drop

## Context

Production pipelines being changed:
- `pg_grammar::compile::compile_project` → `lexicon::build` → `affixes::build_affix_rule`
  (the fwdata/snapshot compiler; NOT the legacy HC-XML `pg_grammar::load`, which already handles
  circumfix-shaped `MorphologicalRule`s generically).
- `pg_foma::emit` structural-composite candidate selection (`is_structural_rule`,
  `structural_candidate_rules`, `composite_candidate_rules`) and
  `pg_foma::capability::CircumfixStructuralCompositePredicate`'s ground truth.

Exclusive ownership: this change owns `pg-grammar/src/compile/affixes.rs`, the
`is_structural_rule`/candidate-selection region of `pg-foma/src/emit.rs`, and
`CircumfixStructuralCompositePredicate` + its tests in `pg-foma/src/capability.rs`. The sibling
perf change (`surface-compile-profile-and-templated-routing`) owns `probe_would_refuse` and
`pg-cli/src/fst_health.rs`; the two changes touch adjacent `emit.rs` regions and are serialized —
this change merges first (see STAGING note in tasks.md).

## D1. Where the new FST capability lives — decision and comparisons

**Decision: the structural-composite home (`build_structural_composites`), entered by widening
`is_structural_rule` with an `Role::Infix => any-allomorph-drops` arm, with a mandatory ownership
handoff out of `crate::preexpand`'s candidate set.** A cheap verification probe of the alternative
"preexpand already covers it" hypothesis runs first (task 3.1) because if TRUE it collapses the
work to a predicate-ground-truth fix; the motivating grammar's own health data (73 uncovered
constructs including the refused rule's allomorphs) predicts it is FALSE.

| Candidate home | What it would take | Recall-proof obligation | Entry-budget cost | Verdict |
|---|---|---|---|---|
| **(chosen) `build_structural_composites` via `is_structural_rule` widening** | One match arm (`emit.rs:1809-1818`) mirroring the existing `None\|Prefix\|Suffix` drop-aware arm + ownership handoff + doc updates | Oracle containment fixture in `tests/phase_c_circumfix.rs` style (generator + `Morpher` sweep, 100% recall) — the builder itself is already role-agnostic and resynthesizes via the real engine (`pg_rules::morph::synthesize`), the same faithfulness argument the predicate already accepts for `CircumfixPrefix`/`Process` | **Flat** with handoff: entries *move* from preexpand to structural rather than duplicating; without handoff it double-counts (rule is `Role::Infix`, so preexpand admits it unconditionally today) | Smallest delta onto proven machinery; exact precedent = commit `18e6835` + census C1/C3 |
| Credit `crate::preexpand`'s existing Infix coverage in the predicate (`structural_composite_attempted` widened to "any faithful mechanism") | Zero new construction code; predicate ground-truth change at `capability.rs:1162` | NEW containment fixture proving preexpand's Infix mechanism is exact *for a drop-shaped allomorph* (its existing tests only cover non-dropping interdigitation), PLUS evidence the rule actually lands in `covered_infix_rules` for real data | Zero | **Verify-first fallback** (task 3.1); predicted false by the motivating grammar's uncovered list, and preexpand coverage is bounded by `MAX_EXTRA_RULES`/budget, making a general recall claim harder, not easier |
| `TemplatedUnderlyingTokens` (P6) path (`structural_allomorph.rs` extension) | New pipeline stage: today's layer handles exactly one shape (`lhs=[var,tail]`, RHS starting `Copy(Input(0))`) — circumfix/infix-drop shapes are new construction, and the whole `CircumfixOutputAction` characteristic is `RepresentsWithKnownGap` there | A new faithfulness argument for a *structural FST composition* (not engine-backed resynthesis) — strictly harder than the chosen option's | N/A (no shared enumeration budget) | Out of proportion for this fix; belongs to the perf change's follow-on if the templated backend becomes the selected strategy for cascade-family grammars |
| Peeler (`ReduplicationPeeler` pattern, propose-side) | New peeler keyed on drop shapes | Peel-exactness proof per depth (the reduplication peeler is only proven at depth 1) | Zero | Trigger mismatch: peelers invert a *surface-copying* operation; an LHS-drop deletes material, which is not peelable without the lexicon — structurally wrong tool |
| Per-rule propose-side enumeration fallback | New architectural category | Unbounded | Zero | Violates the standing FST-propose + HC-confirm decision; rejected |
| New backend | A fourth compiler | Everything | — | Not warranted: the construct fits proven machinery; see the perf change for the backend question at scale |

## D2. Ownership handoff (budget + double-compile prevention)

A rule newly admitted by the `Infix`-with-drop arm must be removed from
`composite_candidate_rules().preexpand_candidates` (the C3 pattern, `emit.rs:2256-2276`), pinned
by a handoff test mirroring `circumfix_infix_ownership_handoff_is_clean`
(`tests/circumfix_candidate_selection.rs`). Without this, the same (root, rule) pairs are
enumerated by BOTH mechanisms; the motivating grammar already sits 0.4% over the default
200k entry budget, so duplication is a regression, not a rounding error. Consequence to preserve:
`covered_infix_rules` bookkeeping for such rules moves to the structural report — the
uncovered-clearing at `emit.rs:3140-3150` must still clear them.

## D3. Predicate semantics after the fix

- `structural_composite_attempted` becomes true for Infix-with-drop rules via the widened
  `is_structural_rule` (no predicate re-derivation — it keeps reading the same fact the compiler
  branches on).
- Verdict for the observed construct: `Refuse → ConfirmOnly`. No admission-filter claim is made
  (same landing spot as every other ConfigPredicate characteristic).
- **No new `CharacteristicKind`.** The existing `CircumfixOutputAction` detail already fires for
  this shape; the coverage row is the shared `constructs.txt` id
  (`AffixProcessRule: prefix/suffix/circumfix/infix`). The existing negative test
  `circumfix_output_action_predicate_refuses_infix_role_drop` flips to a positive pin;
  a narrower negative (e.g. `Role::Reduplication` + drop) replaces it as the remaining fail-closed
  boundary.
- Rollback: reverting the match arm restores `Refuse` — fail-closed in both directions.

## D4. The pg-grammar cross-product lowering (HCLoader.cs:1048-1332 transcription)

- Guard unchanged: `lexeme_morph_type == Circumfix` (already correctly detected; only the
  consequence changes from drop to build).
- Pairing pool: `AlternateForms` only, filtered by exact `MorphType::Prefix` / `MorphType::Suffix`
  (NOT `shape_of`'s broader interfix/clitic classes — must match HCLoader's guid filter).
  Loop nesting order (prefix outer, suffix inner, prefix-env, suffix-env) is load-bearing: it fixes
  the allomorph index order that `pg-rules/validity.rs`'s disjunctive-ordering re-check keys off.
- LHS: one flat pattern — `AnyPlus()` when no envs; else
  `[PrefixNull + prefix-env.right-nodes] AnyStar [suffix-env.left-nodes + SuffixNull]`, with
  prefix-env.left / suffix-env.right becoming ONE external `EnvironmentDef { require: true, .. }`.
  All primitives exist in `environment.rs`.
- RHS: `[insert_segments("{pfx}+"), Copy(Input(0)), insert_segments("+{sfx}")]` — classifies
  `Role::CircumfixPrefix`, so the FST path is recall-safe for these rules with zero pg-foma change.
- Faithful quirks preserved, NOT fixed (flag in comments with a link, per comment policy):
  (a) MPR features read from the **prefix** allomorph's inflection classes only;
  (b) NO `required_syn_fs`/ms_env_features on circumfix allomorphs (C# never sets them here).
- Degenerate data: empty prefix-group or suffix-group → warn + `None` (C# yields zero allomorphs;
  we make the drop loud). Slot registration must go through the same `acc.slot_rules` push as
  ordinary inflectional rules so the owning templates start working — that is the user-visible
  point of the fix.
- U+25CC dotted-circle stripping: verify `insert_segments`' input path already inherits the
  importer's stripping (`pg-fwdata/src/node.rs:187-189`); add a test rather than a second strip.

## D5. Fixture strategy (two constructs, three test surfaces)

1. **Staged conformance fixture** `edge-cases/circumfix-cross-product-and-infix-drop`
   (synthetic-only): a 2-prefix × 2-suffix cross-product rule authored as 4 HC-XML subrules
   (the target lowering shape), plus one Infix-with-drop rule; words.yaml pins oracle behavior,
   including the discriminating 2×2 combinatorial case and an `expect_fail` negative control.
2. **Companion FST-reachability test** (pg-foma, `circumfix_candidate_selection.rs` sibling):
   asserts every oracle analysis is reachable in the compiled net. The Infix-with-drop word is
   the undergeneration witness — **red before the D1 fix, green after**; TDD ordering is
   mandatory.
3. **pg-grammar unit tests** own gap 1's regression pin (the conformance XML front end cannot
   express the LCM cross-product): the existing warn-test flips into a positive lowering
   assertion, plus env-cross-product and 2×2-ordering cases, plus an XML-loader parity test
   (same semantic circumfix via snapshot-compile and via HC-XML load → structurally equal defs).

## D6. Error/timeout/enforcement posture

No change to: capability gate default-enforcement, `--allow-unproven` semantics, word timeouts,
enumeration budget defaults (the motivating grammar's just-over-budget state is pinned by the
existing `mbugwe_exceeds_the_default_entry_budget` regression test and is the perf change's
subject). A grammar-authored contradictory env pair (prefix env claiming empty stem × non-empty
suffix left context) builds faithfully and never matches — same as C#.
