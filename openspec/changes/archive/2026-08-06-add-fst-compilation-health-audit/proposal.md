## Why

The health contract needs one Rust implementation that consumes existing compiler measurements and
produces actionable preflight and observed warnings without remeasuring them.

## What Changes

- Add a complete preflight walk over the frozen grammar model.
- Consume budget and compile-profile events for observed findings.
- Emit canonical `health.json`, derived `health.md`, and normal compiler warning lines.
- Provide `pangloss fst-health` and embed the admission summary in deployable packages.
- Require each semantic compiler change to register cost and remediation findings.

## Impact

This adds compiler diagnostics, not a UI, IDE, playground, Python analytics layer, or general
grammar-review system.
