---
name: module-seams
description: >-
  Use before adding a check, predicate, or condition that another module already decides, and
  before changing a refusal or capability verdict anywhere in the workspace. Not FST-specific:
  this bites wherever two places can drift. Trigger on "add a check for X", "reproduce this
  condition", "where does this condition belong", "extract or duplicate", "this predicate never
  fires", "should this be a CapabilityPredicate", or "how do I know my change did not break
  something else".
---

# Call the owning module, or extract the decision — never re-derive it

Measured over one session: **four of seven code attempts were reverted, and three failed the same
way.** Each tried to reproduce a condition the emitter already decides, by reading its uncovered-item
text and writing an equivalent-looking check. Every one was a strict superset of the real condition
and refused grammars that compile.

- `rule_role`-based check on `Affixation` — changed nothing, because that kind is
  `Disposition::Proven` and predicates there are never consulted. No test failed. No behaviour
  differed. Only a before/after divergence count showed it was inert.
- `rule_role == Reduplication` — closed 4 divergences, broke 5 working ones.
- `preexpand::unbounded_candidate_rules` — the emitter's **own** helper, and still broke 3 tests
  asserting a concatenative realizational rule compiles.

The third is the lesson in its sharpest form. **A helper named `*_candidates` / `*_candidate_rules`
names a candidate set, not a decision.** The emitter turned that set into a refusal only under a
plan-topology flag the caller had skipped. So:

1. **Call the owning module's function.** If the fact is not exposed, **extract** it — factor the
   condition out of the owner so the owner and the new caller share one computation and cannot
   drift. `crate::emit::eager_route_refuses_mixed_circumfix_zone` is the worked example: it and
   the emitter's own `emit_rule_allomorphs` both call the same `standalone_rule_zones`/
   `allomorph_zone_outcome` pair, so the published fact and the real refusal cannot drift apart.
2. **A published fact gets a one-way gate.**
   `the_published_mixed_circumfix_zone_fact_never_over_claims_a_refusal` asserts only that a
   claimed refusal really refuses — the direction a caller gates on, where a false positive costs
   a working capability.
3. **Check the seam before the condition.** The same condition that failed as a `CapabilityPredicate`
   (7 failures against ratified `ConfirmOnly` contracts) passed with 0 at
   `backend_selection`'s existing per-strategy refusal seam, beside `plan_composed_marker_refusal`.
   A backend-specific structural fact belongs there, not in a predicate, and needs no
   `CharacteristicKind`.

## Build the differential measurement before the change, not after

The only reason those four reverts were cheap is that the measurement existed first.
`envelope_agrees_with_compiler_gate` reports agreement and BOTH divergence directions over every
fixture x every backend, so each wrong turn was refuted inside one build cycle at zero shipped
regressions — including the one that produced no failure and no behaviour change at all.

Before changing a refusal, a threshold, or a capability verdict, add the gate that would show the
change was wrong. Prefer one that reports both directions: a gate that only counts what you closed
cannot see what you broke. Existing examples to copy rather than reinvent:
`envelope_agrees_with_compiler_gate`, `faithfulness_coverage_gate`, `witnessed_strategy_coverage_gate`.

Stage the assertion. `FaithfulnessRequirement::NoMoreThan { failures }` is a ratchet, not a target —
it holds today's count so a new regression fails while a known backlog stays legible. An
all-or-nothing gate on a non-empty inventory asserts nothing and gets left that way.

