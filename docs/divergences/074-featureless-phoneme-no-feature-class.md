# 074: Featureless authored phonemes do not match classes requiring feature values

Kind: behavioural.
Status: open — implemented in Rust; upstream discussion open: [sillsdev/machine#537](https://github.com/sillsdev/machine/issues/537).
Evidence: ADR 0008 and code-constructed snapshot regressions.

## C# site

Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`,
`src/SIL.Machine.Morphology.HermitCrab/CharacterDefinitionTable.cs::Add`:
a segment with no supplied features carries Type and StrRep but unspecified phonological
values, allowing analysis-side feature unification. PanGloss excludes this phoneme from
classes requiring at least one feature value instead of using full-mask defaults as membership.
The lead's separate-lane measurement reports that C# subsumption in environments and synthesis
also excludes a featureless x from V=[voc +]; the divergence is analysis-side unification.

## Rust site

`pg-grammar-model/src/chardef.rs::CharDef::has_authored_features`, `membership_bits`;
`membership.rs::class_bits`; `segment.rs::nat_class_cd_set_with_constraints`.
Authored featureless phonemes keep segment-list memberships naming them. Grammar health
reports `provisional.phoneme-features` at Info severity only when at least one natural class
requires a feature value. It reports one finding per affected phoneme, naming it and explaining
how to assign features in FieldWorks. Unconstrained classes and segment lists alone trigger no
featureless-phoneme finding; health calls the membership owner's feature-condition decision.
Unconstrained feature classes admit every segment, including featureless phonemes and
provisional letters. A synthetic Type tag does not count as an authored feature condition.

## Coverage and verification

`pg-grammar/src/compile/tests.rs::featureless_authored_phoneme_keeps_list_membership_but_no_feature_class_membership`
checks the excluded feature membership and retained explicit membership.
`provisional_and_featureless_letters_do_not_match_a_runtime_feature_environment` pins
the parse consequence and a positive control with authored features.
`pg-foma/src/lower/tests.rs::provisional_class_membership_reuses_the_model_decision`
also excludes featureless phonemes when alpha binding omits a feature constraint, while
retaining their explicit-list membership. Expectations derive from ADR 0008.
Managed runs and fix-removed checks are in the udp-core lane report.
`unconstrained_feature_classes_match_provisional_and_featureless_stems` pins the empty-class
exception and retained affixation. The complete fixture replay also pins the existing
unconstrained `ncAny` in `chained-output-feature-override-loss`.
`featureless_phoneme_info_requires_a_feature_condition` pins no finding for a wildcard-only
grammar, one finding per featureless phoneme with one or two feature-requiring classes, no
duplicate finding for provisional letters, and the checked-code report round-trip.

XAMPLE/C# measurements and staged conformance fixtures come from a separate lane.
These are synthetic regressions, not independent C# measurements or exported fixtures.
The C# analysis-side inconsistency (featureless segments unify with feature classes when a rule
is unapplied, though environments and synthesis use subsumption) is raised upstream in
[sillsdev/machine#537](https://github.com/sillsdev/machine/issues/537), posted 2026-10-08. Track the disposition there.
