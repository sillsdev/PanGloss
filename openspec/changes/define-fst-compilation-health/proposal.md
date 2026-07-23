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
