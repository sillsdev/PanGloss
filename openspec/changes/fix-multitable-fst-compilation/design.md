## Decisions

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; this is the first serialized rewrite-correctness change.

- Every compiled rule carries its owning character-table identity explicitly; table zero is never an implicit default.
- Alpha-variable resolution uses the same table as pattern rendering.
- Existing detect-wrong coverage is inverted only after end-to-end proposer-to-confirm analysis containment passes.

## Dependencies

Depends on `define-grammar-coverage-contract` and the pinned Aweti manifest where corpus evidence is used. It is the first exclusive semantic owner of `replace.rs`, gated and ungated `gate.rs` entry points, rendering/table helpers, and multi-table/strata gates.
