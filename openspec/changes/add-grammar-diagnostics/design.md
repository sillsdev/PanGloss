## Context

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; this change is not dispatchable outside that graph.

The active foma deployment path is `FomaAnalyzer → FomaProposer → emit_with_budget →
fsm_lexc_parse_string → apply_up → confirm_batch`. The experimental P6 replacement cascade is not
on this path. Diagnostics must observe the production pipeline it names and must not run an
uncapped Morpher for Aweti or any other adversarial grammar.

## Goals / Non-Goals

**Goals:** one supervised command; separate stable build/assessment schemas; dual-engine timing; Rust gloss output;
optional production-path candidate/confirm diagnostics; PowerShell orchestration; CI and skill.

**Non-goals:** compile-profile/state-explosion curves, C# parity, coverage-contract ownership,
resource-supervisor ownership, a four-language matrix, or language certification.

## Decisions

**D1 — Structured Rust core, PowerShell orchestration.** `pangloss diagnose` compiles one grammar in
memory, optionally writes a Language Pack, assesses a caller word list, and emits separate immutable
`build.json` and `assessment.json` plus optional TSV/JSONL. Build comparison consumes build reports;
semantic comparison consumes assessment reports only. `scripts/diagnose.ps1` handles `<lang>`,
`-All`, and `-Project`, then renders Markdown.

**D2 — Consume shared contracts.** Evidence level, analysis identity, denominator, and
Complete/Truncated are imported from `define-grammar-coverage-contract`. Resource policy/outcomes
and the external watchdog come from `harden-foma-resource-safety`. Diagnostics serializes these
values but does not define substitute caps or certification rules.

**D3 — Measure active parse stages; consume compile events.** Diagnostics owns optional events for
grammar load, FST traversal, decode/dedup, confirm-group construction, restricted HC parsing,
result routing, and total confirm. `profile-fst-compilation` exclusively owns emitter/build/compile
events and `emit.rs`; diagnostics only serializes those events when available. Full-HC oracle time
is separately labeled. Sink-off behavior must remain structurally and result equivalent.

**D4 — Rust gloss signature is stable and multiset-capable.** Every batch result site pairs each
structured analysis with `pg_realize::gloss_bundle`, a canonical rendered gloss chain, and the
analysis surface shape. Duplicate entries remain distinct. This becomes the Rust input consumed by
`add-reference-hermitcrab-parity`.

**D5 — Diagnostic evidence is observational.** Reports use `ObservedOnly`,
`EstablishedByNamedGate`, `NotEvaluated`, or `Failed`. They never infer construct support, corpus
recall, or supported-language status from successful words, glosses, or timings.

**D6 — The matrix consumes diagnostics.** This change only produces the instrument and schema used
later by `certify-four-language-matrix`; it does not run or publish a second four-language matrix.

## Dependencies and Ownership

Schema/CLI scaffolding depends on the coverage schema and resource outcome API. Any command that
actually executes potentially adversarial grammar compile/parse work uses the completed single-worker
watchdog—not merely its interface. This change owns `pg-cli`
diagnostic/report/gloss plumbing and generic parse-stage events. `profile-fst-compilation`
exclusively owns compile events and `emit.rs`; `add-reference-hermitcrab-parity` owns C# execution
and comparison.

## Risks

- Instrumentation overhead is reported and sink-off behavior is gated.
- A timed-out or truncated run remains useful diagnostically but cannot support exact correctness.
- `incoming/` uses explicit gitignore negations for its committed README and smoke fixture.
