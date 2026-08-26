# Conformance coverage sweep -- 2026-08-19

Measurement sweep of construct-level conformance coverage, run in a fresh worktree
(`sweep-conformance-coverage`, branch `sweep-conformance-coverage`, based on local `main` at
`59c19eeb`). Companion to a parallel five-corpus recall/FST-size sweep run independently in another
worktree; this sweep covers `pangloss coverage` and the conformance-fixture test suite only, per
`docs/superpowers/specs/2026-08-11-candidate-filter-contract.md` and
`docs/fst-plan/candidate-filter-assessment.md`'s state as of today, and cross-checks three specific
findings from `docs/fst-plan/conformance-fst-measurement.md` section 7-8 against the current codebase.

## 1. `pangloss coverage` -- per-`CharacteristicKind` disposition and conformance coverage

Ran via `pg.ps1 -Mode run -Bin pangloss -- coverage --json coverage-report.json` (walks both
fixture roots: `discover_scoped(ConformanceScope::All)` internally, so this already covers
`conformance-staging/**` and `machine/conformance/**`).

**Headline: 23/23 constructs Covered by a passing fixture, 0 Unmappable.** 6 Proven, 6 ConfirmOnly,
11 ConfigPredicate.

There are now **23** `CharacteristicKind` variants (not 19 -- that count, and the "45 fixtures"
figure, both come from `docs/fst-plan/conformance-fst-measurement.md`, which is now stale on both
numbers; see section 3 below for why). Current fixture-root sizes: **25** fixtures under
`conformance-staging/edge-cases/**` (local scope) and **33** under `machine/conformance/{edge-cases,
languages}/**` (the upstream half of `-Scope all`), for 58 total -- up from the brief's cited 25+21=46.

| `CharacteristicKind` | Disposition | Conformance status | Construct id(s) | Containment evidence |
|---|---|---|---|---|
| `Affixation` | Proven | Covered | AffixProcessRule: prefix/suffix/circumfix/infix; subtraction/truncation | general_pervasive: f1/f2/f4 gates |
| `RealizationalMorphology` | ConfirmOnly | Covered | RealizationalAffixProcessRule | dedicated: `cover_realizational_morphology_constraints.rs` |
| `Compounding` | ConfigPredicate | Covered | CompoundingRule | dedicated: `cover_compounding.rs` |
| `OrderedMorphRuleApplication` | Proven | Covered | Stratum (Linear/Unordered rule order) | general_pervasive: `phase_c_strata_depth.rs`, f1/f4 |
| `UnorderedMorphRuleApplication` | ConfigPredicate | Covered | Stratum (Linear/Unordered rule order) | dedicated: `cover_unordered_morph_rules.rs` |
| `MprGroupAppend` | ConfirmOnly | Covered | MPR features/groups | dedicated: `cover_mpr_groups.rs` |
| `MprGroupOverwrite` | ConfigPredicate | Covered | MPR features/groups | dedicated: `cover_mpr_groups.rs::overwrite_group_composes_to_confirm_only` |
| `IterativeRewrite` | Proven | Covered | RewriteRule Iterative | general_pervasive |
| `SimultaneousRewrite` | ConfigPredicate | Covered | RewriteRule Simultaneous | dedicated: `phase_c_simultaneous.rs` |
| `LeftToRightRewrite` | Proven | Covered | RewriteRule direction: left-to-right | general_pervasive |
| `RightToLeftRewrite` | ConfigPredicate | Covered | RewriteRule direction: right-to-left | dedicated: `phase_c_right_to_left.rs` |
| `Metathesis` | ConfigPredicate | Covered | MetathesisRule | dedicated: `phase_c_metathesis.rs` |
| `Epenthesis` | ConfigPredicate | Covered | RewriteRule Iterative | dedicated: `epenthesis_structural_route_containment.rs` |
| `SubruleGating` | Proven | Covered | RewriteSubruleDef gating | dedicated: `p6_gate_parity.rs`, `phase_c_partition_k.rs` |
| `CircumfixOutputAction` | ConfigPredicate | Covered | AffixProcessRule: prefix/suffix/circumfix/infix | dedicated: `phase_c_circumfix.rs` |
| `Reduplication` | ConfigPredicate | Covered | AffixProcessRule: reduplication | dedicated: `f6_reduplication_peel_chain_depth.rs`, `f4_composite_gate.rs` |
| `CoOccurrenceConstraint` | ConfirmOnly | Covered | MorphemeCoOccurrenceRule/AllomorphCoOccurrenceRule | dedicated: `cover_realizational_morphology_constraints.rs::morpheme_co_occurrence_exclude_anywhere_over_propose_confirm_prune` |
| `NaturalClassDefinition` | Proven | Covered | NaturalClass: Segments vs FeatureNaturalClass/SegmentNaturalClass | (no containment field -- representational only) |
| `MultiTable` | ConfigPredicate | Covered | CharacterDefinitionTable: >1 table | dedicated: `phase_c_multi_table.rs`, `two_table_symbol_divergence.rs` |
| `QuantifierPattern` | ConfigPredicate | Covered | CharacterDefinitionTable pattern shapes | dedicated: `phase_c_quantifier.rs` |
| `StemName` | ConfirmOnly | Covered | Stem names | dedicated: `cover_realizational_morphology_constraints.rs::stem_name_gating_over_propose_confirm_prune` |
| `FreeFluctuation` | ConfirmOnly | Covered | Disjunctive allomorphs / free-fluctuation | (no containment field) |
| `ProcessMorphology` | ConfirmOnly | Covered | MorphologicalOutputAction: ModifyFromInput/InsertSimpleContext | (no containment field) |

**On "how many fixtures exercise it" specifically:** `pangloss coverage`'s own matching is at
construct-id granularity, and several `CharacteristicKind`s intentionally *share* a construct id
(e.g. `Affixation` and `CircumfixOutputAction` both map to "AffixProcessRule:
prefix/suffix/circumfix/infix"; `OrderedMorphRuleApplication`/`UnorderedMorphRuleApplication` both
map to "Stratum (Linear/Unordered rule order)"; `MprGroupAppend`/`MprGroupOverwrite` both map to "MPR
features/groups"). A fixture-count computed per `CharacteristicKind` by re-grepping `exercises:` tags
would therefore either double-count across sibling kinds or require inventing a finer-grained parallel
match the tool itself does not make -- exactly the coarse-constructs.txt-vs-fine-`CharacteristicKind`
gap the repo's own history already flags (memory: "Coverage gate inheritance trap"). Reporting the
tool's own per-kind ledger (above) rather than a hand-rolled count avoids inventing a second, possibly
inconsistent, data source, per `coverage.rs`'s own module doc ("never inventing a parallel data source
or count").

## 2. Full conformance test suite, `-Scope all`

Run via `pg.ps1 -Mode conformance-test -Scope all` in the `sweep-conformance-coverage` worktree
(whole-workspace `cargo nextest run`, `pg-test-opt` profile).

**Result: 2001 tests run, 2001 passed (1 slow), 161 skipped, exit code 0. Zero failures.**

The one "slow" test (163.767s, flagged at the 60s/120s thresholds but not a failure) is
`pg-ffi::header_abi::installed_header_compiles_links_and_runs_as_c_and_cpp` -- a real C/C++ toolchain
invocation (documented in `CLAUDE.md` as one of the heavier per-test costs on this machine), not a
conformance-suite test. The 161 skips are the ordinary corpus-gated/ignored tests that need private
corpus data or explicit opt-in flags this task did not need; none are conformance-fixture tests (the
fixture-driven suite has no `#[ignore]`d cases by design, per `CLAUDE.md`'s note that
`conformance_fixtures_gate` is deliberately part of the ordinary suite).

Every conformance-specific gate searched for by name passed, including the ones most directly on
point for this sweep:

| Test | Result |
|---|---|
| `pg-parse::conformance_fixtures_gate::all_discovered_fixtures_match_oracle` | PASS |
| `pg-parse::conformance_fixtures_gate::w91_affix_shapes_covered_by_upstream_fixtures` | PASS |
| `pg-parse::conformance_fixtures_gate::graduation_guard_no_duplicate_fixture_names` | PASS |
| `pg-foma::conformance_coverage_gate::supported_construct_conformance_coverage_has_no_gaps` | PASS |
| `pg-foma::p6_gate_parity::synthetic_pos_gate_matches_oracle` | PASS |
| `pg-foma::p6_gate_parity::ungated_cascade_would_have_missed_the_noun_entry` | PASS |
| `pg-foma::plan_interaction_coverage_gate::plan_interaction_coverage_has_no_uncovered_required_tuples` | PASS |
| `pg-foma::plan_interaction_coverage_gate::gate_group_reordering_agrees_on_every_multi_group_corpus_fixture` | PASS |

`supported_construct_conformance_coverage_has_no_gaps` passing independently corroborates section 1's
`pangloss coverage` finding (23/23 constructs Covered) from inside the test suite itself, rather than
only from the CLI report. No investigation of failures was needed -- there were none.

## 3. Three cross-checks against `docs/fst-plan/conformance-fst-measurement.md` section 7-8

All three of the doc's flagged issues are **already resolved in current code** -- the doc has gone
stale since it was written.

### 3a. `MprGroupOverwrite`'s Refuse/ConfirmOnly contradiction -- RESOLVED, not touched further

The doc's central claim (section 7, section 12 gap #1): `MprGroupOverwriteFailClosedPredicate::evaluate` never
returns `Refuse`, contradicting ~10 doc comments across 6 files and the predicate's own name, plus one
shipped unit test whose doc comment said "must compose to Refuse" directly above an assertion that it
equals `ConfirmOnly`.

**Current state, verified by direct read of `rust/crates/pg-foma/src/capability.rs`:**

- The struct is now named `MprGroupOverwritePredicate` (not `...FailClosedPredicate`) -- grepped the
  old name across all of `rust/`: zero matches anywhere.
- Its doc comment now states plainly: "an observed `Overwrite` group rests at
  `PredicateVerdict::ConfirmOnly`" and adds a dedicated "Why this can never be promoted to `Admit`"
  section explaining the soundness argument (a later rule application can retract exactly the
  feature an earlier one added, so an FST filter looking at one transition cannot reconstruct the
  history-dependent accumulated set) -- never mentions Refuse for this construct at all.
- The previously-contradictory test's doc comment now reads: "The mirror image: an
  `MprGroupOutput::Overwrite` group alone also composes to `ConfirmOnly`, never `Admit`" -- consistent
  with its own assertion.
- A grep across all of `rust/` for `MprGroupOverwrite` near `permanent`/`unconditional`/`Refuse` finds
  matches in exactly one file (`capability.rs`), and that file's own summary comment (near line 3366)
  now says explicitly: "All four now have real predicates (..., `MprGroupOverwritePredicate`) that
  each scan `CharacteristicsProfile` directly rather than unconditionally refusing."
- `pangloss coverage`'s own ledger (section 1 above) reports `mpr_group_overwrite` as `ConfigPredicate` /
  `Covered`, backed by `cover_mpr_groups.rs::overwrite_group_composes_to_confirm_only`.

This is exactly the doc's own cheapest gap-#1 remedy ("rename the predicate, fix ~10 comments,
correct one test doc string") -- already applied. **No code change needed; not touched, per the
brief's instruction not to touch the Refuse/ConfirmOnly question.** The measurement doc itself is now
stale on this point and should be corrected or retired if it continues to circulate; that is a
documentation follow-on, not something this sweep did.

### 3b. Ablaut/"process" morphs' taxonomy gap -- RESOLVED

The doc's claim (section 8 gap #3): a single-part `Modify`-only allomorph (ablaut/mutation/simulfix)
reaches the same `O(roots x rules^depth)` `build_structural_composites` enumeration mechanism as
circumfix/reduplication, but has no `CharacteristicKind` of its own -- invisible to
`CircumfixOutputAction`'s predicate (which requires LHS-material-drop) and untracked by the 19-variant
taxonomy.

**Current state:** `CharacteristicKind::ProcessMorphology` exists (`capability.rs`, added by commit
`626ebf06` "capability: give process morphology a characteristic, and claim the construct it always
had"), documented as "A `Modify`-only allomorph: the input is mutated in place rather than affixed to
(ablaut, mutation, simulfix). Distinct from `CircumfixOutputAction`, which cannot fire here at all."
It is in `CharacteristicKind::ALL` (23 variants total now), has `default_disposition() ==
ConfirmOnly`, is mapped in `conformance_coverage.rs` to construct id "MorphologicalOutputAction:
ModifyFromInput/InsertSimpleContext", and `pangloss coverage` reports it `Covered`. There is also now
a dedicated upstream fixture, `machine/conformance/edge-cases/process-morphology-in-place-mutation`.

No predicate is registered for it (it carries no discharging predicate, same as
`CoOccurrenceConstraint`/`StemName`/`FreeFluctuation` -- all `ConfirmOnly`-by-default with no admission
filter), which is architecturally honest: the doc's own point was that this construct was
*structurally invisible*, not that it lacked a filter. It is no longer invisible. **Report-only
finding, no action taken** (adding/removing a predicate here would be a design decision, not asked
for).

### 3c. `MorphemeCoOccurrence`/`AllomorphCoOccurrence` zero conformance-fixture coverage -- RESOLVED

The doc's claim (section 7, section 8 gap #4, section 11): zero of "the 45 fixtures" (this report's
assignment) declare a `MorphemeCoOccurrenceRule`/`AllomorphCoOccurrenceRule` element; the only place
`adjacency="anywhere"` is exercised is `capability.rs`'s own unit tests.

**Current state:** grepped `<MorphemeCoOccurrenceRule` / `<AllomorphCoOccurrenceRule` (actual element
tags, not just the string) across both fixture roots. Real, active declarations exist in:

- `machine/conformance/languages/templatic-root-modification/grammar.xml` -- one `exclude`,
  `adjacency="anywhere"`.
- `machine/conformance/languages/suffixing-evidential-adjacency-chain/grammar.xml` -- five
  `MorphemeCoOccurrenceRule`s (`exclude` x2 `anywhere`, `require` x3 across
  `somewhereToLeft`/`adjacentToLeft`/`somewhereToRight`/`adjacentToRight`) plus one
  `AllomorphCoOccurrenceRule` (`exclude`, `anywhere`).
- `machine/conformance/edge-cases/morphotactic-attribute-breadth/grammar.xml` -- two
  `MorphemeCoOccurrenceRule`s (one `isActive="no"`) and six `AllomorphCoOccurrenceRule`s covering
  every adjacency mode, `anywhere` included.

All three are under `edge-cases`/`languages` -- the categories `pg_conformance_fixtures::discover`/
`discover_scoped` actually walk -- so they count toward `-Scope all` and toward `pangloss coverage`'s
`CoOccurrenceConstraint` row, which is reported `Covered` above via a dedicated Rust-side containment
test (`cover_realizational_morphology_constraints.rs::morpheme_co_occurrence_exclude_anywhere_over_propose_confirm_prune`).

Separately (not counted by `discover`/`discover_scoped`, since it lives under a third category,
`filter-passes/`, that those functions do not walk): `conformance-staging/filter-passes/co-occurrence/`
is a dedicated synthetic fixture built for the candidate-filter project specifically, also exercising
both exclusion and requirement with `adjacency="anywhere"`, run by
`candidate_filter_fixture_weight.rs` rather than `conformance_fixtures_gate`.

**No new fixture was authored -- the gap the brief asked me to consider filling is already closed**,
both by the conformance corpus proper (three upstream grammars) and by the dedicated Rust containment
test the coverage ledger cites. The doc's "zero of 45 fixtures" claim was scoped to "this report's six
assigned fixtures for this [feature/unification-gate] family," which is narrower than "the whole
corpus" -- its own generalizing sentence ("this construct has zero conformance corpus coverage today")
overstated that narrower finding, and in any case the corpus has grown since.

## 4. Bugs found / fixed

None. Every concrete issue in scope (3a/3b/3c) was already resolved in the current `main`-based
worktree; no regression or contradiction was found in `pangloss coverage`'s own output (23/23 covered,
consistent ledger) or in the capability/taxonomy code read during verification.

## 5. Flagged but not touched

- **`docs/fst-plan/conformance-fst-measurement.md` is now stale** on: the `CharacteristicKind` count
  (19 -> 23), the fixture-root sizes (25+21=46 -> 25+33=58), and all three of its own section 8/12 gaps
  #1, #3, #4 (all closed). It is a dated research artifact, not a spec, so this sweep did not edit it --
  flagging for whoever owns that doc to decide whether to retire, correct, or annotate it.
- **The doc's gap #2 and #5** (whether `capability.rs`'s `RightToLeftRewrite`/`Metathesis`/
  `SimultaneousRewrite`/`QuantifierPattern`/`MultiTable` verdicts describe the `replace.rs`/`gate.rs`
  prototype rather than the shipped `emit.rs` mainline path, and the architecture decision about
  whether to wire the prototype in) are **out of this sweep's scope** -- real, open, and unrelated to
  the three specific cross-checks the brief named. Not investigated further here.
- **3a's Refuse-vs-ConfirmOnly promotion question itself** was already made and shipped (code now
  consistently documents `ConfirmOnly`, never `Refuse`, for `MprGroupOverwrite`) -- per the brief's
  explicit instruction, this sweep did not re-open or second-guess that decision.

## 6. Worktree

- Path: `C:\Users\johnm\Documents\repos\PanGloss\.claude\worktrees\sweep-conformance-coverage`
- Branch: `sweep-conformance-coverage`, based on local `main` at `59c19eebc4bb023eb34ed5a13630972e1bfc6cad`
- No corpus-backed suites were run (not needed for this task); `machine/conformance` submodule
  auto-initialized (sparse, ~3.6MB) on first `pg.ps1` invocation as documented.

