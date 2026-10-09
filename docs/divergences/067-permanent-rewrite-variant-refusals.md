# 067 — Permanent refusal reasons for unauthorable rewrite variants

## Kind and status

Representational, documented permanent carve-outs under archived coverage design D7.
The existing inverted-repeat, empty-repeat, and no-owning-table refusals have owner-level
detection tests. Quantified focus and replacement are C#-unsupported positions; their
categorical owner refusal remains an implementation obligation, because alpha-free targets
currently compile in the FST path. This entry does not claim those two gates are implemented.
No authorable ambiguous-disagreement or repeated-alpha environment is carved out.
Evidence class: **correctness/representability**, verdict **Refuse** for the implemented cases.
No limit or readiness policy changes. Machine issue: none; these are domain carve-outs,
and the founding oracle's behavior is recorded rather than replaced.

## C# site

Oracle hc.dll, Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`.
FieldWorks source `089eb9027b6d81be7883c40960f0de3ffa04b699`.

| Reason | C# observation and source | FieldWorks evidence | Permanent disposition |
|---|---|---|---|
| Inverted repeat | `min=3,max=2` contributes exactly three mandatory copies; `eccct` parses ROOT3. `Quantifier.GenerateNfa` emits the required minimum before optional copies. The three direction/bound controls exit 0, seventeen words each. | `HCLoader.cs:2343` passes stored bounds, but `OccurrenceDlg.cs:217-223` refuses an inverted authoring interval; `RegRuleFormulaControl.cs:731-742` stores the accepted interval. | Refuse malformed stored bounds; C# permissiveness does not make the interval authorable. |
| Empty repeat | Empty body is epsilon; `acet` parses ROOT1. Three controls exit 0, seventeen words each. `HermitCrabInput.dtd:560` requires nonempty child content. | `HCLoader.cs:2321-2336` rejects an empty sequence, `2338-2346` creates a quantifier only with a successfully loaded child. | Refuse the empty body; it is outside authored XML and FieldWorks loading. |
| No owning table | An unattached rule is inert; raw ROOT1/ROOT2/ROOT3 forms parse and rewritten forms do not. Three controls exit 0, seventeen words each. | `HCLoader.cs:227-233` creates strata with the owning table; `310-318` attaches loaded rewrite rules. | Refuse the structural call without an owning stratum/table; never guess table zero. |
| Quantified focus | All three controls fail C# rule compilation, exit 255. `PatternNodeCastExtensions.cs:21-39` permits quantified groups in environments, not target node casts. | `HCLoader.cs:2033-2040` reads focus as `IPhSimpleContext`, excluding iteration contexts. | Permanent unsupported position; categorical FST detection is still pending. |
| Quantified replacement | All three controls fail C# rule compilation, exit 255, at the same node cast boundary. | `HCLoader.cs:2060-2067` reads replacement as `IPhSimpleContext`, excluding iteration contexts. | Permanent unsupported position; categorical FST detection is still pending. |

Original minimal XML/words generator and complete C# records:
`evidence/058-variant-lowering/probe.py` and `oracle-results.json`; all fifteen individual XML,
word lists, results and rejection logs are preserved in `evidence/067-permanent-refusals/`.
These inputs are synthetic and well-formed
XML; empty and nested bodies can still violate the stricter DTD, as explicitly recorded.

## Rust site

`pg-foma/src/lower.rs::slots_from_nodes` owns inverted-bound and empty-body detection.
`pg-foma/src/replace.rs::owning_table` owns rule/stratum/table resolution;
`rewrite_rule_is_lowerable` and `compile_rewrite_rule` consult those owners.
The remaining target-position obligation belongs to the same pattern scope, not a new
capability predicate or a reproduced loader condition.

## Detection coverage

`pg-foma-backend/tests/permanent_variant_refusals.rs` asserts the published lowerability
fact and the actual compiler refusal for each of the first three reasons. It mutates the
oracle-recorded alpha-free bounded-right fixture, preserving unrelated rule structure.
The unmodified rule must lower and actually compile before each refusal mutation.
These are detection controls for existing permanent refusals, not new semantic-regression
claims: they must also pass with the repeated-alpha admission removed.
Quantified focus and replacement still need their own categorical detection tests.

The aggregate unlowerable variant rows remain open while an authorable disagreement case
still refuses. A permanent-refusal reason may close the remaining rows only after the
authorable reasons are admitted and tested. Coverage counts and exact managed gates are
recorded in the lane report.
