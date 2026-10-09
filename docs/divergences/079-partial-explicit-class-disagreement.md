# 079 — Partial explicit classes for ambiguous disagreement proposals

## Kind and status

Representational; implemented in this branch for explicit segment classes. The widening is
**ConfirmOnly**: FST paths are proposals and every accepted analysis is checked by HC-Rust.
This is a correctness/representability change, not a production-readiness promotion or a
resource-containment change. Existing FST budgets and thresholds are unchanged.

Machine issue: none. The founding oracle correctly handles these authorable rewrite rules;
the missing behavior was in proposal construction.

## Oracle and authorability

Founding oracle: `hc.dll`, Machine
`18cf242f4b114b0eb9bac304b4b171ca2f499a39`. FieldWorks loads rewrite direction, bounds,
focus, replacement and environments in `HCLoader.cs:2003-2089,2338-2344,2745-2770`, and
explicit segment-class member lists in `HCLoader.cs:2799-2808`.

Entries 067's two partial-class fixtures each omit one member and retain their original 32
oracle rows in `evidence/067-ambiguous-disagreement/`. Four new fixtures omit two members:

| Fixture | Direction | Bound | Environment | Authored members (omitted table members) |
|---|---|---|---|---|
| `partial-class-disagree-bounded-ltr-left` | LTR | bounded | left | `i, a` (`u, y`) |
| `partial-class-disagree-bounded-rtl-left` | RTL | bounded | left | `a, u` (`i, y`) |
| `partial-class-disagree-unbounded-ltr-right` | LTR | unbounded | right | `i, a` (`u, y`) |
| `partial-class-disagree-unbounded-rtl-right` | RTL | unbounded | right | `a, u` (`i, y`) |

Each new fixture has sixteen complete `hc.dll` rows, recorded in
`evidence/079-partial-explicit-class/`; all four recorder runs exited 0 with zero skipped
words. Every fixture accepts `ia` as `AU|ia` and rejects the other fifteen words.

## Recall argument and lowering

The earlier full-table membership proposal was recall-safe but overbroad. Requiring an authored
class to list every table member was unnecessarily restrictive. Machine's `SegmentNaturalClass`
merges its authored members' `FeatureStruct`s lane by lane; conflicting symbolic values form a
union, while a lane missing from any member is dropped. The projection computes those unions from
the authored members only, then removes the governed alpha lanes before building proposal
membership. A table segment remains when each constrained lane overlaps its authored union, or
when the segment itself leaves that lane unspecified. The correlated-member unit test pins a
segment whose values come from different authored members.

This lane-wise projection is a superset of every segment the explicit class can match after the
ambiguous alpha lanes are deferred. It may admit combinations that no one authored member has,
but it does not lose valid combinations allowed by Machine's merged feature structure. Missing
values remove constraints rather than narrowing the set. Confirmation reapplies the original
class and alpha constraints and removes false positives. The projection never obtains members
from a feature natural class.

`lower.rs::alpha_members` now accepts a nonempty partial `Segments` class only when the existing
ambiguity constraints also hold: this rewrite scope, binary governed features, and fully
specified governed values. Feature natural classes, nonbinary governed features and
underspecified governed values still refuse. `project_explicit_members` supplies the
per-occurrence projected alphabet to deferred ambiguous slots; feature classes retain their
existing refusal and are not widened through this path.

## Demonstrated regression coverage

- `pg-foma/src/replace/owning_table_tests.rs::ambiguous_disagreement_with_partial_class_matches_recorded_oracle_after_confirmation`
  compiles all six partial-class fixtures and compares the confirmed multiset for all 96 words
  against their recorded oracle rows. The test requires positives and measured confirmation
  pruning. It fails when the lowering owner is restored to baseline.
- `pg-foma/src/lower/tests.rs::explicit_segment_projection_preserves_lane_wise_feature_union`
  fails with the existential-member projection and passes with the lane-wise union, proving that
  the proposal keeps a cross-member feature combination while still excluding an out-of-class
  lane value.
- `ambiguous_disagreement_still_refuses_unsupported_class_shapes` retains the refusal and typed
  `AlphaAmbiguousDisagree` diagnostic for a feature class, a nonbinary governed feature and an
  underspecified governed value.
- `pg-foma-backend/tests/templated_conformance_proposal_pins.rs::partial_class_disagreement_matches_oracle_after_confirmation`
  confirms the same six fixtures through the templated pipeline. It fails with the baseline
  lowering owner.
- `pg-foma-backend/tests/ambiguous_alpha_containment.rs` pins capability admission, owning
  proposal/confirmation, and templated confirmation for all six fixtures. Its three new tests
  fail with the baseline lowering owner. Both parse paths check 96 words and six positive rows.
- The scoreboard changes only the six `TemplatedUnderlyingTokens` outcomes: the two existing
  partial fixtures and four new fixtures move from `refused` to `oracle_exact`. The four new
  fixtures add four exact `TunedSurfaceProbed` rows; the two existing rows were already exact
  for that strategy. `PlanComposed` continues to refuse all six. A before/after comparison of
  the 324 pre-witness-fixture cells with the lowering restored found no other outcome changes.
  The two feature-class witness fixtures add six new strategy cells without changing existing
  outcomes: TSP is exact, while PlanComposed and TUT refuse.
- The unsupported feature-class boundary has two additional C#-recorded staged witnesses:
  `ambiguous-disagree-feature-class-bounded-rtl` and
  `ambiguous-disagree-feature-class-unbounded-ltr`. Their recorded `ia` parse and `au` rejection
  exercise bounded RTL and unbounded LTR shapes while `PlanComposed` continues to report the
  quantifier refusal; the bounded RTL grammar also witnesses the RTL refusal. Their complete
  records are under `evidence/079-partial-explicit-class/refusal-witnesses/`. The negative-witness
  gate pins those fixture/predicate pairs, while the owner test still pins refusal for a feature
  class, a nonbinary governed feature, and underspecified governed values.

The handoff report records the exact `pg.ps1` gates, exit codes, per-test revert-checks and any
remaining limitations.
