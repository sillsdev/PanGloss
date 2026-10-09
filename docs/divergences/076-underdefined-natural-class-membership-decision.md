# 076: Natural-class membership in underdefined projects

Kind: behavioural.
Status: deliberate divergence — upstream discussion open: [sillsdev/machine#537](https://github.com/sillsdev/machine/issues/537).
Evidence: owner decision of 2026-10-08, ADR 0008 amended in PanGloss `342e6957`
on `feat/underdefined-base`; separate-lane C# measurements and synthetic Rust regressions.

## Owner decision

- A provisional letter belongs to no class that names letters or requires a feature value.
- An authored phoneme with no features belongs to no class that requires a feature value.
  It stays in segment-list classes that name it.
- A class with no conditions matches both. The synthetic Type=Segment tag is not an
  authored feature-value condition; such a class remains a wildcard over segments.

The owner further directed that `provisional.phoneme-features` Info is emitted only when
the grammar contains at least one class requiring a feature value. Each affected featureless
phoneme receives one finding, regardless of class count. A wildcard-only or segment-list-only
grammar does not produce that finding; the membership owner supplies the triggering fact.

This entry records the combined membership decision in addition to the individual
provisional-letter and featureless-phoneme entries [073](073-provisional-letter-no-natural-class.md)
and [074](074-featureless-phoneme-no-feature-class.md). Letter-unit and always-on behavior
remain separate decisions, [072](072-provisional-letter-unit.md) and
[075](075-provisional-definitions-always-on.md).

## Measured C# behavior and remaining divergence

The separate XAMPLE-measurement lane reports that C# HermitCrab subsumption excludes
featureless segments from constrained classes in environments and synthesis: a featureless
`x` is not subsumed by `V=[voc +]`. Its analysis-side unification includes those segments
and yields an extra root analysis. PanGloss excludes them on that analysis path as well.
**The remaining C# membership divergence is the analysis-side unification path**, not the
environment or synthesis subsumption paths. Unconstrained wildcard matching is preserved.
C# does not supply PanGloss's provisional letter definitions; that separate behavior is
covered by the linked entries above.

Reported staged witnesses on `feat/xample-measure`:

- `conformance-staging/underdefined/07-featureless-phoneme`
- `conformance-staging/underdefined/12-featureless-rule-class`

Their measurements and staged artifacts are owned by the separate lane. That branch is
not available as a local ref in this worktree, so this entry records the lead's supplied
provenance; it does not claim these fixtures were present or replayed here. This lane has
not executed XAMPLE or C#. Machine source revision inspected locally is
`18cf242f4b114b0eb9bac304b4b171ca2f499a39`; no measurement revision is inferred from that inspection.

## Rust site and wildcard regression

`pg-grammar-model/src/membership.rs::class_bits` is the single eligibility owner;
`chardef.rs::CharDef::membership_bits` supplies definition status and
`segment.rs::nat_class_cd_set_with_constraints` resolves membership. The rule bridges in
`pg-rules/src/bridge.rs`, `morph.rs`, and `rewrite.rs` share that decision. Foma's
`lower.rs::class_members` and `structural_allomorph.rs::context_members` call the model owner.

The literal rewrite identity encoding from [071](071-strrep-rewrite-unapplication.md) follows
the shared matching lanes, including eligibility. Synthesis takes the replacement definition's
`CharDef::membership_bits`; inverse widening retains the input definition's bits as well.
The integration regressions `strrep_rewrite_preserves_provisional_named_class_exclusion` and
`strrep_rewrite_tracks_membership_when_a_provisional_literal_changes_identity` cover exclusion
and both rewrite directions. Each failed with its corresponding integration fix removed.

An intermediate implementation excluded featureless segments from an authored empty
FeatureNaturalClass because the loader adds Type=Segment. This broke
`machine:edge-cases/chained-output-feature-override-loss`, word `zudi`: baseline/oracle
`["ZUD+INNER|zudi"]`, intermediate Rust `[]`. The shared owner now distinguishes the
synthetic Type tag from an authored feature condition, restoring the unconstrained wildcard.
No fixture expectation was changed.

Implementation and pinning tests are on `feat/provisional-definitions`, based on PanGloss
`5c51a38d26856285295a28bc91d5445c1ee1f7b2`. The udp-core lane report records the implementation
commits, managed gates and fix-reverted checks separately from the other lane's staged witnesses.

## Pinning tests and demonstrated coverage

`pg-grammar/src/compile/tests.rs`:

- `provisional_letter_has_no_membership_in_feature_or_segment_classes`: provisional
  exclusion from both a constrained feature class and an explicit member list.
- `featureless_authored_phoneme_keeps_list_membership_but_no_feature_class_membership`:
  constrained-class exclusion with retained named-list membership and an Info finding.
- `unconstrained_feature_classes_match_provisional_and_featureless_stems`: authored empty
  class admits both definitions and permits actual affixation. It failed with the previous
  owner and passed after the wildcard correction.
- `provisional_and_featureless_letters_do_not_match_a_runtime_feature_environment`:
  constrained environment excludes both; authored feature-valued positive control matches,
  including a table with more than 64 definitions.
- `featureless_phoneme_info_requires_a_feature_condition`: no featureless-phoneme Info
  without a feature-requiring class, one per affected phoneme with one or two such classes,
  and a typed report round-trip preserving the new checked code.

`pg-foma/src/lower/tests.rs::provisional_class_membership_reuses_the_model_decision`
pins constrained-class/list eligibility and alpha-bound constraint omission.
The prior lowerer was restored to verify this regression fails, then the owner call restored
to verify it passes. The grammar membership/runtime regressions likewise failed with
their eligibility fix removed; exact evidence is in the udp-core lane report.

`pg-parse/tests/conformance_fixtures_gate.rs::all_discovered_fixtures_match_oracle`
collects every mismatch over the full replay, including the `zudi` wildcard regression.
Before correction: 130 word mismatches across 29 fixtures in the 727-word, 71-fixture
inventory. After correction: zero mismatches, with unchanged expectations. Three existing
pathological/crash fixtures are outside generic replay. The complete census and managed
command results are in `/tmp/pangloss-lanes/udp-core.md` and its referenced logs/CSV.
This replay does not include the separate branch's two reported staged witnesses.

## Literal surface matching and engine parity

`pg-parse/src/surface.rs::matching_reps_for_node` selects literal representations using
`CharDef::literal_constraint_lanes`, whose eligibility lane is owned by
`pg-grammar-model/src/membership.rs::literal_lanes`. Natural-class eligibility remains enforced
by the class's model-owned character-definition set; literal representation selection preserves
phonological constraints without treating a segment's definition status as a class condition.

Comparing representation candidates with `CharDef::matching_lanes` instead made a provisional
`qtu` render `+|q+?[ktmau][ktmau]`, while an authored featureless `qtu` rendered
`+|[ktmauq]+?[ktmauq][ktmauq]`. This was a Rust implementation bug within the approved wildcard
contract, not an additional deliberate divergence. The focused surface test
`literal_surface_matching_preserves_provisional_and_featureless_wildcards` fails with that call
restored and passes with literal constraints. It also verifies that a feature-constrained class
still excludes both provisional and featureless phonemes while admitting a valued control.

`pg-cli/tests/inferred_segment_engine_parity_gate.rs` compares complete HC-Rust and actual
foma-propose+HC-confirm signatures for seven words across provisional, featureless, and
Front-valued `q` grammars. Provisional and featureless signatures agree on every word;
`qtu`/`kumatu` supply wildcard positives, and only valued `q` satisfies the constrained `qta`
environment. Restoring the surface implementation makes this gate fail on `qtu` again.
These are synthetic Rust regressions, not new exported conformance fixtures or C#/XAMPLE
measurements. Exact runs, feature activation, and the unchanged full conformance census are
recorded in `/tmp/pangloss-lanes/udp-parity.md`.

## Upstream status and evidence limits

This is an owner-authorized deliberate divergence under amended ADR 0008, not a claim
that C#'s analysis result is a reproduced upstream bug. Discussion is open in
[sillsdev/machine#537](https://github.com/sillsdev/machine/issues/537) (posted 2026-10-08), which asks whether the analysis-side
unification is intended and whether a provisional-definition policy belongs upstream. Track the
disposition there and update this entry when it resolves.
The separate lane must preserve exact oracle revisions and witness outputs when integrating
its staged fixtures. Fixture presence, measurement provenance, implementation and demonstrated
regression coverage remain distinct claims.
