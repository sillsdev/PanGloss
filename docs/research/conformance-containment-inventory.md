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

### The zone mismatch: condition understood, implementation deliberately not attempted

The last genuine surface-probe gap is `circumfix-non-first-allomorph-selection`, one fixture. The
emitter's condition is in `emit_rule_allomorphs`: for a rule emitted into zone `Z`, an allomorph
whose `classify_affix` role is neither `Z` nor `Role::None` — and is not the separately handled
`Role::CircumfixPrefix` — is reported uncovered.

Reproducing it needs the zone membership, which is built in the standalone-derivational loop:
`Role::Prefix` goes to the prefix zone, `Role::Suffix` to the suffix zone, `Role::None` to both,
and `any_allomorph_is_circumfix_prefix` then adds a rule to BOTH regardless. That last step is what
creates the mismatch: `rule_role` reads allomorph 0, so allomorph 0 always matches the zone its own
role chose — the conflict only exists for a rule pulled into a second zone by a LATER allomorph,
which is exactly what this fixture's name describes.

Not implemented, as a scope call rather than a blocker. The honest version is an extraction — factor
zone membership out of that loop so the published fact and the emitter share one computation, the way
`unbounded_closure_rule_ordinals` does — because reimplementing an emitter condition rather than
extracting it is what broke three predicate attempts and one emitter suppression in this same work.
That extraction touches a hot loop that also produces `has_compounding_rules`,
`category_changing_out` and the uncovered inventory, for a payoff of one fixture. It wants a fresh
session, not the end of a long one.

The standalone-derivational loop no longer reports a rule the peel claims. The three reduplication
fixtures now compile on the surface route, taking its divergence list from **8 rows to 5**.

The suppression is recall-affecting, so it was verified rather than argued: those fixtures had never
been through containment (the compile refused before), and after the change
`faithfulness_coverage_gate` still reports **19** — the peel genuinely proposes what the loop was
declaring uncovered. The ratchet added earlier is what makes that check binding rather than
decorative.

It is deliberately NOT extended to `is_structural_rule`. Doing so also closed the `process` row, but
broke `phase_c_circumfix::process_role_drop_stays_honestly_unsupported`, an explicit out-of-scope
negative witness that a `Role::Process` drop must stay visible. `is_structural_rule` names a
structural CANDIDATE whose route only runs under a plan topology this loop cannot see — the same
candidate-versus-decision confusion that broke the third predicate attempt. The `process` row
therefore stays, correctly.

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

## A budget the surface probe accepts and ignores

Separate from the containment inventory above, found while type-checking: rustc reports
`unused variable: compose_budget` at `analyzer.rs`'s
`FomaProposer::new_with_budget_and_profile_policy`. That parameter is threaded through four API
layers -- `new_with_budget`, `new_with_budget_and_profile`, `new_with_budget_and_profile_policy` --
and then dropped. The demolition commits `03745a5e` ("remove eager enumeration budget") and
`69efc9dc` ("remove enumeration budget refusals") took out its only consumer and left the signature.

The consequence is not a dead parameter, it is a false claim. A caller who constructs a
`ComposeBudget` in code and passes it to `FomaProposer::new_with_budget` gets no budget at all;
`emit.rs`'s compound loop separately calls `ComposeBudget::from_env().with_chain_depth_cap(...)`, so
the env-derived cap still binds and the *caller-supplied* one silently does not. A test at
`analyzer.rs`'s own `mod tests` does exactly this. `ComposeBudget` is genuinely live elsewhere
(`emit.rs`, `peel.rs` both call `check_chain_depth`), which is what makes this hard to see: the type
is not dead, only this path's use of it.

**The design question is settled by a caller that already exists.** `pg-foma`'s compile worker takes
a `CompileWorkerRequest.chain_depth_cap`, turns it into a `ComposeBudget` in
`CompileWorkerRequest::compose_budget()`, and hands it to `new_with_budget_and_profile` -- its own
doc says the compile runs "under `request`'s own `ComposeBudget`". It does not. That field is a
per-request containment knob with no effect, and no test constructs a request with it set, so
nothing catches it.

Note what still works, because it changes the severity: the ENV-configured cap binds. `emit.rs`'s
compound loop calls `ComposeBudget::from_env()` directly, so `chain_depth_cap_from_env` reaches the
emitter without going through the analyzer at all. It is only the PROGRAMMATIC budget -- the one an
in-process caller constructs and passes as an argument -- that is dropped. That is why the gap
survived: every path anyone measured was env-driven.

So the resolution is to thread it, not delete it:

**CORRECTION -- the paragraph that stood here said "thread it into the emitter", and that was wrong.**
It named `emit_with_budget_profiled` -> `compound_extra_levels_checked_with_cap` as the destination.
That is a DIFFERENT budget dimension, and `emit.rs`'s own doc on
`DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET` says so outright: the compile-time compound-unroll cap is
"kept as its own budget dimension, separate from `ComposeBudget::chain_depth_cap`'s per-word runtime
counter". Threading the caller's budget there would have conflated two deliberately separate
dimensions -- the exact defect class this file exists to record.

Where the runtime budget actually lives: `FomaAnalyzer` (`composite.rs`) owns
`peel_budget: ComposeBudget`, read once from the environment at construction, and threads it into
every `peel_candidates` call. `FomaProposer` -- the struct `new_with_budget*` builds -- has no budget
field at all (`handle`, `report`, `query_encoder`), because the proposer does not peel; the composite
layer does.

So the parameter is not mis-routed, it is **unusable at that layer by construction**, and the right
move is to delete it from `new_with_budget`/`new_with_budget_and_profile`/
`new_with_budget_and_profile_policy`. A caller wanting to bound a COMPILE should use the compound
dimension's own `HC_COMPOUND_CHAIN_DEPTH_BUDGET`; a caller wanting to bound PROPOSE-time peeling
needs `FomaAnalyzer` to accept an explicit budget instead of reading the environment, which it does
not today.

That leaves `CompileWorkerRequest.chain_depth_cap` with no valid destination on this path at all --
it builds a `FomaProposer`, never a `FomaAnalyzer`. Either give `FomaAnalyzer` an
explicit-budget constructor and route the request through it, or drop the field and document the
knob as environment-only. Both are honest; silently accepting it is not.

The falsifying test is a `CompileWorkerRequest` with `chain_depth_cap: Some(n)` small enough to
refuse a grammar that compiles unbounded: it passes today for the wrong reason, because the cap is
ignored and the compile simply succeeds.

Not done in this pass: the change runs through `emit.rs`, which three agents were editing
concurrently. Nothing about the analysis is outstanding -- only the edit.

## Where the divergence count actually stands (measured on the integrated tip)

`envelope_agrees_with_compiler_gate`, all 46 fixtures x 4 strategies:

| | start of track | plain `main` | integrated tip | after the three fixes below |
|---|---|---|---|---|
| agree | 121 | 161 | 166 | **169** |
| too strict (envelope refuses, compile succeeds) | 47 | 11 | 11 | **11** |
| too lax (envelope admits, compile refuses) | 15 | 11 | 6 | **3** |

**Three of the six remaining too-lax rows are one fact already published and deliberately not
consulted.** `polysynthetic-stratal-derivation-chain`, `backend-strata-generic` and
`guesser-pattern-root-fallback` all fail identically: `[rep-variant-overflow] ... root shape
"[Any]*" exceeds 64 representation variants; excess spellings dropped`. That is exactly what
`emit::eager_route_drops_root_spellings` computes. Wiring it into the surface probe's per-strategy
seam closes half the remaining backlog in one line -- and refuses Aweti and Mbugwe, leaving both
with no accepted backend, which is why it has not been wired in. **This is unrelated to the three
fixes below and remains open** (Aweti/Mbugwe question unresolved, own decision, another agent's
scope on `tuned-surface-probed`).

The other three are distinct and unrelated to it: a plan-composed build producing no network
(`loader-pattern-shapes`), an unclaimed standalone Process rule
(`process-morphology-in-place-mutation`), and a Suffix allomorph on a rule referenced in Prefix
position (`circumfix-non-first-allomorph-selection`).

## The eleven too-strict rows, triaged against the oracle rather than against the classifier's own text

`envelope_agrees_with_compiler_gate` names 11 too-strict rows (9 `templated-underlying-tokens`, 2
`plan-composed`). The obvious reading -- "the envelope is too cautious everywhere here, relax it" --
is right for seven of them and wrong for the other four, and the two directions need opposite fixes.
Read out
per-word via `evaluate_plans_observed_with_cache` (bypassing `select_backends`' filter so the refused
backend runs for real against the same oracle `faithfulness_coverage_gate` uses), not inferred from
either side's own diagnostic text:

| row | verdict | witness |
|---|---|---|
| `machine:edge-cases/diacritic-segments` x templated | **(b) envelope wrong** | `FullHcConfirmed`, 13 words, exact match on every comparable word, incl. `gül` correctly SKIPPED |
| `machine:edge-cases/disjunctive-recheck` x templated | **(b) envelope wrong** | `FullHcConfirmed`, 12 words, exact match |
| `machine:edge-cases/loader-isactive-breadth` x templated | **(b) envelope wrong** | `FullHcConfirmed`, 18 words, exact match, incl. `mo+kul`/`zal` |
| `machine:edge-cases/mpr-gated-exception` x templated | **(b) envelope wrong** | `FullHcConfirmed`, 9 words, exact match -- `vokadan` correctly excluded (0 confirmed analyses) |
| `machine:edge-cases/stem-name-restricted-root-allomorph` x templated | **(b) envelope wrong** | `FullHcConfirmed`, 6 words, exact match |
| `machine:edge-cases/strrep-identity` x templated | **(a) compiler wrong** | `IdentityMismatch` on `ndpat`/`imat`: oracle finds 3 identities (`ruleObj` alone, and both linear orders of `rulePfx`+`ruleObj` stacked), templated confirms only 2 -- one stacking order is silently dropped |
| `machine:edge-cases/truncate-morphotactic` x templated | **(a) compiler wrong** | `IdentityMismatch` on `gas`: oracle finds 2 identities (the 1-hop and 2-hop truncation derivations), templated proposes **zero** |
| `machine:languages/suffixing-evidential-adjacency-chain` x templated | **(b) envelope wrong** | `FullHcConfirmed`, 28 words, exact match across every `MorphemeCoOccurrenceRule`/`AllomorphCoOccurrenceRule` adjacency case |
| `staging:edge-cases/backend-gated-generic` x templated | **(b) envelope wrong** | `FullHcConfirmed`, 9 words, exact match (same grammar/construct as `mpr-gated-exception`) |
| `machine:edge-cases/feature-gating-breadth` x plan-composed | **(a) compiler wrong** | `IdentityMismatch` on `kalid`/`kalmuid`: `rrPast` (a `RealizationalRule`) is entirely missing, 0 proposed vs. 1 expected each |
| `machine:edge-cases/morphotactic-attribute-breadth` x plan-composed | **(a) compiler wrong** | `IdentityMismatch` on `kuldede`/`kulru`/`simru`/`kulmoru`: `mrReal` (a `RealizationalRule`) entirely missing, 0 proposed |

### Fixed: the two `plan-composed` rows

Both plan-composed failures are the SAME construct (`CharacteristicKind::RealizationalMorphology`)
and the envelope already carries the exactly right refusal --
`crate::strategy_coverage::plan_composed`'s own table has marked this `CannotRepresent` since before
this session (`coverage_ledger.rs`'s own comment already calls it "the live `PlanComposed` x
`RealizationalMorphology` hole"). What was missing is that `witnessed_coverage::compile_plan_composed`
-- the function `envelope_agrees_with_compiler_gate` actually calls -- never consulted it, so a
network still built while silently dropping every `RealizationalRule`'s material. Fixed by adding the
same check `compile_plan_composed` already runs for marker-bearing plans: read
`GrammarSemantics::characteristics()` for `RealizationalMorphology`, ask
`strategy_coverage::representation_of` (the exact table `capability::strategy_floor` already reads,
not a re-derived condition), and refuse before calling `build_controllable` when it answers
`CannotRepresent`. This moves both rows from too-strict to agree.

### Fixed nothing further: the classifier-gating idea was tried and reverted

The templated route's `strategy-coverage.templated-unsupported-shape` predicate
(`capability::templated_shape_floor`) fires two ways for these fixtures: an "Unordered stratum, N
loose rules" heuristic (fires on 7 of the 9 templated rows) and a `MorphologyRewriteClassifier`
per-allomorph check (fires on `loader-isactive-breadth`, `strrep-identity`, `truncate-morphotactic`).
Both are demonstrably over-broad for at least one fixture apiece -- `loader-isactive-breadth` compiles
and confirms exactly, yet is refused solely by the classifier flagging its boundary-prefix allomorph as
an unsupported "DirectWholeRootWrapper" -- but attempting to narrow either one the obvious way (gating
the classifier behind the same "does a template slot mix prefix and suffix" condition
`emit::atomic_template_carriers` uses before it ever calls the classifier for real) was tried and
**reverted**: `strategy_aware_capability_gate.rs::templated_capability_translates_from_owner_to_final_active_table`
pins a template-FREE synthetic grammar (`CROSS_TABLE_UNTRANSLATABLE_XML`) that must still be refused
by this same classifier for a genuine cross-table translation failure, which proves the classifier is
consulted more broadly than `atomic_template_carriers`' own gate and a grammar-shape-only re-derivation
over-narrows just as reliably as the CLAUDE.md history's three over-refusing attempts. Reverted within
the same build cycle, at zero shipped regressions -- see this file's own rule that a differential
measurement comes before the change, not after.

No narrower, evidence-backed replacement was found in the time available for either heuristic. This
leaves 7 rows in the too-strict count that this session's own oracle comparison shows should read
`Agree`:

- `diacritic-segments`, `disjunctive-recheck`, `mpr-gated-exception`, `stem-name-restricted-root-allomorph`,
  `backend-gated-generic`, `suffixing-evidential-adjacency-chain`: refused solely by "Unordered
  stratum, N loose rules". This is not merely over-cautious, it **duplicates and contradicts an
  already-reviewed verdict**: `CharacteristicKind::UnorderedMorphRuleApplication` has its own
  registered `CapabilityPredicate` (`UnorderedOrderingUnionPredicate`, `capability.rs`), which
  constrains exactly `DERIVATION_LAYER_STRATEGIES` -- `TunedSurfaceProbed` AND
  `TemplatedUnderlyingTokens`, the same two `strategy_coverage::templated_underlying_tokens` says
  share `build_deriv_chain` -- and its verdict for ANY observed `Unordered` stratum, at ANY loose-rule
  count, is `ConfirmOnly`, never `Refuse`. `templated_shape_floor`'s "N loose rules" block is a second,
  ad hoc mechanism for the identical construct, met into the decision OUTSIDE the
  predicate/`Disposition` framework (`capability.rs`'s own `templated_shape_floor` call site, not
  `compose_over_predicates`), and it disagrees with the reviewed predicate for every one of these six
  fixtures. In all 6, the reviewed predicate is the one this session's oracle comparison bears out
  (their two loose rules are either alternatives in one optional slot -- `rPl`/`rDim`,
  `mrSuf`/`mrSufAlt` -- or standalone rules the fixture's own corpus never stacks on one word,
  `ruleT`/`ruleD`). `strrep-identity` (a separate, `(a)`-verdict row above, not one of these six) is
  where the SAME heuristic happens to be right for the wrong reason: its own oracle-cited proof for
  the shared claim, `tests/cover_unordered_morph_rules.rs`, exercises only `FomaAnalyzer`/`Morpher` --
  the mainline/`TunedSurfaceProbed` pipeline -- and names no `TemplatedUnderlyingTokens` case at all,
  so "templated shares the same proven containment" is an UNVERIFIED extension of a mainline-only
  proof, and this session's own measurement is the first evidence it does not hold for the
  two-rule-stacking-order case. Removing `templated_shape_floor`'s duplicate check outright would
  correctly un-refuse these six (bringing the envelope into line with the predicate the crate has
  already reviewed and adopted) but would ALSO un-refuse `strrep-identity`,
  which genuinely needs to stay refused. The predicate framework is exactly where that residual gap
  belongs -- either narrowing `UnorderedOrderingUnionPredicate` itself for the stacking-order case, or
  proving the templated-sharing claim with a templated-specific oracle test the way
  `cover_unordered_morph_rules.rs` does for the mainline -- not left to a second, disagreeing,
  un-reviewed heuristic. Not attempted here: `UnorderedOrderingUnionPredicate` is load-bearing for
  every OTHER `Unordered`-stratum fixture in the suite, and touching it without that templated-specific
  proof in hand is exactly the kind of change this file's own history shows costs a reverted attempt.
- `loader-isactive-breadth`: refused solely by the classifier flagging `mrBoundaryPfx` (the
  boundary-character-inserting prefix behind `mo+kul`) as an unsupported "DirectWholeRootWrapper" --
  the real compile never needs that classifier for a simple one-part prefix allomorph
  (`structural_allomorph::compile_layer`'s own `recipe_for` requires a two-part LHS before it even
  looks at this allomorph), but the classifier still runs and refuses it as if it needed the wrapper
  treatment `atomic_template_carriers` uses for interdigitating templates.

**Recorded as a genuine blocker, not fixed:** a safe narrowing of either check needs either (1) a real
static "can these N loose rules combine on one derivation" analysis, extracted from wherever the
morphotactic-legality machinery (`pg_rules::cascade`, `crate::morphotactics`) already answers an
adjacent question, or (2) splitting the classifier's per-allomorph check so it runs only under the
exact precondition `compile_layer`'s `recipe_for` requires (two-part LHS) in addition to
`atomic_template_carriers`' template-mixing precondition -- neither attempted here because both need
more research than this session's remaining time allowed, and a wrong guess costs a shipped
regression per this file's own three-strikes history.

### Not fixed: the two genuine templated defects (`strrep-identity`, `truncate-morphotactic`)

Both are real: the templated route silently drops an oracle-required analysis while its own
`EmitReport.uncovered` stays empty (`tier: Full`), so `compile_templated_morphotactics` sees nothing
wrong and returns `Ok`. The envelope already refuses both today, via the same "Unordered stratum, loose
rules" heuristic discussed above (imprecise, but happens to be correct here) plus, before this
session's revert, the classifier's `DirectWholeRootWrapper`/`UnlistedTopology` diagnostics. Because
`compile_templated_morphotactics` does not consult the envelope at all (it only consults its own
uncovered-item report, which does not see this class of loss), making the compiler refuse for exactly
these two grammars -- without also refusing the 7 fixtures above where the SAME heuristic is wrong --
needs the same real, targeted fact this section's blocker calls for. Recorded, not implemented.

### Follow-on: the six-fixture blocker above was resolved by deletion, not narrowing

The "genuine blocker" framing above assumed the fix had to be a NARROWER version of the loose-rule
check -- some condition precise enough to keep refusing `strrep-identity` while admitting the other
six. That assumption was wrong, and the actual fix is simpler: `templated_shape_floor`'s "Unordered
stratum, N loose rules" block was never a narrowing problem, it was a **duplicate** of a question
`CharacteristicKind::UnorderedMorphRuleApplication`'s own registered predicate
(`UnorderedOrderingUnionPredicate`) already answers correctly, upstream, for every fixture including
`strrep-identity` -- the predicate's verdict is `ConfirmOnly`, folded into
`compose_envelope_across_strategies`'s decision via `compose_over_predicates` BEFORE
`with_strategy_coverage` ever calls `templated_shape_floor`. Deleting the block (not replacing it with
anything) therefore does exactly the right thing for all seven fixtures at once: the six false
positives lose their only refusal reason and become `Admit`/`ConfirmOnly`, while `strrep-identity`
keeps its OWN, separate, already-present classifier diagnostics (`DirectWholeRootWrapper` on its
`rulePfx`/`ruleObj` allomorphs) and stays exactly as refused as before -- `templated_shape_floor` never
needed to "know" strrep-identity was different, because the check that made it different was never the
one being deleted.

Confirmed via `envelope_agrees_with_compiler_gate`, own runs, before and after this specific deletion:
**168/9/6 -> 174/3/6** (agree/too-strict/too-lax; too-lax unchanged). All six named fixtures
(`diacritic-segments`, `disjunctive-recheck`, `mpr-gated-exception`,
`stem-name-restricted-root-allomorph`, `backend-gated-generic`, `suffixing-evidential-adjacency-chain`)
moved from too-strict to agree; `loader-isactive-breadth`, `strrep-identity` and
`truncate-morphotactic` are unchanged (the first is the still-open classifier false positive above;
the other two are the genuine `(a)` defects, deliberately untouched).

One casualty, fixed in the same commit:
`strategy_aware_capability_gate.rs::templated_selector_refuses_unordered_and_self_opaquing_fixture_shapes`
pinned `strrep-identity`'s refusal WORDING as containing "unordered" -- the exact string the deleted
block produced and nothing else did. `strrep-identity` is still correctly refused (via the classifier
diagnostics named above), so the test was renamed
(`templated_selector_refuses_structural_and_self_opaquing_fixture_shapes`) and re-pinned to
`"morphology relation"`, the phrase every classifier-based refusal witness actually carries -- a
wording correction forced by a deliberate, authorized deletion, not a weakened assertion.

### The `nonregular-process-morphology` shape key hid this from the reference-grammar gate

`backend_selection::capability_shape_key` maps a `CapabilityDiagnostic`'s predicate to a small,
human-facing shape vocabulary for advice/reporting. `strategy-coverage.templated-unsupported-shape` --
the predicate ALL of `templated_shape_floor`'s diagnostics carry, including the now-deleted
loose-rule ones -- is not one of that match's named arms, so every one of its diagnostics falls
through to the `_ =>` catch-all, `"nonregular-process-morphology"`, unless the diagnostic's own
`construct` text happens to contain "truncat"/"delet"/"slot"/"null"/"zero-surface". That means a
refusal produced by the (now-deleted) unordered-stratum heuristic was reported under the exact same
label as a genuine process-morphology refusal -- indistinguishable in `five_language_backend_reports_gate.rs`,
which pins all five private reference grammars (Sena, Indonesian, Amharic, Aweti, Mbugwe) as
templated-`Refused` with precisely that shape key.

Run to check whether any reference grammar's refusal was actually coming from the deleted heuristic
(`PANGLOSS_CORPUS_ROOT=<repo>/samples/data`, `pg.ps1 -Mode corpus-test -Package pg-foma -TestTarget
five_language_backend_reports_gate`, all 6 tests passing both before and after): **none flipped.**
Reading the real diagnostics (`BackendReport::declined_on()`, not the shape key) for every grammar
shows zero mentions of "unordered" anywhere, and every refusal is comprehensive and genuinely
structural:

| grammar | templated verdict | dominant real diagnostic (count, predicate `strategy-coverage.templated-unsupported-shape`) |
|---|---|---|
| Sena | Refused (unchanged) | 234x `DirectWholeRootWrapper morphology relation` |
| Indonesian | Refused (unchanged) | 14x `DirectWholeRootWrapper morphology relation`, 6x `UnlistedTopology morphology relation` |
| Amharic | Refused (unchanged) | 170x `DirectWholeRootWrapper morphology relation`, 8x `AdjacentInitialDrop`, 6x `Infix`, 2x `ModifyFromInput` |
| Aweti | Refused (unchanged) | 388x `DirectWholeRootWrapper morphology relation`, 200x `AmharicInteriorInsertion`, 90x `UnlistedTopology`, 86x `AdjacentTerminalDrop`, 8x `CircumfixPrefix` |
| Mbugwe | Refused (unchanged) | 526x `DirectWholeRootWrapper morphology relation`, 140x `AmharicInteriorInsertion`, 120x `UnlistedTopology`, 24x `Infix`, 12x `AdjacentInitialDrop`, 4x `AdjacentTerminalDrop` |

This is a clean negative result, not an inconclusive one: every reference grammar's templated
refusal rests on dozens to hundreds of independent, per-allomorph structural-classifier findings
(the SAME `structural_allomorph::MorphologyRewriteClassifier` mechanism discussed above, run for real
against real grammars), so the deleted heuristic was never load-bearing for any of the five. The
shape-key collision is still real and still worth fixing on its own (a future refusal from the
now-deleted mechanism, or from a genuinely new cause, would be similarly invisible behind the same
catch-all label), but it is a reporting-fidelity gap, not a capability one -- recorded here rather
than fixed, since the instruction for this session was to check, not to touch `emit.rs` or
`capability_shape_key`.

### Where this leaves the divergence count

Both `plan-composed` rows AND six of the seven `(b)`-verdict templated rows are now closed, moving
from too-strict to agree in this session: **166/11/6 (session start) -> 174/3/6 (current)**. The
remaining three too-strict rows are `loader-isactive-breadth` (the classifier false positive,
narrowing attempted and reverted, blocker recorded above), and `strrep-identity`/
`truncate-morphotactic` (the two genuine `(a)` compiler defects, deliberately out of scope). See the
top-level report for the full measured before/after triple and the test-status ledger.
The other three were distinct and unrelated to the rep-variant-overflow question, and are now
closed:

### `loader-pattern-shapes` x `plan-composed`: closed via `grammar_has_no_tokenizable_root`

The published fact `crate::replace::grammar_has_no_tokenizable_root` (already pinned one way round
by `the_published_no_tokenizable_root_fact_never_over_claims_a_refusal`, and already naming this
exact fixture in its own doc) was computed but never met into `backend_selection`'s per-strategy
seam. `loader-pattern-shapes` has two root allomorphs — one an optional-group pattern (`is_pattern`
true) and one a mandatory natural-class reference (untokenizable, `is_pattern` false) — so every
root fails `SegAlphabet::shape_is_tokenizable` or is pattern-only, `emit_underlying_filtered` skips
every root line, every gated group's `root_entries` stays zero, and
`witnessed_coverage::compile_plan_composed` returns "plan-composed build produced no network" with
no typed error. `select_backends` now meets `plan_composed_no_tokenizable_root_refusal()` into the
`PlanComposed` decision whenever the fact holds, beside the existing marker-leaf refusal.

### `process-morphology-in-place-mutation` x `tuned-surface-probed`: closed via `eager_route_refuses_unclaimed_standalone_rule`

The standalone-derivational loop's own catch-all branch (`other => uncovered.push(...)`, for any
`rule_role` that is not Prefix/Suffix/None/CircumfixPrefix and not a peelable reduplication) was
factored into `standalone_rule_unclaimed_role(g, mid)`, called both by the loop itself (so it
cannot drift from the published fact) and by the new
`crate::emit::eager_route_refuses_unclaimed_standalone_rule(g)`. Met into
`tuned_surface_structural_refusal`, this closes the `Role::Process` row. This does **not** attempt
the "Process is an emitter over-report" question this document raised earlier
(is_structural_rule admits it unconditionally, per `phase_c_circumfix::
process_role_drop_stays_honestly_unsupported`) — it only makes the envelope refuse when the real
compiler already does, which is this document's actual subject.

### `circumfix-non-first-allomorph-selection` x `tuned-surface-probed`: closed via `eager_route_refuses_mixed_circumfix_zone`

This document's own "zone mismatch" section named the exact condition and deliberately stopped
short of extracting it. It is now published as `crate::emit::eager_route_refuses_mixed_circumfix_zone`:
true when `any_allomorph_is_circumfix_prefix` holds for a rule (forcing it into both derivational
zones) and that same rule owns another allomorph outside `Role::None`/`Role::CircumfixPrefix` — the
exact and sufficient condition for `emit_rule_allomorphs`'s zone-mismatch branch to fire in
whichever zone that allomorph does not own. It reuses `any_allomorph_is_circumfix_prefix`,
`allomorphs_of` and `classify_affix` verbatim rather than re-deriving them, and the loop itself was
left untouched — only a fact was published, met into `tuned_surface_structural_refusal`. Measured
with the divergence gate before and after: agreement 166 → 169, too-strict unchanged at 11, so
neither this fact nor the Process one above cost a working capability anywhere in the fixture set.
