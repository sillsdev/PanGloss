# `deadend_census` — dead-end attribution design

`rust/crates/pg-foma/examples/deadend_census.rs` attributes WHY failing proposer candidates die
during confirm, per grammar, weighted by wall time, into six causal buckets:

- d1: allomorph environment check failed against the intermediate shape.
- d2: disjunctive-allomorph block (first-match-wins picked a different allomorph than the
  candidate's segmentation needs).
- d3: feature-structure unification/subsumption/MPR clash between the pinned morphemes.
- d4: shape mismatch — no rule sequence reproduces the surface.
- d5: ordering/slot violation — stratum or template order excludes the pinned rule sequence.
- d6: other/unattributable (raw reasons stay visible so this bucket can be split further).

## Why this needed new instrumentation

A prior census's `confirm_one_traced`/`parse_word_selected_traced` path traced only the synthesis
half of the pipeline; `pg_rules::stratum::StratumAnalyzer`, which runs the analysis (unapply)
cascade every restricted reparse starts from, was untraced. Since validity-gate (post-synthesis)
rejections account for only a small share of failing time on every grammar, the dominant dead-end
mass necessarily dies during analysis or synthesis apply-time checks that run before a complete
candidate word ever exists — exactly the region that was a black box. `pg-rules/src/stratum.rs`,
`pg-rules/src/morph.rs`, and `pg-rules/src/trace.rs` wire that existing machinery up as
`is_tracing()`-gated code, so production behavior/cost is unchanged
(`NoopSink::is_tracing()` is `false` on every ordinary `parse_word*`/`confirm_batch`/`confirm_all`
call path).

## Frontier definition (the thing that decides the numbers)

For each failing candidate, one `TreeTraceSink` captures both the analysis and synthesis cascades
in a single tree. The census walks it once from the root, computing for every node a `depth` = the
number of ancestor nodes that represent a successful rule (un)application step (distinguished from
a failed attempt of the same `TraceType` by `node.output.is_some()`; see `is_success_step`). Every
node representing a dead attempt (a "not applied"/"not unapplied" event, or a terminal `Failed`
node; see `is_frontier`) is a frontier candidate; among all of them in the tree, the one with the
greatest depth is "the attempt that got furthest" — ties broken by a fixed pipeline-stage ordinal
(analysis < synthesis < final validity/match gate), preferring the later stage (`stage_ordinal`).

This is a coarse but fully-defined proxy for how many of the pinned rules' (un)applications a
branch got through before dying. It does not reconstruct the exact pinned-rule-sequence position;
bookend/stratum nodes interposed between rule-application nodes are not counted either way, so
branches that pass through more or fewer bookends at equal rule-progress are still ranked correctly
relative to each other, which is all the "deepest frontier" comparison needs.

**Two known coarsenings**, each reported with its own visible size rather than asserted away:

1. The phonological-rule unapply loop (`StratumAnalyzer::analyze`'s prule loop) does not advance
   the trace cursor between successive prules within one stratum — every prule attempt in that loop
   is a sibling under one parent. A dead end after N successful prule unapplications and a dead end
   after 1 are not distinguished by depth; only "died during the prule loop at all" vs "died later,
   having escaped the prule loop" is visible. This under-counts depth for multi-prule chains
   specifically, and would only distort d4-internal ranking (which specific prule chain won the tie),
   never which bucket (d1-d6) a candidate lands in.
2. `pg-parse::Morpher::lexical_lookup_filtered` (the boundary between the analysis cascade's output
   and the synthesis cascade's input) has no trace event of its own. A candidate whose analysis
   cascade fully and successfully unapplies every pinned rule (no frontier node anywhere below it)
   but whose resulting shape still does not match any stored allomorph of the pinned root produces a
   trace tree with no frontier node at all — reported as `Outcome::LexLookupBoundary`, folded into
   d4 (a lexicon-shape miss is the same kind of failure as a rule-shape miss) but counted and printed
   on its own line.

## Time attribution

Counts alone are not the deliverable — time share is, measured as a counterfactual under the real
batched `pg_foma::confirm::confirm_batch` (naive per-candidate unbatched sums are untrustworthy).
Per word:
- `baseline` = `confirm_batch(all candidates)`
- `keep_confirming` = `confirm_batch(only candidates that end up confirming)`
- `minus_dN` = `confirm_batch(all candidates except the ones classified dN)`, N in 1..=6
- class dN's share of failing time = `(baseline - minus_dN) / (baseline - keep_confirming)`

Classification is a separate, untimed pass using `classify_failing_candidate`; the six timed
`confirm_batch` calls per word (plus baseline/keep_confirming) never touch tracing.
