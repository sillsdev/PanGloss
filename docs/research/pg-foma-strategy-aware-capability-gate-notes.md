# pg-foma strategy_aware_capability_gate.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/tests/strategy_aware_capability_gate.rs`
implementation comments so the source can carry a one- or two-line pointer instead of the full
argument.

## The defect this test file pins

`capability::Disposition::ConfirmOnly` is defined as "recall-preserving only if the proposer
proposes the superset." That precondition is a claim about a *proposer*, but the capability layer
had no proposer in hand: `characterize(g: &Grammar)` took no strategy, and `EmissionStrategy`
appeared nowhere in `capability.rs`, `coverage_ledger.rs`, `conformance_coverage.rs`, or `gate.rs`.
So a `ConfirmOnly` disposition was checked against the *union* of every compiler's abilities.

The consequence was measured, not hypothesized: `Compounding` rested at a non-refusing disposition
while `crate::uflexc` — the only lexicon emitter `EmissionStrategy::PlanComposed` has — emitted a
structurally single-root continuation graph that could not propose any compound. One compiler's
coverage was silently inherited by all three, and the ledger's cited evidence for it
(`tests/cover_compounding.rs`) exercised only `FomaAnalyzer::new`, i.e.
`EmissionStrategy::TunedSurfaceProbed`.

That specific hole is now fixed (`uflexc` grew a bounded compound loop). This file pins the
*accounting*, against a hole of the identical shape that is still live: `MorphRuleDef::Realizational`.
`uflexc`'s mrule loop reports every such rule in `skipped` as `kind=realizational-rule` and
`continue`s past it — no lexc line is written for the rule at all, so `PlanComposed`'s proposer
returns zero candidates for any word requiring it, while both whole-grammar compilers handle it
through `emit.rs`'s shared rule accessors.

Synthetic, delanguaged fixtures only (this repo's standing rule for conformance-shaped grammars).
