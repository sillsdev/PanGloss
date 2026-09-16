# 042 — Metathesis relocation left a stale origin-table char-def identity at the surface-match gate

## Kind
Behavioural (Rust-multi-table-only: HermitCrab has exactly one `CharacterDefinitionTable` per
grammar, so the cross-table collision this entry describes cannot arise in C# at all).

## Status
Fixed-in-rust.

## C# site
None — not reachable in C#. `MetathesisRule`/`SynthesisMetathesisRule`
(`PhonologicalRules/MetathesisRule.cs`, `SynthesisMetathesisRule.cs`) swap two spans' segments within
the single table every HermitCrab grammar has; there is no second table whose raw indices could
collide with the first.

## Rust site
`pg_rules::metathesis::synthesis_reorder` (`rust/crates/pg-rules/src/metathesis.rs`), consumed by
`pg_parse::Morpher::is_match_traced` and `pg_parse::surface::matching_reps_for_node`
(`rust/crates/pg-parse/src/{morpher,surface}.rs`).

## What differs
Every OTHER identity-changing synthesis path (`pg_rules::rewrite::syn_feature`/`sim_feature`) resets
a changed node's `char_def` to `NO_CHAR_DEF` when it modifies that node's identity, so a later
consumer knows to re-derive the node's representation from its current feature lanes rather than
trusting a stale literal. `synthesis_reorder` physically relocates a segment (metathesis's whole
point) without ever resetting its `char_def` — the relocated segment keeps carrying its ORIGIN
table's raw char-def index all the way to the final surface-match gate,
`pg_parse::Morpher::is_match_traced`, which renders concrete char-def identities via
`matching_reps_for_node` against the grammar's OUTERMOST stratum's table unconditionally. For a root
entered on an Inner stratum and metathesized on the Outer stratum's own rule, the relocated segment's
Inner-table raw index was compared against the Outer table's own raw indices — an apples-to-oranges
collision specific to metathesis, since it is the only rule kind that moves material without also
erasing its concrete identity.

## Can it change a parse?
Yes: this made a correctly-metathesized, cross-table root's own surface form unrecognizable to the
confirm engine — `pg_parse::Morpher` found zero analyses for it even after entry 041's table-zero-
default fix made metathesis resolve its OWN rule's owning table correctly, because the remaining
defect was in a downstream consumer (the surface-match gate), not in which table the metathesis rule
itself resolved.

## Evidence
`conformance-staging/edge-cases/multi-table-metathesis-shared-representation/`'s own STAGING.md
documents the isolation directly: three throwaway probes (deleted after transcription) first ruled
out the table-zero-default hardcode alone as sufficient (a probe with the raw indices for "m"/"x"
made to coincide across both tables found ROOT1 correctly, isolating the failure to raw-index
misalignment specifically), then a fourth, non-throwaway investigation (`9cfcd4c2`) pinpointed
`is_match_traced`'s outermost-stratum-table rendering as the exact remaining site. Fixed by
resetting a relocated segment's `char_def` to `NO_CHAR_DEF` at the point of relocation in
`synthesis_reorder`, mirroring `syn_feature`/`sim_feature`'s existing convention — chosen over
teaching `is_match_traced` to resolve "the node's own owning table" instead, because no per-node
table-provenance metadata exists anywhere in `pg_shape::Shape`/`MutNode`, and inventing it would be a
larger, riskier change without removing the actual staleness. `words.yaml`'s `xm` entry was flipped
from `expect_fail: true` to `parses: [{signature: "ROOT1|xm"}]`, re-derived by running the engine, not
hand-derived. Re-verified against the C# founding oracle: signature matches exactly
(`oracle-provenance: founding-oracle`).

## Upstream
None, not applicable — pure Rust-side defect specific to Rust's own multi-table representation; no
C# equivalent exists to report.

## Notes
Flagged, not chased: whether an ordinary (non-metathesis, non-feature-changing) morphological
copy-through (`pg_rules::morph::copy_part`'s plain `CopyFromInput` branch, which also never resets
`char_def`) could carry the same stale, origin-table-relative identity across a table boundary for a
root never otherwise touched by an identity-resetting rule is an open question this fixture's own
two-root design does not exercise. No reproducing fixture exists yet, so no fix was attempted
speculatively.
