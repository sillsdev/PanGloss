# 054: Provisional letters do not match classes with letter or feature conditions

Kind: behavioural.
Status: open — implemented in Rust; cross-engine measurements pending.
Evidence: ADR 0008 and code-constructed snapshot regressions.

## C# site

Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`,
`src/SIL.Machine.Morphology.HermitCrab/CharacterDefinitionTable.cs::Add`, `GetShapeNodes`:
C# does not create provisional letters. Feature structures without authored values have
unspecified phonological lanes, which can unify with feature-class constraints.

## Rust site

`pg-grammar-model/src/membership.rs::class_bits` owns natural-class eligibility.
`CharDef::membership_bits` distinguishes provisional letters from authored phonemes.
`segment::nat_class_cd_set_with_constraints` computes membership; rule bridges consume
the same eligibility lane. `pg-foma/src/lower.rs::class_members` and
`structural_allomorph.rs::context_members` call the same membership owner.
Provisional letters are excluded from segment-list classes and feature classes requiring
at least one authored feature value. Unconstrained feature classes match every segment,
including provisional letters; a synthetic Type tag is not an authored feature condition.

## Coverage and verification

`pg-grammar/src/compile/tests.rs::provisional_letter_has_no_membership_in_feature_or_segment_classes`
checks both class kinds, including an explicitly named provisional member.
`provisional_and_featureless_letters_do_not_match_a_runtime_feature_environment` checks
actual parses, positive authored-feature controls, and a table exceeding 64 definitions.
`pg-foma/src/lower/tests.rs::provisional_class_membership_reuses_the_model_decision`
checks FST lowering, explicit-list exclusion, and alpha-bound feature omission.
These expectations are derived from ADR 0008. Managed and fix-removed results are recorded
in the udp-core lane report; these synthetic tests are not exported conformance fixtures.

`unconstrained_feature_classes_match_provisional_and_featureless_stems` pins the wildcard
exception with actual affixed parses. The complete Machine/staging replay collects every
word mismatch instead of stopping at the first, including the existing
`chained-output-feature-override-loss` wildcard regression.

XAMPLE/C# measurements and staged conformance fixtures come from a separate lane.
No XAMPLE measurement is claimed here. No Machine issue or fix PR was posted here because
network access is closed. The cross-engine discussion and staged fixtures remain follow-ups.
