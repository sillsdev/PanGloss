# `make-report` design notes

`pangloss make-report <grammar> <out.md> [options]` composes evidence already defined by
`pg_foma::readiness_policy`/`readiness_verdict`/`plan_diagram` into one markdown file: build time,
artifact size, latency percentiles, the compilation-plan mermaid diagram, and the conformance
verdict. It reimplements none of those; it measures what feeds them and states plainly what it did
not test.

> **Current product policy (2026-08-23).** `make-report` may expose unsafe overrides only in a
> developer/test build; production must hide and reject `--allow-unproven` and
> `--remove-size-limits`. `--allow-unproven` may lose valid parses and may write local developer
> evidence, but never production-publishes or certifies. `--remove-size-limits` removes internal
> deterministic size/work caps only; exact
> completion, external watchdog/RSS containment, bounded I/O, and the absolute ceiling remain
> mandatory. A complete/accurate stress result may retain `Error` evidence, but `Error` is
> production-unready and `Critical` is a correctness gap. The legacy `--no-enforce-capability`
> escape is developer-only.

## What this module measures itself, versus what it only composes

The readiness types define the *shape* of a certification, but nothing populated a real
`Measurements` from a live grammar until this module:

- **Lexicon scale** is `grammar.entries.len()`, a direct count from the in-memory `Grammar`, not
  a caller-supplied value.
- **Latency percentiles** are measured in-process via nanosecond `Instant`/`Duration` timing over a
  real, freshly-built `FomaAnalyzer`, mirroring `pg-foma/tests/typology_speedup.rs`'s methodology:
  median-of-repeats per word, one discarded warmup call, a per-run-calibrated timer floor, and
  below-floor values reported honestly rather than as `pangloss batch`'s integer-millisecond `0`.
  See "Latency methodology" below for the percentile computation.
- **Coverage** is an attestation, never a measurement (`readiness_verdict`'s own rule), when
  `--corpus`/`--attestor`/`--attested-on` are all given; honestly not-assessed otherwise.
- **Build time** is the wall-clock cost of `FomaAnalyzer::new`, informational only — no threshold
  gates it — rendered with the same below-floor discipline as latency.
- **The plan diagram and conformance verdict** are pure composition:
  `plan_diagram::{build_plan_document, render_mermaid}` and `readiness_verdict::certify`, exactly as
  `plan-diagram` and the readiness-verdict tests already exercise them. No new logic here.

## Latency methodology

For each word in the word list (`--words=<path>`, one per line; falling back to the grammar's own
lexical root surface forms when omitted), this module times `FomaAnalyzer::analyze_word`
`--repeats` times (default 7) after one discarded warmup call, keeps that word's median nanosecond
duration, then computes p50/p90/p99 (nearest-rank method) over the sorted per-word medians — the
same "median-of-repeats per word, percentile across words" shape as `typology_speedup.rs`, driven
over one caller-chosen grammar/word-list instead of the whole conformance corpus.
`measure_timer_floor_ns` calibrates this process's real `Instant` granularity once per run (never a
hardcoded platform constant); any percentile at or below that floor renders as `BelowFloor`, never
a literal `0`.

## Coverage's token definition

A corpus's tokens are its whitespace-separated words. A token flagged unsegmentable by
`crate::foma_invalid_shape` (the same check `run_batch`/`run_parse` use) counts as a miss, not an
exclusion — the analysis-rate denominator is every token in the corpus, never a pre-filtered subset.

## What is always stated as not tested

Correctness is never certified by this report (coverage is an analysis rate, not accuracy — the
conformance suite is the correctness authority); a fallback word list is named as such when
`--words` is omitted; coverage is named not-assessed when no corpus attestation is supplied; and in
the refuse-without-override case, build time/artifact size/lexicon scale/latency/coverage are all
named not-measured/not-assessed together, since no compiled artifact exists to measure any of them.
