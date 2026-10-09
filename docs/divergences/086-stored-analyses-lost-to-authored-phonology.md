# 086: Grammar health reports stored analyses lost to authored phonology

Kind: diagnostic (deliberate XAMPLE difference; no new parse divergence).
Status: implements the accepted promise in [ADR 0008](../adr/0008-provisional-definitions.md), requested by the integration lead on 2026-10-09.
Related comparison: [078](078-xample-minimum-authored-phonology.md).

## Contract and owner path

When grammar health receives a FieldWorks project, it compares each imported `WfiAnalysis` key
with PanGloss's confirmed parses for that `WfiWordform`. A missing ordered allomorph/MSA/inflection
type key is synthesized forward through the existing Morpher, and its trace reports each authored
phonological rule whose application changed the synthesized shape. The Info finding identifies
the wordform, stored morph forms and source GUIDs, rule names and GUIDs, and the resulting surface.
Authored rules remain authoritative; the report records their effect without rejecting the
grammar or retrying with rules disabled.

If the stored tuple cannot be resolved or synthesis cannot complete, the finding says
`unattributed` and gives the reason. A reached synthesis step budget is reported exactly as
`synthesis step budget reached`; it cannot be mistaken for a complete synthesis with no rule
effects.

`pg-fwdata` owns extraction of the `WfiWordform`, `WfiAnalysis`, and `WfiMorphBundle` objects and
classifies each as a carrier in the source inventory. `pg-cli` owns the project-level comparison
and finding construction. `pg-parse` owns forward synthesis and trace-based surface rendering.
The finding uses the existing grammar-health metadata and FieldWorks source-subject mechanism.

## Featureless output rendering

In `12-featureless-rule-class`, a feature-only rewrite clears a segment's concrete char-def
identity. The ordinary `pg-parse::surface::to_plain_string` renderer consequently picks `x`, the
first representation compatible with each such underspecified feature shape, producing `xxxx`
or `xxxxx`. The stored-analysis finding uses the applied rule's literal RHS segment IDs from the
actual synthesis trace to recover and show `mupa`, `xupa`, and `xmupa`; it also reports the ordinary
renderer output in a surface-display note. If a trace does not identify an exact spelling, the
finding retains the renderer's output and states the display limit.

## Evidence and coverage

Ledger 078 and the staged underdefined projects provide the independent expected behavior: the
`m->p` rule changes the listed words in `08-rule-context` and `12-featureless-rule-class`. The
native XAMPLE/C# captures were not rerun in this lane. The regression tests import those exact
FieldWorks projects and assert the wordform, stored morph GUIDs, rule name and GUID, and resulting
surface. A separate control retains a parseable `kat` analysis without a finding while reporting
an unrelated missing analysis as unattributed. A synthetic zero-step-budget check requires the
explicit cap reason. Implementation, staged fixture presence, and demonstrated regression
coverage are distinct evidence; fix-removed results and final managed gates are recorded in the
lane report.

No new Machine issue or upstream claim is made for this product reporting behavior.
