# 115 — Alpha-variable environment matching loses alternatives

Kind: behavioural.
Status: open; a binding-aware matcher is planned in the follow-up.

Machine `RewriteRuleSpec.MatchSubrule` seeds the left environment from target variable bindings, then passes the resulting bindings to the right environment. The left matcher runs right-to-left; the right runs left-to-right. PanGloss currently matches environments before checking agreement, selecting an initial candidate that may disagree while a later valid candidate exists. The allomorph confirmation port exposes this pre-existing gap and can consequently lose analyses.

The Mbugwe HC XML comparison against Machine `c2454760` found 19 completed words whose analyses were lost by the allomorph port but retained by C#. See [113](113-analysis-allomorph-confirmation.md) and the `mbugwe-548-oracle.md` investigation report for the observations. The candidate-retry experiment in PR #13 recovered analyses but caused substantial search growth; it is not included in this split. Regression coverage and a binding-aware fix remain follow-up work.
