## Why

The Aweti plan assumes 68/104 recall, but the current honest compiler baseline is 32/104 after unsupported RightToLeft and Simultaneous rules stopped being silently mistranslated. Timing and optimization against the old network would be invalid.

## What Changes

- Pin the grammar, commit, supported-rule set, denominator, and exact recalled-analysis manifest.
- Introduce one shared compiled Aweti network constructor used by correctness and diagnostics.
- Diagnose the bare-root gap without restoring incorrect rule compilation.
- Instrument the real path, measure it, and publish a decision report selecting at most one candidate optimization for a new bounded OpenSpec change.

## Impact

This establishes trustworthy Aweti evidence and a shared measurement seam. It does not implement
RightToLeft or Simultaneous semantics, and it does not pre-authorize an optimization whose hotspot
and files are unknown before measurement.
