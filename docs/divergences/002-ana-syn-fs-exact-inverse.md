# 002 — Analysis syntactic-FS fold: Exact inversion

Analysis should recover derivations that forward synthesis can perform, without accepting invalid
ones. Rust now uses Exact inversion; its alignment with current Machine remains open work.

## Kind
Behavioural.

## Status
Open. Implemented on PanGloss main in `149f88df`, present at audit base `0006b938`.
A posted Machine issue and PR comment exist; no merge-ready Exact PR or merged C# Exact fix is claimed.

## C# site
`AnalysisAffixProcessRule.Apply` and `AnalysisCompoundingRule.Apply`.
The research branch `perf/pr494-priority-union` has an `AnalysisSyntacticFeatureMerge` mode switch;
that research toggle is not the implementation proposed by current PR #494.

## Rust site
`rust/crates/pg-rules/src/morph.rs::ana_syn_fs` and `pg-featstruct/src/ops.rs::remove_paths`.
The gate uses PriorityUnion(required, output); inversion removes output paths before imposing required
features. The function's fallback on failed unification must be included in any full proof.

## Evidence
- Rust integration grammar: `rust/crates/pg-parse/tests/exact_analysis_fs_recall.rs`.
- Rust rule gates: `rust/crates/pg-rules/tests/analysis_syn_fs_gate.rs`.
- Machine conformance grammar already exists at `conformance/edge-cases/chained-output-feature-override-loss`,
  on `integrate-conformance-framework` at `8bad193454a14422a3be59c3156513691ae3da3b`.
  Its six words include the forward-justified `zudiua` parse and negative `zudia` control.
  This is deliberately red, not a missing fixture. A fresh 2026-09-15 run at `8bad1934`
  completed all six words: `zudiua` again returned no parse, while the four positive controls
  and negative `zudia` matched. This does not substitute for rerunning current master and #494.
- Historical measurements: `docs/research/2026-09-11-exact-analysis-fs-measurements.md`.
  A timeout becoming a completion is a resource outcome, not evidence that the old algorithm would
  return no parse. Fewer rule attempts alone proves neither soundness nor completeness.
- Template/slot widening still lacks a demonstrated fix-removed discriminator; see entry 035.

## Upstream
[Issue #504](https://github.com/sillsdev/machine/issues/504) tracks current-head reproduction and the fix.
The [Exact comment on #494](https://github.com/sillsdev/machine/pull/494#issuecomment-5666163708)
is posted, not an unposted draft and not an Exact PR. The
[maintainer's rerun request](https://github.com/sillsdev/machine/pull/494#issuecomment-5667230506)
follows merged #493. Reproduce against current master and current #494 before claiming the upstream
gap is unchanged. [Issue #505](https://github.com/sillsdev/machine/issues/505) tracks merge compatibility.

## Remaining work
Establish positive and negative controls under current C# and Rust revisions, including nested paths,
variables, disjunctions and merge interactions. Record actual fix-removed failures rather than treating
a test filename, finite parity sample or algebraic sketch as a universal correctness proof.
