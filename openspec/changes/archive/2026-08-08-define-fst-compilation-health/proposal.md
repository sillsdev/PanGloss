> **Archived 2026-08-08 — the schema shipped and is live; the six open tasks moved to
> `recipe-scoped-fst-health`.** Unlike the other changes archived in this sweep, this one is not
> being abandoned: `pg-foma/src/health.rs` is in production use, its severity axis already matches
> the settled two-axis rule (cost graded, correctness binary), and a finding already carries the
> free-form explanation and ranked remedies that health was asked for. What it lacks is a way to say
> WHICH BACKEND a finding was measured under, which only matters once more than one compiler can
> run — so that work belongs to the successor, not here. Its size bands were raised 10x on the same
> day and are documented in code as a stated target rather than a measurement.

## Why

PanGloss needs compiler warnings that explain when a grammar's FST construction is large, slow, or
explosive without duplicating general grammar-quality review or measurements in another language.

## What Changes

- Define Rust types and stable codes for FST compilation-health findings.
- Define Ideal, Info, Warning, Error, and Critical semantics and override behavior.
- Pin FST-payload size bands and multi-dimensional severity aggregation.
- Require affected constructs, evidence, thresholds, and applicable remedies in every finding.
- Define canonical machine-readable output consumed by CLI, FieldWorks, AI tooling, and packages.

## Impact

This is a policy/schema change. It does not instrument the compiler, change grammar semantics, add
Python analytics, or judge whether a linguistic analysis is good.
