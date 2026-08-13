# Planner-owned pipeline health

## Why

The existing health work correctly measures compile and HC work, but its older design can independently
derive a “best backend” and interpret capability. That would create a second selector beside the new
Machine-obligation strategy planner and allow reporting code to disagree with compilation.

The useful work is retained on `codex/profiler-health` at `45abaffc`: evidence identities,
deterministic/censored profiling, canonical health serialization, immutable acknowledgements, and
verified attention joins. It must be ported behind the new planner boundary rather than merged whole.

## What changes

Health consumes one authoritative `StrategyChoice`, a separate projected/measured cost envelope,
CandidateFilter evidence, and HC-authoritative confirmation/profile evidence. It reports requested and
realized physical strategy, operational cost, filtering work, confirmation work, censorship, budgets,
and incomplete outcomes.

Health does not select a backend, classify semantic support, maintain a completeness denominator,
validate filter proofs, or turn cost severity into `Admit | ConfirmOnly | Refuse`. Attention and
acknowledgements inherit the same boundary.

Machine's atomic, within-rule configuration, cross-rule interaction, and schedule obligation IDs are
the semantic identity source. PanGloss health may cite those IDs and their pinned catalog revision; it
does not redefine or count them.

## Dependencies

1. Accepted circumfix/filter integration tip.
2. Pinned Machine semantic catalog revision and imported obligation IDs.
3. Deep StrategyPlanner interface and separate CostEnvelope.
4. CandidateFilter observation contract.
5. HC-authoritative semantic comparison.

## Source-material policy

Do not merge `codex/profiler-health` wholesale. Port the audited slices in the order recorded in
`docs/superpowers/plans/2026-08-13-planner-owned-health-reintegration.md`. The branch remains an idea
source; its health-owned selection logic is obsolete.

## Non-goals

- Choosing or recommending a semantic backend.
- Certifying feature support or semantic completeness.
- Making health severity a compiler admission decision.
- Treating CandidateFilter savings as correctness evidence.
- Recalibrating thresholds before multi-adapter measurements exist.
