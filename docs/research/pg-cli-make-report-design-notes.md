# `make-report` design notes

`pangloss make-report <grammar> <out.md> [options]` composes evidence already defined by
`pg_foma::readiness_policy`/`readiness_verdict`/`plan_diagram` into one markdown file: build time,
artifact size, latency percentiles, the compilation-plan mermaid diagram, and the conformance
verdict. It reimplements none of those; it measures what feeds them and states plainly what it did
not test.

## What this module measures itself, versus what it only composes

The readiness types define the *shape* of a certification, but nothing populated a real
`Measurements` from a live grammar/pack until this module:

- **Pack size + trust status** come from a real `.pgpack`, never a caller-supplied trust
  parameter — see "Trust provenance" below.
- **Lexicon scale** is `grammar.entries.len()`, a direct count from the in-memory `Grammar`, not
  derived from the pack (whose runtime payload is still a placeholder).
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

## Trust provenance: a real artifact, never a caller-settable parameter

`certify`'s `trust: &TrustStatus` is never populated from a bare CLI flag. Either `--pack=<path>`
names an existing `.pgpack` (read via `pg_pack::read_pack`, and `manifest.capability_trust` is the
trust certified against), or, with no `--pack`, this module builds one itself via
`crate::pack::build_pack` — the same capability-trust-stamping logic `pangloss pack` uses — and
reads the trust back off the manifest that call produces. Either way the trust certified against is
the real stamp on a real artifact.

`map_trust` is a plain, non-lossy field-for-field projection of `pg_pack::CapabilityTrust` into
`pg_foma::readiness_verdict::TrustStatus`. The two shapes are kept in a hand-maintained
correspondence rather than a shared type because `pg-pack` already depends on `pg-foma` (for
`HealthReport`), so the reverse dependency would cycle.

## Capability enforcement mirrors the rest of the CLI

Exactly like `run_batch`/`run_parse`/`pangloss pack`: a capability `Refuse` verdict with no
`--allow-unproven` means no compiled artifact is built or measured at all — every check reports
`NotAssessed` and the tier is `NotSupported`, citing the real refusal. `--allow-unproven` on
`make-report` requires the same flag be passed to `pangloss pack` too; a caller cannot point
`--pack` at a pre-built overridden artifact and have this command quietly measure against it
without acknowledging the override at the report layer as well.

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
