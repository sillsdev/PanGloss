# 069 — Analysis binds an alpha feature overwritten by the replacement

## Kind and status

Behavioural, Rust-only parity defect repaired in its analysis owner. This was the third
independent port discrepancy found in this lane; the lead authorized this repair.
The founding C# behaviour and oracle expectations are preserved.
Evidence class: **correctness/representability**. Limits and readiness policy are unchanged.
Machine issue: none; C# gives the required answer, and network access is closed.

## Founding oracle and reproduction

hc.dll, Machine `18cf242f4b114b0eb9bac304b4b171ca2f499a39`.
FieldWorks `089eb9027b6d81be7883c40960f0de3ffa04b699`,
`HCLoader.cs:2033-2067,2745-2770,2799-2808`, loads focus and replacement contexts,
signed alpha variables, and explicit natural classes. The plain complete-class control
requires neither a quantifier nor a partial class and is an authorable, fully defined shape.

The synthetic lexical root is `iu`. A single vowel focus binds plus alpha backness;
its initial left environment is a vowel with minus alpha backness. Replacement is the
literal segment `i`, which overwrites the governed backness. C# produces `ii`.

| Word | hc.dll | HC-Rust |
|---|---|---|
| `ii` | `ok`, `IU\|ii` | `ok`, `-` |

Both engines complete all sixteen two-vowel words. The other fifteen rows agree.
The staged `ambiguous-disagree-repeated-minus-partial-focus` fixture uses a one-copy
quantified environment and a partial focus class. Three controls reproduce exactly the
same discrepancy: plain/partial focus, plain/complete focus, repeated/complete focus.
Each control completes sixteen rows in both engines with no cap, timeout or skip.
Thus this is independent of the earlier repeated-agreement and nullable-environment fixes.

Raw XML, words, C# and Rust TSVs and managed run logs are in
`evidence/069-overridden-alpha/`. The staged fixture records hc.dll provenance and
fails the existing `all_discovered_fixtures_match_oracle` gate on `ii`. No oracle parse
expectation was changed to match the port.

## Owner diagnosis

C# `PhonologicalRules/FeatureAnalysisRewriteRuleSpec.cs:52` creates the analysis target
with the RHS priority-union over the LHS. Match bindings therefore come from the effective
analysis target, where this literal replacement has overwritten the LHS alpha feature.
Its variable bindings are then threaded through environment matching.

HC-Rust `pg-rules/src/rewrite.rs::ana_feature` declares unfiltered LHS variable occurrences
at line 2019 and passes them to `resolve_bindings` at line 2075 before unapplication.
That binds the surface `i`'s backness as if it were the underlying focus's alpha value;
the left `i` then fails the minus agreement check. The accompanying comment promises
variables surviving on unchanged features, but the computation does not apply that filter.
The owner now computes effective target lanes and variable occurrences in one
`ana_feature_target` priority-union operation. RHS literal pins remove LHS variables
on that feature; RHS variables replace LHS occurrences on their governed feature.
Surviving LHS occurrences remain. `ana_feature` passes these effective occurrences
to `resolve_bindings`; synthesis continues to bind its original target.

## Oracle sweep and demonstrated regression coverage

Four synthetic, well-formed fixtures are staged as `overridden-alpha-{plain,bounded,
unbounded}-ltr-left` and `overridden-alpha-bounded-rtl-left`. Each records hc.dll
provenance and sixteen exact oracle rows, including `ii -> IU|ii`. Each named parser
regression fails with this owner repair reverted (exit 101, four failures) and passes
with it restored. The earlier partial-focus witness is preserved without changing
its oracle expectation.

`evidence/069-overridden-alpha/sweep-records.json` records 144 grammars: plain/bounded/
unbounded × replacement overwrites/retains alpha backness × plus/minus focus alpha ×
plus/minus environment alpha × left/right environment × LTR/RTL/simultaneous.
Every grammar has all sixteen synthetic two-vowel lexical roots and surfaces.
The retain control changes rounding only, so an unchanged governed feature actually
exercises surviving focus agreement. All 2,304 C# adapter rows and HC-Rust rows
complete and agree as exact signature multisets, with no caps, timeouts or skips.

The same record contains hc.dll tracing output for every grammar. These explicitly
observe all sixteen forward inputs and outcomes, including pattern refusal leaving
the input unchanged. The direct Rust rewrite-synthesis test compares all 2,304
observed root/output pairs against these C# traces. These are observed forward
outputs, not inferred or Rust-authored expectations.

`pg-parse/tests/effective_analysis_target.rs` replays both the oracle parse matrix
through the shared fixture assertion and the C# synthesis traces through the owning
rewrite API. The parse-matrix regression also fails with the fix reverted. The
synthesis sweep is a control, not a claim that this analysis repair changes synthesis.
Regeneration scripts and raw failing-first controls accompany the evidence.

No Machine issue or upstream fixture PR was created: the defect was in this port,
C# already behaved correctly, and network access is closed. Exact managed gates,
per-test revert checks and limitations are recorded in the lane report.
