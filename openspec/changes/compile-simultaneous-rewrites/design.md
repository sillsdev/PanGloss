## Decisions

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; this change follows the merged RightToLeft change.

- Match discovery is based on the unchanged input tape; replacements are applied as one simultaneous relation.
- Overlap and subrule priority follow the full-HC oracle, including cases where an iterative implementation differs.
- Any uncompiled combination stays honest unsupported.

## Dependencies

Depends on `define-grammar-coverage-contract`, `reconcile-aweti-baseline`, and merged `compile-right-to-left-rewrites`. It is the next exclusive owner of `replace.rs`, gated/ungated `gate.rs` entry points, and Simultaneous/Aweti evidence.
