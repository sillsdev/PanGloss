# 028 — Confirmation-free accuracy screen: a soundness hazard, now measured

## Kind
Efficiency (a Rust-only evaluation-harness shortcut, not a shipped parser behavior; catalogued
because its soundness argument rests on an assumption about `pg-rules`' own dedup key that this
catalogue's other entries also touch).

## Status
Open — measured sound on current fixtures, not proven sound in general.

## C# site
Not applicable — this concerns only a Rust-only evaluation/measurement harness
(`pg_foma::recipe_accuracy`), not the shipped parser. No C# equivalent exists.

## Rust site
`pg_foma::recipe_accuracy` and `pg_foma::parity::IdentityDivergence`
(`rust/crates/pg-foma/tests/parity_divergence_census.rs`).

## What differs
`recipe_accuracy` detects FST-proposer **undergeneration** by checking that a candidate proposed the
admission key of every oracle analysis, performing **no full-HC confirmation** at all — a
sound-on-its-own test for undergeneration. It is equivalent to full certification only if the
opposite direction is also free: that a candidate's confirmed identity set can never contain an
identity the oracle's own (unrestricted) confirmation lacks.

The argument for that freedom is strong but not airtight, and the gap is specific and narrow:
`pg_rules::word::WordKey` (the analysis-search dedup key) deliberately excludes the syntactic feature
struct, while `pg_parse::identity::AnalysisIdentity::category` is projected from it via
`WordAnalysis::pos_id`. Two search states differing only in `syn_fs` can therefore collapse to one
map entry, and which one survives is decided first-wins by traversal order — an order the FST
proposer's *restriction* can perturb relative to the oracle's unrestricted search. So a restricted
run could in principle surface a category the unrestricted run's dedup silently discarded: a
candidate-only identity the accuracy screen would never notice, because it never confirms.

## Can it change a parse?
Not a parse the shipped engine produces — this concerns only whether the accuracy-measurement
harness could report a false "sound" verdict about a *different* FST-proposer strategy, not whether
`Morpher::parse_word` itself is affected. Framed the way the source itself frames it: "a non-zero is
a finding, not a nuisance: it would mean the parity relation and the compilation disagree about
analysis identity somewhere, which is worth more than any speedup."

## Evidence
`pg_foma::parity::IdentityDivergence::candidate_only_identities` is measured directly inside the
existing certification path (`certify_corpus`, sharing its one projection pass rather than
reimplementing a second one that could itself disagree), accumulated per run by
`RunEvaluationCache`. Per the design notes: a zero on current fixtures licenses exactly the claim
that, on those fixtures, confirmation never yielded an identity the oracle lacked — it does not
license removing confirmation from the certification path, and it does not make the accuracy verdict
a certification. `occurrences_compared` is asserted non-zero specifically so a refused/step-capped
run cannot report zero candidate-only identities by having compared nothing at all.

## Upstream
Not applicable.

## Notes
A good worked example of this repo's "a control that cannot act must say so" rule: the
`occurrences_compared` non-zero assertion exists precisely to stop a silently-refused comparison from
reading as "zero divergences found."
