# 053: Provisional letters use extended grapheme clusters

Kind: behavioural.
Status: open — implemented in Rust; cross-engine measurements pending.
Evidence: ADR 0008 and code-constructed snapshot regressions.

## C# site

Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`,
`src/SIL.Machine.Morphology.HermitCrab/CharacterDefinitionTable.cs::GetShapeNodes`:
NFD longest matching authored representation; an unknown representation fails segmentation.
C# does not supply this provisional-letter algorithm.

## Rust site

`pg-grammar/src/compile/substrate.rs::failing_letter`, `complete`: an undefined letter
is one extended grapheme cluster in NFD. Composed or decomposed `ã` becomes one NFD letter.
`exemplar_letter` isolates exemplar selection and currently accepts only entries equal to a
whole grapheme cluster. Provisional multigraph handling is deferred to the owner decision.
Authored definitions still take precedence. An unclassifiable control character refuses
the grammar with a named error, also recording the allomorph owner's segmentation decision.

## Coverage and verification

`pg-grammar/src/compile/tests.rs::provisional_multigraph_exemplars_preserve_root_and_prefix_analyses`,
`provisional_letter_units_keep_a_composed_or_decomposed_grapheme_together`, and
`provisional_letter_control_refusal_names_the_owners_allomorph` pin the ADR-derived contract.
`provisional_letter_overlap_with_an_authored_multigraph_refuses_by_name` pins a named
refusal when an authored representation prevents the supplied grapheme from being segmented.
Managed results and fix-removed checks are in the lane report. These tests pin grapheme
fallback and preserve the root/prefix ambiguity while multigraph handling is deferred.
Commands and results are in the udp-core lane report, not a claim of C# parity.

XAMPLE/C# measurements and staged conformance fixtures come from a separate lane.
The lead reports a 12-case XAMPLE measurement: with `{ch}`, both root `chuma` and
prefix `c` + root `huma` are returned. A longest-first provisional `ch` loses that analysis.
The measurement artefacts and staged fixtures remain owned by the separate lane; this lane
has not executed XAMPLE or C# and does not claim independent verification of that report.
No Machine issue or fix PR was posted here: network access is closed. Upstream discussion
and independent cross-engine evidence remain required by ADR 0008.
