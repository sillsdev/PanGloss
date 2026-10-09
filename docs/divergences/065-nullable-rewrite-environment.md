# 065 — A nullable environment is rejected at the word edge

## Kind and status

Behavioural, Rust-only defect; fixed and oracle verified in this branch.
Baseline: `5946d07bedd51ff05c4647ec0fb0b22fb52bd1ac`.
Founding oracle: hc.dll, Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`.
FieldWorks source: `089eb9027b6d81be7883c40960f0de3ffa04b699`.
Evidence classification: **correctness/representability**. No readiness policy or limit change.
Machine issue: none; C# behaves correctly, and network access is closed.

## Reproduction and diagnosis

The ambiguous-disagreement probe recorded in entry 058 has underlying `au`.
Its four-member vowel class varies in backness and rounding, so each alpha-governed
feature value is shared by two members. The rule flips backness at the first position
and rounding at the second, preserving the other feature: the required surface is `ia`.
The right environment is a vowel repeated zero or one times followed by a word edge.

| Word | hc.dll | HC-Rust baseline |
|---|---|---|
| `ia` | `ok`, `AU\|ia` | `ok`, `-` |
| `au` | `ok`, `-` | `ok`, `AU\|au` |

Both engines complete all sixteen two-segment words; fourteen other rows agree.
The plain control without an environment agrees on all sixteen rows. Bounded/unbounded,
LTR/RTL and left/right controls reproduce the same two discrepancies.
This isolates the failed existence check to the nullable environment at an empty
word-edge context; the disagreement variable itself is already resolved correctly.

`pg-rules/src/rewrite.rs::left_env_match` and `right_env_match` call `empty_env_match`
when their segment context is empty. That helper accepted only a recursively nullable
agreement tree, which is retained only for variables inside repetitions (entry 058).
The alpha-free repetition in this witness therefore returned no environment match.
The rewrite never ran, which both lost `ia` and wrongly retained `au`.

The owner fix reads the already compiled FST's start-state `accepting` flag, the same
fact used by `pg-fst/src/traverse.rs::check_accepting_start_state`. It removes the
bridge's independent quantifier-nullability computation. An accepting empty context
consumes no variable occurrences, so the existing binding handoff preserves its bindings.
The frozen FST traversal contract is unchanged.

C# evidence: `PhonologicalRules/RewriteRuleSpec.cs:83-112` matches both environments
with the target's bindings; `RewriteSubruleSpec.cs:19-32` creates both direction-specific
matchers; `FiniteState/TraversalMethodBase.cs::CheckAcceptingStartState` accepts the
nullable start state. FieldWorks evidence: `HCLoader.cs:2338-2344` constructs a Quantifier
from authored bounds; `2745-2770` constructs agreeing/disagreeing alpha variables;
`2085-2086` installs the boundary condition. These are authored, fully defined shapes.

## Fixtures and regression coverage

Nine synthetic, well-formed HC XML fixtures are staged under
`conformance-staging/edge-cases/nullable-disagree-*`: one plain control and
bounded/unbounded × LTR/RTL × left/right (eight regressions).
Each `words.yaml` records sixteen hc.dll word rows, complete with status and signature;
provenance names the oracle and exact Machine revision. No expectation is hand derived.

Eight named `nullable_disagree_*` tests in `pg-parse/tests/conformance_fixtures_gate.rs`
fail on the baseline before the fix and each fails again with both owner files restored
to that revision. With the fix restored, full local conformance passes. The nine
oracle controls complete all 144 word rows with exact C#/HC-Rust status and signature
agreement. Per-test checks and the both-root parse-path run are recorded in the lane report.
The plain control is a diagnostic control, not a claimed red-on-revert regression.

Recorded oracle evidence and generator: `evidence/065-nullable-environment/`.

No FST admission follows from this port fix alone. Lowering, containment evidence,
permanent-refusal detection tests and coverage obligations have separate proof requirements.
