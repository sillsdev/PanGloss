# STAGING: mpr-group-overwrite-without-realizational

## Why this fixture exists

Narrow, single-purpose probe for `pg_foma::capability::CharacteristicKind::MprGroupOverwrite`.
Closes the `MprGroupOverwrite x plan-composed` gap in `witnessed_strategy_coverage_gate`.

The only two grammars in the whole conformance corpus that carry an `Overwrite`
`MorphologicalPhonologicalRuleFeatureGroup` —
`machine/conformance/languages/fusional-realizational-morphology` and
`machine/conformance/languages/suffixing-extension-slot-ordering` — BOTH also carry a
`RealizationalRule` elsewhere in the same grammar. `CharacteristicKind::RealizationalMorphology`
is `StrategyRepresentation::CannotRepresent` for `EmissionStrategy::PlanComposed`
(`strategy_coverage.rs`: `uflexc::emit_underlying_filtered` skips every `RealizationalRule`
wholesale), so `crate::backend_selection` refuses `PlanComposed` on either grammar before it ever
reaches the MPR-group material — confirmed by the baseline coverage report's own gap attribution:
"2 grammar(s) exhibit it; on this backend 2 were refused by the selector". Neither existing
grammar could ever witness this pair, however long the corpus grows, as long as `RealizationalRule`
and the MPR-group material stay co-located.

This fixture isolates the `ThemeGroup` mechanism from `fusional-realizational-morphology`'s own
`MprGroups` stratum (`mrThemeX`/`mrThemeY`/`mrEndZ`) into a grammar with **zero**
`RealizationalRule` anywhere, so `PlanComposed` is never refused and can actually attempt (and
succeed at) the compile.

## What it pins

- `udof` (root + `mrThemeA`, sets MPR feature `mprA`) parses; `udofq` (+ `mrEndC`, which requires
  `mprA`) also parses — `mprA` is still present.
- `wudof` (root + `mrThemeA` + `mrThemeB`, sets `mprB`) parses, with BOTH rules in the signature —
  the Overwrite group did not block the second rule from applying, only from ACCUMULATING its
  predecessor's feature.
- `wudofq` — THE distinguishing row — must **fail**: after `mrThemeB`, `ThemeGroup`'s
  `outputType="overwrite"` semantics have already dropped `mprA` (not merely accumulated `mprB`
  alongside it), so `mrEndC`'s `requiredMPRFeatures="mprA"` fails. Zero parses. An engine that
  treats `MprGroupOutput::Overwrite` as a flat union (the pre-fix "Append" mis-implementation this
  exact mechanism, mirrored from `fusional-realizational-morphology`'s own `yxpedz` row, was built
  to catch) would wrongly accept this.

Empirically confirmed (see "Verification"): all five words came back exactly as predicted from a
real `pg_parse::Morpher` run, and `characterize` reports `MprGroupOverwrite` (and no
`RealizationalMorphology`) for this grammar.

## Verification

Signatures and the `characterize`/`select_backends_for_grammar` findings were captured by a
throwaway test (`rust/crates/pg-foma/tests/temp_probe_new_fixtures.rs`, deleted after
transcription) that loaded this fixture's `grammar.xml` from disk, printed
`characterize(&g).observations()` and `select_backends_for_grammar(&g)`'s per-backend reports, and
ran `pg_parse::Morpher::parse_word` over every word above.

`characterize` reports `{Affixation, OrderedMorphRuleApplication, MprGroupOverwrite,
NaturalClassDefinition}` — `MprGroupOverwrite` present, `RealizationalMorphology`/
`ProcessMorphology` absent, exactly as designed. Backend selection: all three backends
(`TunedSurfaceProbed`/`TemplatedUnderlyingTokens`/`PlanComposed`) are selected with decision
`ConfirmOnly` — in particular `PlanComposed` is NOT refused here (unlike on either of the two
existing `RealizationalMorphology`-carrying grammars), which is the entire point of this fixture.
The gate's own compile step (not this probe) is what turns that selection into a real witness;
`witnessed_strategy_coverage_gate`'s own run (see the commit this fixture lands in) confirms the
compile actually succeeds and the gap closes.

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Authored fresh
for this task; `words.yaml` signatures captured by driving `pg_parse::Morpher::parse_word` directly
(a throwaway in-repo test, described above).

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/mpr-group-overwrite-without-realizational/`. On acceptance, delete
this staged copy in the same change (graduation guard enforces this mechanically).
