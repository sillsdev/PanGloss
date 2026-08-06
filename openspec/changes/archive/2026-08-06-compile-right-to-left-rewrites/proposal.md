## Why

RightToLeft rewrite rules are honestly skipped. This is one of the two largest known Aweti recall gaps and must be implemented with real directional semantics rather than restoring the old lucky mis-map.

## What Changes

- Compile RightToLeft rules using an explicit reversal construction with correct boundaries and environments.
- Add oracle-backed witnesses across rewrite shapes and ordering interactions.
- Re-run the Aweti manifest and record only analysis-level gains.

## Impact

This changes semantic support in the rule compiler. It is separate from Simultaneous mode because both require different algorithms and share high-conflict files.
