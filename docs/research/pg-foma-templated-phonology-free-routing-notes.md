# Templated phonology-free routing notes (`pg-foma/tests/templated_token_cascade_phonology_free_routing_gate.rs`)

Closes a routing gap: a template-bearing grammar with no phonological rules (the measured Sena
shape) must still be offered `EmissionStrategy::TemplatedUnderlyingTokens`, the only candidate
whose lexicon carries template-aware morphotactic structure, rather than only the plan-composed
baseline's deliberately minimal self-looping `uflexc` emitter (which does not generalize to
templated grammars).

`token-cascade-morphology` (the family that requests this strategy) was gated on
`Applicability::HasPhonology` (`!grammar.prules.is_empty()`), a structural fact a
templates-but-no-phonology grammar does not have. So the only underlying model ever offered for a
grammar shaped this way was `uflexc`, and the template-aware candidate was never even materialized
to compare against it. `Applicability::HasPhonologyOrTemplates` widens the gate to `HasPhonology OR
HasTemplates`, evaluated structurally over the same two `Grammar` fields the two narrower variants
already read.

## Correctness beyond reachability

The widened routing is only useful if the compiler it points at actually builds for a
phonology-free grammar. `compile_templated_morphotactics` used to unconditionally turn "zero
declared phonological rules" into `TemplatedCompileError::NoCompiledRules` (an empty
`prules_in_order` composes to `Ok(None)`, then `.ok_or(NoCompiledRules)`-ed into an error) — a
guaranteed build failure that the old `HasPhonology` gate happened to mask by never offering the
family in the first place.
`templated_candidate_builds_and_proposes_on_the_phonology_free_fixture` asserts the candidate now
reaches a real, non-`BuildFailed` verdict with non-zero proposals on `recipe-template-generic`: an
honest confirm/mismatch is a real result, but a build failure would mean the routing fix handed the
optimizer a candidate that can never do anything.
