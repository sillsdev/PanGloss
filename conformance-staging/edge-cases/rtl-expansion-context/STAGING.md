# RTL iterative expansion inclusion

Pins the FST-only under-proposal reviewed in PanGloss `0627ce48`. The proposing
relation must include the explicit forward expansion alongside unchanged material.
The reviewed processed-context/unchanged-material construction omitted it.

`words.yaml` preserves the reviewer's C#-observed identities and negative controls
at Machine `4d9b454d42a0da35e058641a65596339a79d115b`, using the existing Release
driver. Evidence: `fst-production-20261103/code-review/0627ce48.md` and
`code-review/probe-0627ce48/oracle-expansion.log` in that run directory.
Fresh lane replay: `logs/fst-stage1-rtl-context-oracle.log` in that run directory,
2 fixtures passed, 0 failed/skipped, covering all six word rows. Both XML/word
files remain byte-identical to the review fixtures. The direct relation regression
fails on the reviewed implementation and passes with whole-batch RTL reversal.
No HC-Rust change, Machine issue or divergence entry is needed for this FST bug.

Upstream PR: (not opened; no push/PR authorized in this lane)
