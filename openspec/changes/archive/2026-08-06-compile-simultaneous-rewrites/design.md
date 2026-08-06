## Decisions

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; this change follows the merged RightToLeft change.

- Match discovery is based on the unchanged input tape; replacements are applied as one simultaneous relation.
- Overlap and subrule priority follow the full-HC oracle, including cases where an iterative implementation differs.
- Any uncompiled combination stays honest unsupported.

## Dependencies

Depends on `define-grammar-coverage-contract`, `reconcile-deep-truncation-baseline`, and merged `compile-right-to-left-rewrites`. It is the next exclusive owner of `replace.rs`, gated/ungated `gate.rs` entry points, and Simultaneous/deep-truncation-chain evidence.

This change is authored on the reified compilation model (`reify-compilation-plans`) rather than the
old hardcoded `should_run`/`probe_would_refuse`/`partition_entries` branching. Its subrule-overlap
predicate — already surfaced above as an explicit requirement, per ADR 0001's own worked example — is
registered with `add-capability-characteristics-check` as a configuration-predicate capability
boundary, confirm-only-by-default per ADR 0001 unless a proven no-false-negative admission-filter
argument exists.
