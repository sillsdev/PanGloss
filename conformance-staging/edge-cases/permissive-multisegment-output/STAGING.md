# Optional proposing expansions

The C#-verified review regression for PanGloss `5cc418d4` rewrites root `a`
to `ai`; obligatory replacement includes it, while the old foma `(->)` proposing
construction omits it. Source: Rust FST construction, not an HC algorithm defect.
The complete proposer must retain both `a` and `ai`; HC rejects unchanged `a`.

The review fixture is preserved with additional two-site (`aa`) and mixed-site
(`aia`) roots and negative controls. Expected surfaces are explicit forward
substitutions, independently replayed with the existing Release hc-conformance
at Machine `4d9b454d42a0da35e058641a65596339a79d115b`.
The direct regression `expansion_includes_founding_oracle_output` fails on the
old construction. Operand-shape tests separately cover both directions/modes,
expansion before/after, contractions, deletion and adjacent insertion choices.

Evidence: `fst-production-20261103/code-review/5cc418d4.md`, its probe directory,
and `logs/fst-stage1-expansion-{red,sweep-red,oracle}.log` in that production run.
No Machine issue, divergence or allowlist entry is needed for this FST-only bug.
The separate HC-Rust ambiguous-material observation belongs to another lane.

Upstream PR: (not opened; no push/PR in this lane)
