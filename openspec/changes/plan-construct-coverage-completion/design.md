## Ground truth this plan is built from (read, not assumed)

`cargo build -p pg-cli --release && ./rust/target/release/pangloss coverage` (2026-07-25, this
change's own verification run): **20 `CharacteristicKind`s** (`capability.rs::CharacteristicKind::ALL`)
— 6 `Proven`, 3 `ConfirmOnly`, 10 `ConfigPredicate`, 1 `FailClosed`; 12/20 rows name a registered
discharging predicate (`default_registry()` — exactly 12 predicates registered); 19/20 rows have curated
containment-test evidence (`coverage_ledger.rs::containment_evidence_for`; the one exception,
`NaturalClassDefinition`, is a deliberately pinned `None` — representational-only, no capability
implication); 16/20 rows map to a `machine/conformance/constructs.txt` id and are `Covered` by a passing
fixture; 4 rows (`LeftToRightRewrite`, `RightToLeftRewrite`, `SubruleGating`, `MultiTable`) are
permanently `Unmappable` — no `constructs.txt` row exists for them at all
(`conformance_coverage.rs::construct_ids_for`'s own doc names the near-miss rows considered and
rejected for each).

**`machine/conformance/constructs.txt` is 43 lines total** (`wc -l`), of which **25 are actual
construct-checklist rows** (lines 19-43; lines 1-18 are header/comment). Only **13 of those 25 rows**
are referenced by `construct_ids_for` at all (several `CharacteristicKind`s share one row, e.g.
`Affixation`+`CircumfixOutputAction` both cite `"AffixProcessRule: prefix/suffix/circumfix/infix"`,
`OrderedMorphRuleApplication`+`UnorderedMorphRuleApplication` both cite `"Stratum (Linear/Unordered rule
order)"`). The other **12 `constructs.txt` rows have no `CharacteristicKind` at all** — see "The
`constructs.txt` mismatch" below.

**Non-`Proven` count is 14, not 13.** `default_disposition`'s own match: `Affixation`,
`OrderedMorphRuleApplication`, `IterativeRewrite`, `LeftToRightRewrite`, `SubruleGating`,
`NaturalClassDefinition` are the 6 `Proven` rows; the remaining 14 are
`RealizationalMorphology`, `Compounding`, `UnorderedMorphRuleApplication`, `MprGroupAppend`,
`MprGroupOverwrite`, `SimultaneousRewrite`, `RightToLeftRewrite`, `Metathesis`, `Epenthesis`,
`CircumfixOutputAction`, `Reduplication`, `CoOccurrenceConstraint`, `MultiTable`, `QuantifierPattern`
(3 + 10 + 1 = 14, matching `pangloss coverage`'s own printed disposition counts). This plan's per-
construct table below therefore covers 14 rows, not 13 — flagged here rather than silently forced to
match the assignment's rough estimate.

## Decisions

Execution order, prerequisites, and exclusive ownership are governed by `openspec/changes/STAGING.md`;
this change is not dispatchable outside that graph, and is itself pure documentation (no `.rs` file is
touched by this change).

### D1. The promotion ladder (defined once)

Per ADR 0001, a construct's **specific configuration** — not the whole `CharacteristicKind` — moves
between three states. Never the kind as a whole: 10 of the 14 non-`Proven` kinds are `ConfigPredicate`
precisely *because* they have both an evidenced-closed configuration and a `Refuse` configuration at
once, and adding evidence to the `Refuse` side never erases the split (e.g. `Compounding` will always
have a `compounding.recursive` question distinct from `compounding.non-recursive`, however well the
recursive case eventually gets covered) — so "promoted to `Proven`" is not the right target shape for
any `ConfigPredicate` kind's default disposition; only individual configurations move.

1. **`Refuse` (open gap).** No faithful (recall-preserving) construction exists for this configuration,
   or one exists but is unverified. `compose_envelope` hard-fails it; ADR 0005's override is the only
   on-ramp to force-compile and experiment.
2. **`ConfirmOnly` (closed, permanent-eligible).** A construction exists that proposes a genuine
   over-approximating superset for this configuration, oracle-verified as recall-preserving via (a) a
   dedicated containment test (`coverage_ledger.rs::containment_evidence_for`'s citation) and (b) a
   passing `machine/conformance`-shaped fixture per **configuration**, not merely per `CharacteristicKind`
   (a `ConfigPredicate` kind needs one fixture for its `ConfirmOnly` split and, separately, whatever
   evidence its `Refuse` split's own promotion below requires). This is a **legitimate, permanent**
   rest — ADR 0001: "confirm-only by default... a first-class, non-failure verdict." Full coverage does
   **not** mean promoting every `ConfirmOnly` configuration further; `RealizationalMorphology`,
   `MprGroupAppend`, and `CoOccurrenceConstraint` (the 3 unconditionally-`ConfirmOnly` kinds — no
   `Refuse` split exists for any of them at all) are architecturally confirm-dependent forever: their own
   proof obligation (a construction that is a safe superset) is already fully discharged and covered
   today, and there is nothing further for this plan to schedule for them.
3. **`Admit` (closed, optimization, non-blocking).** A *further*, separate proof that the FST
   construction itself narrows admissions with no false negative (ADR 0001: "having the FST itself
   filter admissions... is an optimization, and it carries a proof obligation... absent that proof, the
   construct is confirm-only"). `SimultaneousRewrite`'s non-overlap split is the one configuration in
   today's registry that has actually reached `Admit`
   (`SimultaneousSubruleOverlapPredicate::evaluate`, via `lower.rs`'s real span intersection —
   `subrules_pairwise_verdict`'s `Ok(())` arm returns `PredicateVerdict::Admit` directly, not
   `ConfirmOnly`) — the existence proof that this rung is reachable, not merely aspirational. Promoting
   an already-`ConfirmOnly` configuration to `Admit` is explicitly **out of scope** for "coverage
   completion": it is a performance track, never a correctness gate, and this plan's definition of done
   (D7) does not require it anywhere.

**Checkable promotion criteria**, per the task framing (ADR 0001 + `IMPLEMENTATION-READINESS.md`):
closing a `Refuse` configuration to `ConfirmOnly` requires ALL of:
- (a) a structural, over-approximating construction that a real compile path attempts (never a silent
  skip) for the configuration, with a written recall-safety argument (the "oracle verified the
  construction, the predicate reads structure" split every existing `*Predicate` doc in `capability.rs`
  already draws — see e.g. `RightToLeftRewriteFaithfulReversalPredicate`'s own doc);
- (b) a dedicated containment test proving proposer-to-confirm containment against this repo's own
  confirm engine (`pg_parse::Morpher`) for that configuration, added to `coverage_ledger.rs::
  containment_evidence_for`'s citation table;
- (c) a passing `machine/conformance`-shaped fixture (staged or upstream) exercising the newly-closed
  configuration, so `conformance_coverage.rs`'s cross-check (once fixed per D7) actually sees it; and
- (d) the Stage 0A gate cross-check: `compose_envelope`, run over a real grammar exhibiting the
  configuration, actually returns the new verdict — not a hand-reviewed claim divorced from the gate
  that will enforce it (`capability.rs`'s own `default_registry_discharges_every_fail_closed_or_
  config_predicate_kind`-style integration test is the existing pattern to extend).

Where the oracle itself is unverified for a configuration (never independently pinned against `hc.dll`),
step (a)/(b) cannot even be attempted honestly — that configuration is **unsupported by definition**
(ADR 0001, verbatim, citing exactly `SimultaneousRewrite`'s overlap case) until
`add-reference-hermitcrab-parity` produces independent ground truth. See D6.

### D2. Per-construct table — all 14 non-`Proven` kinds

Every "unsupported split" cell is transcribed from `capability.rs`'s own `default_disposition`/predicate
doc comments and confirmed against each predicate's actual `evaluate()` body (not inferred) — citations
inline. "Fixture(s) needed" names what closes the *open* cell only; already-`Covered` splits are marked
done.

| # | `CharacteristicKind` | Disposition (floor) | Discharging predicate | Split: closed vs. open | What closes the open split | Fixture(s) needed | Verdict |
|---|---|---|---|---|---|---|---|
| 1 | `RealizationalMorphology` | `ConfirmOnly`, no split | — (none registered; none required) | Always `ConfirmOnly` — presence-blocking depends on which allomorphs competitively win, inherently confirm-time. | Nothing open. | Already `Covered` (`RealizationalAffixProcessRule`). | **PERMANENT CARVE-OUT** (already fully closed) |
| 2 | `Compounding` | `ConfigPredicate` | `compounding.non-recursive` (`CompoundingRecursionSafePredicate`) | Closed: non-recursive license-gated head×non-head cross product (`ConfirmOnly`). Open: `detail.recursive == true` (self-feeding/nested `CompoundingRule`) → `Refuse` (design.md D2 item 3, "still unproven," explicitly not "impossible"). | Bound self-feeding depth via a rule-graph reachability pass over the stratum's `CompoundingRule` input/output-PoS graph (max-cycle-length, same shape as the existing non-recursive/recursive classifier, extended to a depth bound) + a depth-budgeted faithful cross-product construction (same ADR 0003 chain-depth-budget shape `unordered`/`peel` already use) + a no-false-negative containment proof against `pg_parse::Morpher`. | New `edge-cases` fixture: a stratum whose `CompoundingRule` output PoS re-enters its own input PoS (self-feeding), asserting bounded-depth recall parity. Author against `pangloss` per the conformance-grammars skill's oracle-discipline note (confirm engine assumed complete, R1) — no new C# ground truth needed. | **PROVABLE** |
| 3 | `UnorderedMorphRuleApplication` | `ConfigPredicate` | `unordered-application.chain-depth-bounded` (`UnorderedOrderingUnionPredicate`) | Closed: `within_bound` (stratum's loose-rule count ≤ `DEFAULT_ORDERING_MULTIPLICITY_BUDGET`) → `ConfirmOnly`, already `Covered`. Open: `!within_bound` → `Refuse`. | Nothing provable here in the correctness sense — `UnorderedOrderingUnionPredicate`'s own doc: this "mirrors `FomaProposer::new_with_budget`'s own, independently-derived refusal," i.e. a genuine combinatorial resource ceiling (any-order proposal literally grows with the bound), not an unproven construction. | None (no fixture closes a resource ceiling). | **PERMANENT CARVE-OUT** — cost axis, owned by `calibrate-fst-resource-envelopes`'s governance (evidence + proposed diff + human-reviewed commit), not a proof this plan can close |
| 4 | `MprGroupAppend` | `ConfirmOnly`, no split | `mpr-group.append-output` (`MprGroupAppendNonNarrowingPredicate`) | Always `ConfirmOnly` when observed — "zero marginal cost... discharges an EXISTING code path verbatim," no `Refuse` branch in `evaluate()`. Already orthogonality-retired against `UnorderedMorphRuleApplication` (`plan_interaction_coverage::retired_interactions()` entry 1, `cover-mpr-groups` design.md D4). | Nothing open. | Already `Covered` (`MPR features/groups`). | **PERMANENT CARVE-OUT** (already fully closed) |
| 5 | `MprGroupOverwrite` | `FailClosed`, unconditional | `mpr-group.overwrite-output` (`MprGroupOverwriteFailClosedPredicate`) | No closed split exists or is claimed to be reachable: `evaluate()` unconditionally `Refuse`s any `Overwrite`-output group. `pg_grammar::model::mpr_add_output`'s own doc + this predicate's own doc: "a monotone-accumulation admission filter is unsound for history-dependent Overwrite replace semantics" — not "unproven," structurally unsound *by construction* (which prior state a later rule overwrites *from* depends on application order; there is no safe finite superset to propose). | None — no construction is claimed to exist even in principle for a bare superset-then-prune shape. | None. | **PERMANENT CARVE-OUT** — the clearest case; ADR 0005 override is the only on-ramp |
| 6 | `SimultaneousRewrite` | `ConfigPredicate` | `simultaneous.subrule-overlap` (`SimultaneousSubruleOverlapPredicate`) | Closed: pairwise-non-overlapping subrules (via `lower.rs`'s real span intersection, `subrules_pairwise_verdict`) → **`Admit`** (already the strongest rung any construct has reached). Open: self-opaquing OR a genuine lowered-span intersection (real overlap) OR an unsupported span → `Refuse`. | ADR 0001's own cited example, verbatim: "the oracle itself is unverified for a configuration (e.g. simultaneous-subrule overlap, never pinned against `hc.dll`)... unsupported by definition." The blocking question is not construction quality — `lower.rs`'s span intersection is already the real evidence, per this task's own framing — it is that HermitCrab's actual behavior for genuinely overlapping subrule environments has never been independently verified. | A C#-oracle-authored fixture with two subrules whose environments provably overlap, once the harness exists. | **NEEDS-ORACLE** (only construct with an ADR-0001-cited, named oracle gap) |
| 7 | `RightToLeftRewrite` | `ConfigPredicate` | `right-to-left-rewrite.faithful-reversal-construction` | Closed: in-scope pattern shapes (no `Quantifier`/`Segments`/`Anchor`/disagree-polarity alpha var, resolvable owning table) → `ConfirmOnly`, `Covered` by 3 named fixtures (`rtl_plain_rule...`, `rtl_feature_environment_swap...`, `rtl_deletion...`). Open: out-of-scope shapes → `Refuse` (the real compiler already honestly skips, `Ok(None)`). | Extend `compile_rtl_branch_net`'s reversal-plus-safety-net-union construction to the excluded pattern shapes one at a time (each is its own sub-task — `Quantifier` support likely composes with `compile-bounded-fst-quantifiers`'s own construction once both land); the general "safety-net union is recall-complete against `pg_rules::rewrite`'s direction-blind pick-order" argument is structural and does not obviously change per shape, but each extension needs its own re-verification. | One new `edge-cases` fixture per newly-supported shape (e.g. an RTL rule with a bounded `Quantifier` in its environment), authored against `pangloss`. | **PROVABLE** (an open-ended, per-shape engineering queue, not a single yes/no proof) |
| 8 | `Metathesis` | `ConfigPredicate` | `metathesis.faithful-swap-construction` | Closed: `Dir::LeftToRight` in-scope shapes → `ConfirmOnly`, `Covered`. Open: `Dir::RightToLeft` metathesis has **zero construction attempted at all** (`compile_metathesis_rule`'s own doc: "out of scope for this change's swap construction") — this is not an unproven existing construction, it is a not-yet-built compiler feature. | Design and build a right-to-left swap-relation construction from scratch (no existing partial attempt to extend, unlike RTL rewrite). | A new RTL-metathesis fixture, once (if) built. | **NEEDS-DECISION** — is a from-scratch RTL-metathesis construction worth building at all (how rare is RTL metathesis in practice), or is this a candidate for a declared, permanent scope boundary the way `MprGroupOverwrite` is? This plan cannot answer that priority question from evidence alone. |
| 9 | `Epenthesis` | `ConfirmOnly`, no split | `epenthesis.structural-composite-route` | Always `ConfirmOnly` when observed (`evaluate()`: `if observed { ConfirmOnly } else { Admit-vacuously }` — no `Refuse` arm exists at all). | Nothing open. | Already `Covered` (`RewriteRule Iterative (epenthesis/...)`). | **PERMANENT CARVE-OUT** (already fully closed — no split to promote) |
| 10 | `CircumfixOutputAction` | `ConfigPredicate` | `circumfix-output-action.faithful-structural-composite` | Closed: `structural_composite_attempted == true` (reaches `build_structural_composites`) → `ConfirmOnly`, `Covered`. Open: `structural_composite_attempted == false` → `Refuse`. | First step is a census of exactly which circumfix-shaped allomorphs fail `crate::emit::is_structural_rule`/`build_structural_composites` today (not enumerated in any doc read for this plan) — then extend the structural-composite builder to the missing shapes, each with its own recall-safety argument. | One new fixture per newly-supported shape, once the census identifies them. | **PROVABLE**, but the first task is the census itself — the specific gap shapes are not yet named anywhere in-repo |
| 11 | `Reduplication` | `ConfigPredicate` | `reduplication.peel-eligible-rule-kind` | Closed: peel-eligible `AffixProcessRule` true-reduplication → `ConfirmOnly`, `Covered`. Open: a `RealizationalRule` allomorph carrying the same true-reduplication RHS shape → `Refuse` — explicitly documented as "a real, faithfully-preserved C# quirk" (`crate::peel::is_reduplication_rule`'s own doc), a **deliberate parity choice**, not an unproven construction. | Nothing to prove — the carve-out exists *because* matching C#'s own quirk faithfully means never peeling a `RealizationalRule`, not because a construction is missing. | None. | **PERMANENT CARVE-OUT** (a faithfully-preserved oracle quirk) |
| 12 | `CoOccurrenceConstraint` | `ConfirmOnly`, no split | — (none registered; none required) | Always `ConfirmOnly` — co-occurrence exclusion depends on which allomorphs actually co-occur in a candidate analysis, inherently confirm-time (ADR 0001's own named example, alongside realizational constraints). | Nothing open. | Already `Covered` (`MorphemeCoOccurrenceRule/AllomorphCoOccurrenceRule`). | **PERMANENT CARVE-OUT** (already fully closed) |
| 13 | `MultiTable` | `ConfigPredicate` | `multi-table.faithful-table-threading` | Closed: `representations_pairwise_disjoint == true` → `ConfirmOnly`, `Covered` (two fixtures, incl. the stronger two-table-disagreement case). Open: tables share a representation → `Refuse` ("the residual case this change's threading fix cannot make faithful"). | A PUA-style disjoint-token-range encoding across tables (assign each `CharacterDefinitionTable` its own reserved token range rather than relying on natural disjointness) would remove the residual collision risk — a describable, buildable fix, not a fundamental impossibility. | A new fixture with two tables that legitimately share a spelling, once the disjoint-range encoding lands. | **PROVABLE**, but larger in scope than the other structural extensions (touches the shared token-space design) — flagging for explicit prioritization alongside item 7/10, not asserting it is small |
| 14 | `QuantifierPattern` | `ConfigPredicate` | `quantifier.bounded-expansion` | Closed: every quantifier bounded and the rule otherwise compiles → `ConfirmOnly`, `Covered`. Open (two distinct sub-splits): (a) a genuinely unbounded quantifier (`max == -1`, the Kleene sentinel) → `Refuse`; (b) all-bounded but blocked by some *other* unsupported construct in the same rule → `Refuse` (inherits that other construct's own row, not a `QuantifierPattern`-specific gap). | Sub-split (a) is genuinely unclear from available evidence: the predicate's own doc frames the refusal as "a finite cutoff must never masquerade as unbounded Kleene semantics," which reads as a *semantic honesty* concern (do not silently truncate something the grammar declared unbounded) rather than a proof that foma's native (regular-language) Kleene star cannot express a truly unbounded repetition faithfully. No document read for this plan says which is actually true. | Unknown pending the decision below. | **NEEDS-DECISION** on sub-split (a) — is true-unbounded quantifier compilation (foma's native Kleene star, no cutoff) structurally infeasible for some reason not yet written down (interaction with `lower.rs`'s span intersection or `MultiTable`'s per-table windowing?), or simply unattempted? Sub-split (b) is not this row's own gap. |

### D3. The `constructs.txt` mismatch — mapping-gap shape

Two independent, non-overlapping gaps exist between the 25-row `constructs.txt` checklist and the
20-`CharacteristicKind` registry — conflating them would misstate the coverage picture:

- **12 of 25 `constructs.txt` rows have no `CharacteristicKind` at all** (this repo's characterizer has
  nothing to say about them): `MorphologicalOutputAction: CopyFromInput/InsertSegments`,
  `MorphologicalOutputAction: ModifyFromInput/InsertSimpleContext`, `Affix template slots
  (obligatory/disjunctive/ordering)`, `Boundary markers (CharacterDefinitionTable)`,
  `Guesser/LexicalGuess`, `Disjunctive allomorphs / free-fluctuation`, `Stem names`, `Syntactic feature
  agreement (...)`, `Alpha-variable phonological environments (...)`, `CompoundingRule constraints
  (MaxApplicationCount/Blockable/...)`, `Ordinary/realizational rule constraints
  (MaxApplicationCount/RequiredStemName/Blockable)`, `Tracing (TraceType)`. These are outside this
  plan's own scope (no capability-envelope work touches them today) but are named here so a future
  reader does not mistake "16/20 kinds Covered" for "the whole `constructs.txt` checklist is exercised."
- **4 of 20 `CharacteristicKind`s have no `constructs.txt` row** (`LeftToRightRewrite`,
  `RightToLeftRewrite`, `SubruleGating`, `MultiTable`) — see D5, the upstream task.
- One judgment call, not a gap: `constructs.txt`'s `"AffixProcessRule: subtraction/truncation"` row is
  folded into `CharacteristicKind::Affixation` (no distinct characteristic for plain truncation exists —
  `CircumfixOutputAction`'s own trigger explicitly excludes single-part-LHS truncation), documented
  already in `conformance_coverage.rs`'s own doc, restated here only because it is easy to mistake for a
  13th unmapped row.

### D4. Fixture enumeration bounded by the plan tree (the actual point of this plan)

Naively, "cover every open (construct, configuration) cell against every plan-tree position it could
occur at" is a cross-product: 8 open cells (D2's PROVABLE + NEEDS-ORACLE + NEEDS-DECISION rows, excluding
the 6 already-`Covered`-and-closed PERMANENT CARVE-OUTs) × however many distinct plan-tree positions a
construct's tag could attach to. `plan_interaction_coverage.rs` is exactly the instrument that collapses
this:

1. **The adjacency-tuple space is closed at 7 shapes** (`legal_adjacency_tuples()`, proven closed because
   `enumerate_default` is this crate's only enumerator strategy and its topology is fixed). Closing a
   `Refuse` gap for any construct in D2 **never adds an 8th tuple shape** — a new recursive-compounding
   fixture, for instance, still only ever realizes `(Gate, Compose[Static])`,
   `(Compose, Leaf/LexiconFragment)[Static]`, etc., the same 7 shapes every existing fixture already
   realizes. New evidence rides on an *existing* tuple; it never grows the tuple set.
2. **Most non-`Proven` characteristics fold onto the single representative `Gate` node**, not their own
   `PlanNodeKind` (`plan_interaction_coverage::representative_kinds`'s own doc lists exactly this set:
   `Compounding`, `UnorderedMorphRuleApplication`, `MprGroupAppend`, `MprGroupOverwrite`,
   `CircumfixOutputAction`, `Reduplication`, plus grammar-wide facts). So closing `Compounding`'s
   recursive split, say, adds evidence to the *same* `(Gate, Compose)` tuple's tag set that
   `Compounding`'s non-recursive fixture already covers — it is one new fixture, not a new tuple × every
   other tag combination.
3. **Orthogonality retirement is the convergence mechanism, and it already demonstrates growth-by-
   subtraction.** `retired_interactions()`'s two entries (`mpr-group.append-output` ×
   `unordered-application` co-occurrence at `Gate`; `Gate`-group sibling reordering) each retire a whole
   class of would-be fuzz cases by citing an existing proof rather than generating cases. As D2's
   PROVABLE items land (e.g. a proof that `RightToLeftRewrite`'s reversal-union construction is safe
   *regardless* of which other `ConfigPredicate` characteristic co-occurs on the same rule), each such
   proof is a candidate **third retirement** — the mechanism is designed to keep shrinking the residual
   fuzz surface, not to enumerate it exhaustively.
4. **The concrete fixture-authoring rule this plan sets**: for each PROVABLE row in D2, author exactly
   one new conformance fixture (staged in `conformance-staging/` per the `conformance-grammars` skill,
   then graduated) exercising the newly-closed configuration — never a matrix against every other
   construct. After each new fixture lands, re-run `tests/plan_interaction_coverage_gate.rs` and confirm
   (a) `unexpected_tuples` stays empty (no 8th shape appeared — would only happen if a second enumerator
   strategy shipped, out of scope here) and (b) both existing retirements still hold (their own citations
   still apply unchanged). This is the whole fixture-enumeration method: linear in open gaps, never
   combinatorial in gaps × tree positions.

### D5. The 4 Unmappable constructs — an explicit upstream task

`LeftToRightRewrite`, `RightToLeftRewrite`, `SubruleGating`, and `MultiTable` cannot be asked for
conformance coverage at all until `constructs.txt` gains rows that tag them as their own phenomena —
`conformance_coverage.rs::construct_ids_for`'s own doc already names the near-miss rows considered and
rejected for each (direction is not what `RewriteRule Iterative`/`Simultaneous` tag; the `MPR
features/groups` row's every actual usage is a morphological `MprGroup`, never a gated phonological
subrule; no row mentions "more than one `CharacterDefinitionTable`" at all). This plan schedules, as an
explicit task (D7/tasks.md), a PR against `sillsdev/machine` (`conformance-framework` branch, per the
`conformance-grammars` skill's own Graduate section) adding four new checklist rows — proposed text,
for review, not final:
- `RewriteRule Direction (LeftToRight/RightToLeft, as its own tagged phenomenon distinct from
  Iterative/Simultaneous application order)`
- `PhonologicalSubrule required/excluded MPR or POS gating (subrule-level, distinct from
  MorphologicalRule-level MPR features/groups)`
- `Multiple CharacterDefinitionTable (per-stratum table assignment, cross-table symbol/representation
  threading)`
Only 3 new rows are listed because `LeftToRightRewrite`/`RightToLeftRewrite` share one direction-tagged
row (mirroring how `OrderedMorphRuleApplication`/`UnorderedMorphRuleApplication` already share
`"Stratum (Linear/Unordered rule order)"`). Until this PR merges and the `machine` submodule pointer is
bumped, these 4 kinds structurally cannot leave `Unmappable` — no amount of in-repo fixture-authoring
substitutes for the upstream checklist row existing.

### D6. What needs the C# oracle harness

Per `IMPLEMENTATION-READINESS.md` R1, "HermitCrab and the Rust model are assumed complete apart from bug
fixes" — this assumption covers every construct in D2 uniformly *unless* a specific configuration is
explicitly named as unverified. Re-reading every row against that bar: **`SimultaneousRewrite`'s
overlapping-subrule configuration is the only one ADR 0001 itself names** ("e.g. simultaneous-subrule
overlap, never pinned against `hc.dll`") — every other PROVABLE row in D2 (`Compounding.recursive`,
`RightToLeftRewrite`'s additional shapes, `CircumfixOutputAction`'s missing shapes, `MultiTable`'s
shared-representation encoding) can be closed against this repo's own confirm engine per the
`conformance-grammars` skill's own "Oracle discipline" note (`pangloss` is the oracle for a fixture until
re-verified; that is the norm this repo already operates under for every staged fixture authored without
`dotnet`/C#).

**Flagged as a judgment call, not asserted as settled**: whether the two NEEDS-DECISION rows
(`Metathesis`'s from-scratch RTL construction, `QuantifierPattern`'s unbounded question), if greenlit,
should *also* get a genuine C#-oracle re-verification pass before being promoted, given how novel and
rare both configurations are relative to the rest of the registry. This plan does not resolve that
question — it is named for the human decision in D7/tasks.md alongside the two NEEDS-DECISION rows
themselves.

### D7. Sequencing and definition of done

**Sequence** (each step's own worktree, per `STAGING.md`'s existing merge-hotspot discipline):
1. File the `constructs.txt` PR (D5) — unblocks `Unmappable` → mappable for all 4 rows; bump the
   `machine` submodule pointer on acceptance.
2. Fix `conformance_coverage.rs`'s own scope gap **before** any build-breaking flip: today
   `supported_kinds()`/`supported_coverage_report()` restrict themselves to the 6 `Proven` kinds, which
   is narrower than the 16/20-row ledger `coverage_ledger.rs::build_ledger` already computes over *all*
   20 kinds. Flipping only the narrow (`Proven`-only) cross-check to build-breaking would under-scope
   "full coverage" the moment a `ConfigPredicate`/`ConfirmOnly` configuration is claimed closed but
   never actually gated. The ledger-wide cross-check (all 20 rows, `FailClosed`/permanently-`Refuse`d
   splits excluded via their own declared-carve-out status, mirroring `plan_interaction_coverage
   ::TupleStatus::ContainsUnsupported`'s own "never a candidate for a covering fixture to begin with"
   framing) is the one that must eventually go build-breaking.
3. Close the PROVABLE rows one construct at a time, Stage-2-style (full kit: construction + containment
   test + conformance fixture + ledger-row update): `Compounding.recursive`, `RightToLeftRewrite`'s
   additional pattern shapes, `CircumfixOutputAction`'s census-then-fix, `MultiTable`'s disjoint-encoding
   fix — in roughly that order (cheapest proof obligation first).
4. Escalate the NEEDS-DECISION rows (`Metathesis` RTL, `QuantifierPattern` unbounded) plus D6's oracle-
   re-verification-policy question to a human/architect decision record (a new short ADR or a dated
   `STAGING.md` note) before any engineering effort is dispatched against them.
5. Resume `add-reference-hermitcrab-parity` far enough to resolve `SimultaneousRewrite`'s overlap
   configuration (D6) — independent of step 3, different crate/toolchain, can run in parallel.
6. After every promotion, re-run `plan_interaction_coverage`'s report (D4's own check) to confirm the
   7-tuple set stays closed and both retirements still hold; add new retirements as new orthogonality
   proofs land.
7. Flip both cross-checks (conformance-coverage, ledger-wide per step 2; plan-interaction-coverage) from
   advisory to build-breaking. This is the finish line, not a follow-on cleanup step.

**Definition of done** — full coverage is reached when, and only when:
- Every one of the 20 `CharacteristicKind` rows in `coverage_ledger.rs` is either `Proven`; a
  `ConfigPredicate`/`ConfirmOnly` kind whose every *reachable* (non-permanently-carved-out) configuration
  has a passing conformance fixture and a curated containment-test citation; or a `FailClosed`/
  permanently-`Refuse`d configuration with a written carve-out reason in its own predicate's doc (already
  true for `MprGroupOverwrite` and `Reduplication`'s `RealizationalRule` case; to be written for whichever
  of D2's rows land there after step 4's human decision).
- Zero `Unmappable` rows (D5's PR has merged).
- Zero unresolved NEEDS-DECISION rows (each has become PROVABLE-and-closed or PERMANENT-CARVE-OUT-and-
  documented by an explicit human decision, never silently defaulted either way).
- The ledger-wide conformance-coverage cross-check (step 2's fix) asserts zero gaps as a hard,
  build-breaking CI gate — `conformance_coverage.rs`'s own deferred "Task 5.1 proper" assertion, finally
  flipped, is the literal textual change that marks this milestone.
- `plan_interaction_coverage`'s report shows zero `Uncovered` required tuples, and its own gate has
  likewise flipped to a hard assertion.
- `ConfirmOnly` → `Admit` promotion remains an explicitly separate, optional, non-blocking track — restated
  here so a future reader does not mistake "full coverage" for "everything becomes `Proven`/`Admit`,"
  which D1 already establishes is not the target shape.

## Dependencies

Requires `add-capability-characteristics-check` (the registry/ledger this plan reads), `add-pairwise-
grammar-interaction-coverage` (the tree instrument D4 generalizes), and the 11 Stage-2 per-construct
changes (the predicates D2's table characterizes). Names, but does not itself implement, work against
`add-reference-hermitcrab-parity` (D6) and a `sillsdev/machine` PR (D5). Sequenced after Stage 3 in
`STAGING.md`'s spine — see that file's own updated section.
