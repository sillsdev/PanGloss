## Why

Metathesis is represented and executed by HermitCrab but its FST proposal coverage is separately
incomplete. Treating it as an ordinary replacement would change switch and environment semantics.

## What Changes

- Compile metathesis switch regions through a dedicated relation using the shared lowering IR.
- Preserve direction, environments, boundaries, feature classes, and table ownership.
- Add exact oracle and resource witnesses.

## Impact

This adds proposer coverage for proven metathesis variants without changing the Rust HermitCrab
engine or claiming support for unproven combinations.
