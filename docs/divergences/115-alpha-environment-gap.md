# 115 — Alpha-variable environment matching loses alternatives

Kind: behavioural.
Status: fixed-in-rust.

Machine `RewriteRuleSpec.MatchSubrule` seeds the left environment from target variable bindings, then passes the resulting bindings to the right environment. The left matcher runs right-to-left; the right runs left-to-right. PanGloss now mirrors that binding-aware traversal, checking agreement as each branch is matched. Previously it selected an initial environment candidate before checking agreement, potentially losing a later valid candidate exposed by the allomorph confirmation port.

The Mbugwe HC XML comparison against Machine `c2454760` found 19 completed words whose analyses were lost by the allomorph port but retained by C#. See [113](113-analysis-allomorph-confirmation.md) and the `mbugwe-548-oracle.md` investigation report for the observations. The binding-aware matcher recovers these alternatives. `rust/crates/pg-rules/tests/alpha_gate.rs` covers optional candidates, shared left and right bindings, repeated variables, and bounded-work pruning. Some Mbugwe words still hit the default search cap; FST support remains open in [116](116-fst-alpha-environment-gap.md).
