# 059: Construct variants FieldWorks cannot author stay refused by the fast path

Kind: unported (fast path only).
Status: permanent refusal — owner decision of 2026-10-08 ("won't fix").
Evidence: `evidence/059-fieldworks-unauthorable-variants/` (source review and real-engine runs).

## What is refused

Four construct variants are refused by the FST fast path and always parsed by the HermitCrab path
instead. No parse changes: the refusal only decides which engine answers.

| Variant | Why it is permanent |
|---|---|
| `circumfix.no-structural-route` | FieldWorks' HCLoader never emits the dropping circumfix role it needs (`HCLoader.cs:1273-1311,1334-1420`). HC-XML only. |
| `reduplication.no-proposal-route` | Needs a reduplicating realizational rule, which HCLoader does not emit (`HCLoader.cs:976-979`). HC-XML only. |
| `metathesis.left-to-right-unlowerable` | FieldWorks rejects iteration or sequence contexts in a metathesis description and offers no variables. A rule saved with an alpha variable crashes HCLoader ([LT-22826](https://jira.sil.org/browse/LT-22826)); a corrupt switch index exports malformed XML ([LT-22827](https://jira.sil.org/browse/LT-22827)). C# HermitCrab cannot load any of these shapes. |
| `metathesis.right-to-left-unlowerable` | Same as left-to-right. |

The test the owner set: a variant is fixed only if a FieldWorks user can author it and C#
HermitCrab parses it. These four fail that test. The variants that pass it (right-to-left rewrite
and both quantifier variants) are worked separately: entries 058 and 065.

## C# site

FieldWorks `HCLoader.cs` (`LoadMetathesisRule` 2103-2150, affix-process loading 1273-1420);
Machine `AnalysisMetathesisRuleSpec.cs:20-52` (switch-group children cast to constraints) and
`XmlLanguageLoader.cs` (empty variable map for metathesis).

## Rust site

`pg-foma/src/capability/variants.rs::permanent_refusal_reason` names each variant's reason and
points here. `pg-foma-backend` coverage ledger counts a documented permanent refusal as a met
obligation (design D7 of the archived construct-coverage plan).

## Coverage and verification

- Reduplication: staged fixture `conformance-staging/edge-cases/realizational-reduplication-no-proposal-route/`,
  recorded from hc.dll, plus an exact-variant refusal check.
- Metathesis: Rust-only detection tests in `pg-foma-backend/tests/conformance_coverage_gate.rs`
  (`metathesis_{ltr,rtl}_unlowerable_is_a_documented_rust_only_refusal`). They are not oracle
  fixtures: C# throws while building the parser, so there is no C# answer to record.
- Circumfix: its reason is pinned, but no grammar reaches this refusal through PanGloss's own
  classifier today, so there is no detection fixture. If one appears, it needs a fixture then.
- Each test was shown to fail with its reason removed.

## Upstream

FieldWorks: LT-22826, LT-22827 (robustness; neither is an authoring path). No Machine change is
proposed: Machine's refusal of quantified switch groups matches what FieldWorks can produce.
