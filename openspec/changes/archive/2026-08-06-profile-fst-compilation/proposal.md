## Why

Compilation diagnostics must describe the network actually used in production. Today production
uses the surface-prebaked `emit_with_budget` path; the P6 replacement cascade is experimental. A
per-rule cascade curve is truthful only after Stage 2 wires that cascade into production.

## What Changes

- Stage 1 profile: production emitter/probe/lexc time, per-template line counts, final lexc network
  states/arcs, total compile time, and resource outcomes.
- Stage 2+ profile: once the replacement cascade is the production path, add per-rule own-net
  metrics, alpha-tuple/group counts, and the running composition state/arc curve.
- Keep experimental and production profiles explicitly distinct until the production switch.

## Impact

This change owns compile-profile events and `emit.rs` instrumentation. It consumes the diagnostic
report schema but does not own general parsing diagnostics or semantic compiler implementation.
