# 056: Provisional definitions apply to every project

Kind: behavioural.
Status: open — implemented in Rust; cross-engine measurements pending.
Evidence: ADR 0008 and code-constructed snapshot regressions.

## C# site

Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`,
`src/SIL.Machine.Morphology.HermitCrab/CharacterDefinitionTable.cs::Segment`:
an undefined character throws `InvalidShapeException`. FieldWorks' HC loader can drop
the affected allomorph. There is no C# equivalent of this unconditional completion policy.

## Rust site

`pg-grammar/src/compile/mod.rs::compile_project_recording` always completes selected
literal usage. `compile/options.rs::CompileOptions` has no Auto/Strict/Complete policy.
ActiveParser remains provenance; it does not control provisional definitions.
Every provisional definition is reported at Info severity. Fully defined means that
grammar health lists no provisional definitions. No strict mode or CLI flag exists.

## Coverage and verification

`pg-grammar/src/compile/tests.rs::provisional_letters_are_enabled_for_every_project_without_parser_policy`
tests HC and XAMPLE with both values of AcceptUnspecifiedGraphemes, retaining the same
missing-letter allomorph and naming the assumption in an Info finding.
The expectations are ADR-derived; managed runs and fix-removed sensitivity are recorded
in the udp-core lane report. The obsolete policy API tests were deleted by owner direction.

Undefined natural-class environments retain current behaviour; ADR completion of that
construct is a separate seam and is not implemented here.
The lead reports that FieldWorks/XAMPLE and C# HC both drop an environment naming a
nonexistent class whole. PanGloss keeps that drop; its Info finding remains later work.

XAMPLE/C# measurements and staged conformance fixtures come from a separate lane.
No upstream issue or fix PR was posted here because network access is closed. Staged
fixtures and measured cross-engine results remain distinct, pending deliverables.
