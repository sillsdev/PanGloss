## Why

PanGloss must never overclaim: a grammar either compiles into a recall-preserving proposer or fails
loudly at compile time. Today there is no such gate — `Compounding`, `MorphRuleOrder::Unordered`, and
`MprGroup` are implemented in the confirm engine but never proposed by the FST, so a grammar using
them compiles and silently loses recall. `define-grammar-coverage-contract` produces an after-the-fact
ledger, not a load-bearing compile-time refusal. See `docs/adr/0001` and `docs/adr/0005`.

## What Changes

- Introduce the **characteristics check** as a first-class compile gate: project a grammar + stem
  data into a **characteristics profile**, compose a **capability envelope** from per-stage and
  interaction predicates, match them, and **hard-fail** any not-proven-faithful configuration with a
  typed diagnostic.
- Granularity is **configuration-predicate**, not variant ("supported *unless* X"); each predicate is
  an oracle-verified proof obligation that may over-refuse but never under-refuse.
- **Default-deny characterizer** exhaustive over the frozen `model.rs` with no catch-all: adding a
  variant breaks the build until it is characterized.
- Add the **developer-only capability override + unproven-trust runtime signal** (ADR 0005): a
  refused grammar may be force-compiled for grounding behind an indelible degraded-trust signal.
  It may omit valid parses, is hidden/rejected in production, and never publishes, certifies, or
  passes conformance. This correctness override is distinct from developer stress execution with
  internal size/work caps removed; neither mode permits partial output to count as accurate.
- Add the **conformance-coverage CI gate**: a construct/config may be marked supported only when a
  passing synthetic `machine/conformance/` fixture exercises it, else CI breaks.
- First act: mark `Compounding`, `MorphRuleOrder::Unordered`, `MprGroup`, and all unproven configs
  **fail-closed**.

## Impact

This is the keystone; every semantic, interaction, scale, and conformance change sequences behind it.
It establishes the capability manifest and the mechanical meaning of "supported". It carries a
**capability evidence provenance** field (behavioral vs. structural). It does not itself compile any
new construct — it defines the contract those constructs are promoted through.
