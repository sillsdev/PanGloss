# What stands between the faithfulness gate and `NoFailures`

`faithfulness_coverage_gate` runs propose+confirm over every discovered fixture against the full
Rust HermitCrab oracle and prints every containment failure, but asserts non-vacuity only. Its own
doc names the condition for tightening: *"swap to `FaithfulnessRequirement::NoFailures` once the
printed failure inventory reaches zero."* Nothing had reduced that inventory to root causes, so this
records what is actually in it.

Measured with `-Scope all`: **61 fixtures discovered, 61 observed, 19 (construct, backend) pairs
failed containment.**

## 19 pairs, 4 causes

Every one of the 19 is the same shape — `proposal set offered 0` against an oracle identity of
multiplicity 1, i.e. the backend **missed an analysis the oracle found**. They collapse onto four
(fixture, word) pairs, because one missing word is attributed once per construct the fixture
exercises:

| fixture | word | backends that miss it | pairs |
|---|---|---|---|
| `machine:edge-cases/morphotactic-attribute-breadth` | `kuldede` | tuned-surface-probed, templated-underlying-tokens | 8 |
| `machine:edge-cases/feature-system-breadth` | `isk` | templated-underlying-tokens | 6 |
| `machine:edge-cases/loader-isactive-breadth` | `mo+kul` | tuned-surface-probed | 4 |
| `machine:edge-cases/mpr-overwrite-order-dependence` | `daboyuxa` | templated-underlying-tokens | 1 |

So the gate is **four words away** from being able to assert `NoFailures`, not nineteen defects away.
The 19 figure is a fan-out of construct attribution, and reading it as an error count overstates the
work by roughly five times.

**`plan-composed` appears in the attempted-backends line and in none of the failures.** Whatever the
other two do differently on these four words, the plan-composed compiler does not do it.

## Why this matters more than a backlog count

Under ADR-0001 an FST may overgenerate and may never miss. A backend that can fail to generate an
analysis the grammar licenses must fail hard at the **capability-envelope** step, naming the
construct. These four do neither: the envelope admits them, a network is built, and the miss is
caught only because full-HC confirmation runs afterwards and disagrees.

That the miss is caught is the system working. That it is caught at *confirmation* rather than at
*characterization* is the gap — confirmation is the expensive backstop, not the contract.

`machine:edge-cases/morphotactic-attribute-breadth` is the clearest case to look at first: under
`recipe-optimize` all five of its candidates come back `identity-mismatch` on `kuldede`
(`oracle has 1 distinct identities, candidate has 0`), so no candidate can ever be selected for that
grammar, and nothing before confirmation says why.

## Two neighbouring findings from the same measurement

**A grammar no backend can compile.** For the staged fixture `backend-strata-generic`,
`pangloss fst-health` reports `representability=WithinLimits` while both whole-grammar emitters
refuse at build time with `Partial { uncovered: 1 }`, and all four of its plan-composed candidates
are refused as marker-bearing. A `recipe-optimize` run over it confirms zero candidates. Same shape
as above — the envelope says yes, something later says no.

**The refusal does not name what it cannot do.** `EmitReport.uncovered` is a `Vec<UncoveredItem>`
carrying `kind`, `id` and `reason` — exactly what ADR-0001 asks a refusal to state — but
`Certification::BuildFailed { reason }` stringifies only the tier, so a reader gets
`Partial { uncovered: 1 }`. Several test binaries already print `[{kind}] {id} — {reason}`; the
production path drops it.

## Where the gate now sits, and what is left

`FomaProposer::new` characterizes once and asks the envelope about
`TunedSurfaceProbed` before emitting; a refusal is `FomaError::CapabilityRefused`, naming
predicate/construct/witness rather than a tier, and costs no emission.
`tests/envelope_agrees_with_compiler_gate.rs` is the standing measurement — 183 observations across
every fixture x every backend:

| | count | meaning |
|---|---|---|
| agree | 121 | envelope and compiler reach the same answer |
| envelope admitted, compiler refused | 15 | envelope too lax; safe, but decided in the wrong place |
| envelope refused, build succeeded | 47 | **not** all capability losses — see below |

The gate asserts the 47 contains no `TunedSurfaceProbed` row, which is what makes the surface-probe
gate safe; it is 38 `PlanComposed` and 9 `TemplatedUnderlyingTokens`.

### Step 1 landed; step 2 has a measured false start worth not repeating

**Step 1 (done).** The plan-composed witness path applied no marker check, so a plan whose marker
subtrees `build_controllable` does not build counted as a compiled backend. The check does **not**
belong in `build_controllable`: that module documents those leaves as out of scope, and the
differential oracle, the plan-walk equivalence tests and selection's size measurement all pass it
marker-bearing plans deliberately — comparisons where both sides omit the same subtrees, or where
only state/arc counts are read. Refusing there failed 17 tests, every one a legitimate use. Moving
it to `witnessed_coverage::compile_plan_composed`, where an `Ok` claims a usable backend, closed
36 divergences with none of that fallout: agreement 121 → 157, too-strict 47 → 11.

**Step 2 (measured, reverted).** The eight surface-probe too-lax rows have exactly five causes,
read out of `EmitReport::uncovered` by
`envelope_agrees_with_compiler_gate::report_uncovered_constructs_behind_surface_probe_divergence`:

| cause | fixtures |
|---|---|
| standalone rule's primary allomorph is Reduplication | 3 |
| root shape `[Any]*` exceeds 64 representation variants | 3 |
| standalone rule's primary allomorph is Process | 1 |
| allomorph is a Suffix referenced in a Prefix position | 1 |
| unbounded RealizationalRule, no provable finite closure | 1 |

All five are grammar-structural, so all are reachable without emitting. Two obstacles stand in the
way, both found by trying:

1. **`Affixation` is `Disposition::Proven`, so a predicate hung there is never consulted.** The
   Process and Infix roles have no `CharacteristicKind` of their own and would fall under
   Affixation. A predicate discharging it changed nothing at all — measured, not assumed. Widening
   this needs a kind, or a disposition change whose blast radius covers every grammar.
2. **`rule_role == Reduplication` is too broad a condition.** Hung on `Reduplication` (which *is*
   `ConfigPredicate`), it closed 4 divergences and broke 5 working ones —
   `prefix_reduplication_confirms`, `metathesis_boundary_in_surface_confirms`,
   `bare_root_phonology_makes_post_nasal_voicing_proposable`, plus the plan-diagram and
   templated-selector gates. The emitter reaches its standalone-rule refusal only for rules no
   other route claims, and that fact is **not** recoverable from
   `CharacteristicsProfile::reduplication_details` — `peel_attempted` /
   `structural_composite_attempted` do not separate the refused three from the working three, since
   `ReduplicationPeelSupportedPredicate` already tests exactly that pair and admits all six.

So the next attempt needs the emitter's own routing decision surfaced as a grammar-derived fact,
rather than a condition inferred from the uncovered item's text. Reusing `crate::emit::rule_role`
was right — it is the same classifier — but the role alone is not the emitter's guard.

### Why all three predicate attempts failed: the emitter's refusals are plan-conditional

Three attempts, three different kinds, one root cause. A `CapabilityPredicate` receives the grammar,
the profile, and **one `PlanNodeKind`** — never the plan. Every refusal in the too-lax inventory is
decided by the emitter *after* it knows which route the plan asked for, so a grammar-only condition
is necessarily a superset of the emitter's own and over-refuses:

| attempt | kind | outcome |
|---|---|---|
| `rule_role`-based | `Affixation` | never consulted — that kind is `Disposition::Proven` |
| `rule_role == Reduplication` | `Reduplication` | closed 4, broke 5: the peel covers those rules on another route |
| `unbounded_candidate_rules` | `RealizationalMorphology` | closed 1, broke 9: the emitter refuses only when `plan_wants_composite_emission`, and this helper is the *candidate* superset |

The third is the clearest statement of the pattern. `crate::preexpand::unbounded_candidate_rules` is
exactly the set the emitter starts from, and using it still broke three tests asserting that a
*concatenative* realizational rule compiles through a regular loop — because the emitter only turns
that candidate set into a refusal under a plan flag the predicate cannot see.

Only predicates are gated by disposition, and only `Proven` is skipped (`capability.rs`'s
`.filter(|o| o.disposition != Disposition::Proven)`); `ConfirmOnly` kinds like
`RealizationalMorphology` **are** consulted, which is why attempt three ran at all where attempt one
silently did nothing.

### Aweti and Mbugwe drop root spellings — the sharpest open question here

`crate::emit::eager_route_drops_root_spellings` publishes whether the surface route enumerates a
lexical root's spellings past `REP_VARIANT_CAP` and discards the remainder. Met into selection it
closes three fixture divergences with **no** growth in the too-strict direction — and it refuses
**Aweti and Mbugwe**, leaving those two reference grammars with no accepted backend at all.

So it is not met in. The two readings are:

- The refusal is right, and "all five grammars work" has been resting on a network that silently
  loses spellings for two of them.
- The dropped spellings are immaterial for those grammars, and refusing costs real capability.

The fixture set cannot decide it: `faithfulness_coverage_gate` covers conformance fixtures, not
corpus grammars, and **nothing has ever compiled Aweti or Mbugwe on this route** — the five-language
gate characterizes and stops. That gap is the point. Their `TunedSurfaceProbed Accepted` verdict has
never been checked against a compile, which is exactly the envelope-versus-compiler divergence this
whole document is about, now reaching the reference corpus rather than the fixtures.

Settling it needs one measured run: compile Aweti and Mbugwe on the surface route and compare
proposals against the full-HC oracle for their corpora. Until then the fact is published and pinned
one way round (`the_published_root_spelling_fact_never_over_claims_a_drop`) but not consulted.

### The structural change landed for one cause, and it turns the blocker into a decision

`crate::emit::eager_route_refuses_unbounded_closure` now publishes that route's own refusal as a
grammar-level fact. The topology it depends on is itself grammar-derived
(`plan_topology_decisions`), so the decision is reachable without emitting; the emitter and the fact
share one computation (`unbounded_closure_rule_ordinals`) so they cannot drift.
`the_published_closure_fact_never_over_claims_a_refusal` pins it one way round — the direction a
caller would gate on.

With the fact in hand, a predicate on `RealizationalMorphology` closes the divergence and the three
concatenative-realizational tests that the ungated superset broke now pass, confirming the condition
is right. Seven tests still fail, and they are a different kind of failure — they encode envelope
contracts rather than compiler behaviour:

| test | what it pins |
|---|---|
| `compose_envelope_confirm_only_for_realizational_rule_alone` | a realizational rule alone is `ConfirmOnly`, not `Refuse` |
| `every_narrowing_excuses_only_a_compiler_that_can_represent_the_construct` | narrowing to one strategy requires the others to represent the construct |
| `two_strategies_get_their_own_answers_from_one_shared_semantics`, `the_strategy_blind_envelope_cannot_see_the_hole` | strategy-aware verdict shape |
| `unbounded_realizational_composite_route_returns_no_artifact` | the route returns no artifact — reached differently once the envelope refuses first |
| `coverage_ledger_golden_json`, `plan_diagram_root_verdict_...` | recorded verdicts |

**So this is a decision, not an implementation gap.** Does an unbounded realizational rule make
`TunedSurfaceProbed` `CannotRepresent` — the eager route generates no FST at all, so there is
nothing to confirm against — or does `RealizationalMorphology` stay `ConfirmOnly` as those tests
assert? The predicate is written and measured either way; what it costs is rewriting the contracts
above, and that is a call about what the envelope means, not about what the compiler does.

The remaining causes still need one of two structural changes:

- **give the predicate the plan**, so a grammar-and-plan condition can match the emitter's; or
- **have the emitter publish its route decision as a grammar-level fact** the envelope reads,
  the way `structural_composite_attempted` already publishes `is_structural_rule`.

The second is the smaller change and matches how this envelope already works elsewhere. Either way,
a predicate written against a grammar-only condition will keep over-refusing, and the divergence
gate will keep catching it — all three attempts were measured and reverted within one build cycle
each, at a cost of 0 shipped regressions.

### The whole surface-probe inventory, triaged

`crate::emit::structurally_routed_rule_ordinals` publishes which rules `build_structural_composites`
claims. An uncovered item naming one of those is the emitter reporting a role its
standalone-derivational loop cannot route while another mechanism covers the same rule — an
over-report, fixed in the emitter — as against a genuine gap, refused at the envelope. Measured:

| uncovered item | another route claims it | verdict |
|---|---|---|
| `process` / `process-morphology-in-place-mutation` mrule 0 | **structurally routed** | over-report |
| `reduplication` x5, three fixtures | not structural, but `peel_attempted = true` | over-report |
| `rep-variant-overflow` x3 | none — a dropped spelling has no second home | **gap** |
| suffix allomorph in a prefix zone | none; `capability.rs` records it "never reaches `build_structural_composites` either" | **gap** |
| unbounded realizational closure | none — no artifact is produced at all | **gap, closed** |

So **four of the eight** surface-probe divergences are emitter over-reports, not capability gaps, and
no predicate or selection refusal should be written for them. The `process` result also confirms the
documented behaviour that `is_structural_rule` admits `Role::Process` unconditionally.

That leaves exactly two open gaps: the representation-variant drop (implemented and published, held
back only by the Aweti/Mbugwe question above) and the zone mismatch (one fixture, not implemented —
reproducing it needs the prefix/suffix zone assignment, which is derived from the same `rule_role`
pass rather than from a single helper).

### The reduplication third is an emitter over-report, not a capability gap

Measured, for exactly the mrules each refusal names:

| fixture | rules | `peel_attempted` | `structural_composite_attempted` |
|---|---|---|---|
| `metathesis-phase-isolation` | mrule4, mrule5 | **true** | false |
| `backend-ordered-generic` | mrule4, mrule5 | **true** | false |
| `deletion-reduplication-exception-composite` | mrule3 | **true** | false |

The peel claims every one of them, which is why `ReduplicationPeelSupportedPredicate` admits them
and is right to: `ReduplicationPeeler` has a proposal route for these analyses. What refuses is the
eager composite route's standalone-derivational-rule loop, which has no arm for
`Role::Reduplication` and reports the rule `uncovered`; `FomaProposer::new` then refuses the whole
emit as `Partial`.

So for this third of the inventory the envelope is **correct to admit** and a capability predicate
is the wrong instrument — which is exactly why hanging one on `Reduplication` broke
`prefix_reduplication_confirms` and two of its neighbours. Those fixtures have peel-covered
reduplication too, and refusing them at the envelope loses working capability.

The fix belongs in the emitter: the standalone loop should not report a rule uncovered when the
peel covers it. That is a recall-affecting change and must not be made on this reasoning alone — if
the peel does not in fact propose those analyses at runtime, suppressing the uncovered item creates
exactly the silent under-generation ADR-0001 forbids. The order is: prove peel recall on these three
fixtures first, then suppress. Note the instrument for that proof is currently blocked by the
refusal itself, since the tuned-surface compile never completes for them.

### Two things landed that need no predicate

**A refusal now names its construct.** `FomaError`'s `Display` stringified only the tier, so a
reader got `Partial { uncovered: 1 }` while `EmitReport::uncovered` held kind, id and reason. It now
renders them, which is what ADR-0001 asks a refusal to carry and what makes the five causes above
readable from a production message rather than only from a test.

**The containment inventory is ratcheted.** `FaithfulnessRequirement` offered `NonVacuity` or
`NoFailures` and nothing between, and the inventory has never been empty, so a backend that newly
started under-generating would have joined the list silently. `NoMoreThan { failures: 19 }` holds
today's line without demanding the backlog be cleared first. It is guarded by a test that the
ceiling detects its own target — a ratchet that fires for no count gates nothing.

Three things must land before the other two backends can gate the same way:

1. **`build_controllable` must refuse a marker-bearing plan itself.** It currently succeeds and
   under-generates — `crate::build::unbuildable_markers`' own doc records such a network proposing
   nothing for 19 of 20 corpus words. The check exists but is applied by callers, so anything
   reaching the builder directly steps around it, exactly the bypass shape ADR-0001 forbids. Most of
   the 38 should resolve to `agree` once it does.
2. **The 15 too-lax pairs need capability predicates.** Each is a construct the emitter discovers it
   cannot cover; deciding it from the characterization is what moves the refusal to the envelope.
3. **The four containment causes above.** These are the dangerous direction: envelope admits,
   compiler succeeds, network still misses an oracle analysis.

Only then does "quit only on 1 GiB / 10 GiB / 10 minutes" become literally true, since those are the
sole remaining non-capability reasons a compile stops (`DEFAULT_EXECUTION_LIMITS`).

## What this blocks

`recipe_optimize_continuation`'s three tests derive their bounds from a baseline run's per-candidate
confirmation cost, and need a fixture whose profile has a non-confirmed candidate mid-sequence AND a
final candidate carrying cost. Measured profiles:

| fixture | statuses | confirmation cost |
|---|---|---|
| `backend-strata-generic` (theirs) | all refused | `[0,0,0,0,0,0]` |
| `mpr-gated-exception` | `x C C x x` | `[0,9,9,0,0]` |
| `compounding-breadth` | all confirmed | `[7,7,7,7]` |
| `diacritic-segments` | all confirmed | `[12,12,12,12,12]` |
| `suffixing-evidential-adjacency-chain` | all confirmed | `[28,28,28,28,28]` |
| `template-category-sharing` | all confirmed | `[6,6,6,6,6]` |
| `morphotactic-attribute-breadth` | all mismatched | `[26,26,29,29,26]` |

No fixture has the mix. Marker-free grammars confirm everything; marker-bearing ones refuse their
plan-composed candidates, which are exactly the ones that sort last. The property those tests pin is
currently unexercisable, and repointing them at whichever fixture happens to pass would be fitting
the test to the tree rather than to the property.
