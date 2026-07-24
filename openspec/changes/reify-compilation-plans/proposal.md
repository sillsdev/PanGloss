## Why

There is no single best compilation strategy across all grammars, and PanGloss already selects among
strategies (composed paths vs. enumeration) per grammar informally. Because nothing ships until the
multi-topology system exists, the compile step should be refactored to a plan-reified model *as the
model*, not as an optimizer bolted onto a hardcoded pipeline. Today topology is hardcoded branching
(`should_run` / `probe_would_refuse` / `partition_entries`) with nothing to enumerate. See
`docs/adr/0002`.

## What Changes

- Refactor the compile step so a **compilation plan** is first-class, enumerable data: a strategy
  enumerator emits the legal composition topologies for a grammar.
- Selection is **capability-safe by construction**: every capability-passing plan is recall-preserving,
  so all produce the identical confirmed set; selection can never pick a fast-but-wrong plan.
- Add the **differential-correctness oracle**: build ≥2 plans and assert identical confirmed sets;
  disagreement is a predicate bug caught automatically.
- Name each compilation module by **the parts it composes** (or another disambiguating scheme), never
  by a language (hard rule).
- **Out of scope / parked** to a follow-on (`add-compilation-cost-planner`): projected-cost model with
  error bounds, committed-plan config + derived cache, profile-guided autotuning.

## Impact

This is the "massive refactor" the rest of Stage 2 builds on, so it must land before per-construct
capability work to avoid authoring constructs twice. It touches the core compilation topology
(`replace.rs` / `gate.rs` / `emit.rs` / composition constructor) and requires single-owner
serialization. It grants no new construct capability by itself.
