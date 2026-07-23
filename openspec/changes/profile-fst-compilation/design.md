## Context

`emit_with_budget → fsm_lexc_parse_string` is the active deployment compile. It pre-bakes phonology
into emitted surface forms, so replacement-rule nets and alpha tuple folds do not exist there. The
experimental `replace.rs` cascade can expose those metrics, but presenting them beside production
timing would misidentify the pipeline.

## Decisions

**D1 — Two gated profile phases.** Phase A profiles only production emitter/probe/lexc stages and
may land in Stage 1. Phase B is blocked until Stage 2 semantic changes wire the replacement cascade
into the production constructor.

**D2 — No observer-induced minimization.** Record exactly the states/arcs returned at existing fold
boundaries. Profiling SHALL NOT add minimize/determinize/clone operations that change bytes,
semantics, timing, or resource use.

**D3 — Top-line compile time is mandatory.** Every profile reports total grammar-to-ready-network
wall time, with child stage times treated as attribution rather than guaranteed additive partitions.

**D4 — One `emit.rs` owner.** Phase A owns the optional sink threading through `emit.rs`. Semantic
compiler work that also touches `emit.rs` is serialized through `STAGING.md`.

## Dependencies

Phase A depends on `add-grammar-diagnostics` report/event schema and the safety outcome schema.
Phase B additionally depends on Stage 2 production wiring of the replacement cascade. Merely having
experimental `replace.rs` functions is insufficient.
