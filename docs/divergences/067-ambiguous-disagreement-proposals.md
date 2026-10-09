# 067 — Ambiguous disagreement requires confirmed membership proposals

## Kind and status

Representational, oracle-proven candidate for the recorded explicit segment-class variant.
The implementation commit follows this research record.
Baseline: `129e6284a151bda03dee199d306cc381c635e0e8`.
Evidence class: **correctness/representability**, verdict **ConfirmOnly**.
The owning proposal and actual templated pipeline both require HC confirmation. No word is
accepted from the widened network alone. Resource containment uses the existing managed
9 GB cgroup and apply limits of 128 paths / 32 candidates, without raising any limit.
Production readiness is assessed separately; this entry makes no readiness promotion.
Machine issue: none; this is an FST construction, and the founding C# oracle behaves correctly.

## Oracle and authorability

Founding oracle: hc.dll, Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`.
FieldWorks: `089eb9027b6d81be7883c40960f0de3ffa04b699`, `HCLoader.cs:2745-2770`
loads agreeing/disagreeing alpha constraints; `2338-2344` loads iteration bounds;
`2003-2089` loads rewrite direction, focus, replacement and environments.
These are authorable, fully defined shapes, not provisional definitions.

The synthetic four-member vowel class has two members sharing each back/round value.
The rule flips backness at the first position and rounding at the second, retaining the
other feature. hc.dll requires `ia -> AU|ia` and rejects `au`. Nine fixtures cover the plain
control and bounded/unbounded × LTR/RTL × both environment sides; all 144 complete oracle
rows are preserved in entry 065's evidence and staged `nullable-disagree-*` fixtures.
The port's separate nullable-environment repair is recorded in entry 065.

Two further synthetic `partial-class-disagree-*` fixtures omit one explicit class member.
C# still requires `ia` and rejects `au`; all 32 complete rows are recorded in
`evidence/067-ambiguous-disagreement/`. HC-Rust agrees on every row. This is a remaining
FST refusal, not a permanent carve-out: widening the authored member list would need its
own proof of recall. FieldWorks loads explicit member lists in `HCLoader.cs:2799-2808`.

## Owning construction

`pg-foma/src/lower.rs::alpha_members` publishes one eligibility computation to both the
slot lowerer and its diagnostic walk. Ambiguous minus alpha is admitted only for an explicit
segment class covering the owning table's segments, binary governed features, and fully
specified governed values. Exact span intersection keeps its refusal.

`replace.rs::compile_rewrite_rule_subset` defers the subrule's alpha agreement to confirmation
and uses each occurrence's membership union. The required concrete input and output are in
those unions for the admitted shape. Optional per-site rewriting preserves both actual rewrites
and unchanged sites when the widened environment or membership overmatches. Confirmation
restores alpha agreement and other feature values; it prunes the overproposed alternatives.

A sequential cascade of 64 optional concrete tuples overflowed the test stack, including
an attempt that minimized intermediate networks. Those attempts were rejected. The membership
construction completes within the unchanged limits, avoiding that tuple cascade.

Partial segment classes, feature classes, nonbinary governed features and underspecified
governed values retain their ambiguity refusal. Inverted/empty repetition and missing owning
tables retain their existing refusal controls. The permanent reasons are recorded in entry 068;
its quantified focus/replacement owner gates remain a separate pending obligation.

## Demonstrated regression coverage

- `pg-foma/src/replace/owning_table_tests.rs::ambiguous_disagreement_matches_recorded_oracle_after_confirmation`
  replaces the obsolete cannot-lower expectation with the oracle-confirmed contract. It checks
  all sixteen plain fixture words, exact multisets, positive results, actual rejected proposals,
  complete bounded apply results, and measured HC confirmation calls whenever candidates exist.
- `ambiguous_disagreement_with_partial_class_stays_refused` retains both actual compiler
  refusal and the published owner fact, plus its typed diagnostic, for a remaining census shape.
- `pg-foma-backend/tests/ambiguous_alpha_containment.rs` checks capability admission, the raw
  owning cascade, and the actual templated compiler on all nine fixtures. Both parse tests check
  all 144 oracle rows and measured HC confirmation calls; no cap, skip or missing row counts as
  agreement. The existing twelve repeated-alpha fixtures still check 200 rows per pipeline.

The new confirmed-contract owner test and each of the three backend regression tests fail with
both lowering owner files restored to baseline and pass with the lowering restored. The partial
class refusal control passes on both versions; it is a detection control, not a red-on-revert claim.
Exact gates, per-test revert results and coverage counts are in the lane report.

Supported sibling rows gain the ambiguity containment citations. The three aggregate unlowerable
rows remain unresolved while authorable partial classes still refuse. No aggregate obligation is
silently closed and no permanent-refusal reason is inferred for these authorable controls.
