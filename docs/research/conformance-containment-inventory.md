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
