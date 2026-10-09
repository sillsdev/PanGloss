# 066 — Repeated alpha environments require optional FST proposals

## Kind and status

Representational, implemented and oracle verified in this branch for repeated environment
alpha variables. Other reasons for the three unlowerable variants remain separate obligations.
Baseline: `1dca5968b06bf31783823f402463b3f2b258af3b`.
Evidence classification: **correctness/representability**, resulting verdict **ConfirmOnly**.
Resource containment: existing managed-build cgroup and finite apply budgets; no raised limits.
Production readiness: unchanged, assessed independently of representability.
Machine issue: none; this is an FST proposal construction, not a C# defect.

## C# site

Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`:
`Quantifier.GenerateNfa`, `PhonologicalRules/RewriteRuleSpec.cs:83-112` environment matching,
and `RewriteSubruleSpec.cs:19-32` direction-specific matchers preserve repeated agreement.
The owning HC-Rust agreement repair is recorded in entry 058; nullable environments in 065.
FieldWorks `089eb9027b6d81be7883c40960f0de3ffa04b699`,
`HCLoader.cs:2338-2344,2745-2770,2085-2086`, emits authored bounds, alpha constraints,
and word boundaries. This variant is authorable.

## Rust site

`pg-foma/src/lower.rs::PatternLowerScope::RewriteEnvironment` permits repetition of
`Slot::RepeatedAlpha` membership unions. `replace.rs::render_branch_regex` chooses optional
per-site rewriting when either environment contains such a slot. Compiler lowerability and
capability characterization use that same lowering scope. Exact span intersection and rewrite
targets retain their independent scope; ambiguous disagreement still refuses.

Each C#-valid repetition belongs to the independent membership language. Widening may add
matches, so mandatory rewriting could destroy a valid unchanged candidate. Optional per-site
rewriting includes every combination of actual rewrites and unchanged overmatched sites.
The confirmed output, rather than the proposal relation, must equal C#'s complete parse multiset.
No alphabet-wide fallback is used to make the containment test pass.

## Fixtures and regression coverage

Twelve synthetic, well-formed `conformance-staging/edge-cases/quantified-alpha-*` fixtures
record hc.dll provenance. They cover bounded/unbounded repetition, LTR/RTL, both environment
sides, zero-count contexts, and mixed agreeing/disagreeing feature values. Their 200 oracle rows
include unchanged roots, rewritten roots, and words whose overproposed candidates confirmation
must prune. Tags now name `quantifier.bounded-compilable`, `quantifier.unbounded-compilable`,
and `right-to-left-rewrite.reversal` where applicable; recorded parses are unchanged.

`pg-foma-backend/tests/repeated_alpha_containment.rs` has three regression tests:

- `repeated_alpha_environments_are_admitted_for_confirmation`: the quantifier and applicable
  RTL capability predicates return ConfirmOnly for all twelve fixtures, and the real templated
  backend's selection report admits their representation.
- `repeated_alpha_propose_confirm_matches_recorded_oracle`: the owning rewrite compiler builds
  with no skipped rules; all 200 confirmed multisets equal the recorded oracle. Apply budgets
  limit each word to 128 paths and 32 candidates; every result completes, and actual pruning
  and positive analyses are both required.
- `templated_repeated_alpha_matches_recorded_oracle`: the real templated compiler constructs a
  complete proposer; all 200 recorded rows complete within the same apply budgets and confirm
  exactly to C#'s recorded multisets.

All three tests fail with the owner lowering removed and pass restored. The initial default-backend
probe passed before the change and was discarded as evidence. PlanComposed's unrelated required
subtree materializer refuses these particular full grammars, so the capability regression checks
the owning variant predicates and the output regression composes the owning rewrite relation
directly. The separate templated test supplies actual whole-backend coverage.

Removing only optional rewriting also fails the output regression: mandatory rewriting loses
`MIXED|ttca` on `quantified-alpha-bounded-ltr-left`. The variant capability still admits this
temporary construction, so this is a direct check of proposal containment, not diagnostic text.

Gate results, differential counts in both directions, per-test revert checks, coverage obligations,
and the remaining permanent-refusal work are recorded in the lane report.
