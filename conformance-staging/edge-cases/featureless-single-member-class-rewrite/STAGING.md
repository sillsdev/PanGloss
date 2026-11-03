# featureless-single-member-class-rewrite

**Classification: BUG PIN (PanGloss issue #120, the minimal case: classes of one member)**

## What it tests

`[k] -> [s] / [a i] _ [a i]` in a grammar with no phonological feature system, written with segment
natural classes. In C# a one-member class has the same StrRep as the literal, so the rule behaves
like `k -> s`. `asa` parses (`AKA`), `aka` does not, and `ita` is untouched because `t` is outside
the left-hand class. Companion of `featureless-segment-class-rewrite`, which uses multi-member classes.

## Results

C# founding oracle, sillsdev/machine aab4c598 (hc-conformance self-check), 2026-11-03: 1 passed.
HC-Rust at main f4ed795c, before the fix: `asa` 0 parses, `aka` 1 parse (the issue's table).

## Provenance

Oracle of record: C# founding oracle, `hc-conformance --fixtures <root>` self-check built from
sillsdev/machine aab4c598, one run, 2026-11-03. Synthetic data only. FieldWorks-producible as
described in `featureless-segment-class-rewrite/STAGING.md`.

Promotion status: upstream_candidate.
Upstream fixture PR: not opened; candidate destination is machine/conformance/edge-cases/featureless-single-member-class-rewrite/.
