---
name: fst-limits
description: >-
  Use before changing any FST threshold, refusal, retry, budget, or containment mechanism in
  pg-foma, and when reading a compiler finding to decide what it actually blocks. Trigger on
  "raise/lower this limit", "why did the compiler refuse this grammar", "should this block
  publishing or building", "is this Readiness or Representability or Containment", "the compose
  budget fired", "MachineLimit vs NotProductionReady", or any change to FindingCode/Severity and
  how a caller routes on them.
---

# Classifying FST evidence before changing a limit

Before changing an FST threshold, refusal, retry, or containment mechanism, state whether the
evidence concerns **correctness/representability**, **production readiness**, or **resource
containment**, and name the verdict. `Severity` names the blocking TIER; `FindingCode::class()`
names WHICH question — read them together, never one for the other:

- `LargeMultiplier` — a magnitude is too large, predicted or measured (`ValueProvenance` says
  which). Class `Readiness`. Blocks nothing. Remedy: check grammar optimization.
- `CannotRepresent` — the feature cannot be faithfully proposed. Class `Representability`. Blocks
  building it; never overridable by production selection.
- `NotProductionReady` — the publication-blocking tier. Its commonest cause is an oversized
  compiled payload, but a budget stop, a build-process fault and a proven bound land here too; the
  finding's class says which. A label, not a gate on compiling.
- `MachineLimit` — an external monitor aborted this attempt. Class `Containment`. About this
  machine and this attempt, never about the grammar.

Three rules that are easiest to get wrong:
(a) `NotProductionReady` must never prevent an FST from being built — it blocks only publishing.
(b) `MachineLimit` is host protection, never a verdict about the grammar.
(c) A severity alone does not tell you the question. Two findings can share a tier and answer
    different questions, so route decisions on class and never on an ordinal comparison.

Do not use a `NotProductionReady` label to avoid a contained stress attempt, and never use a larger
limit to excuse incomplete or unproven output. Follow
`docs/superpowers/specs/2026-08-23-stress-grammar-construction-and-production-admission.md`.

