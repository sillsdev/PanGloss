## Decisions

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; this change follows the merged multi-table change.

- Direction is modeled by reversing the relevant automaton relation and boundaries, not by treating RTL as LTR.
- Plain, feature, deletion, and epenthesis witnesses are required before corpus claims.
- Unsupported combinations remain typed honest unsupported rather than falling back to LTR.

## Dependencies

Depends on `define-grammar-coverage-contract`, `reconcile-aweti-baseline`, and merged `fix-multitable-fst-compilation`. It is the next exclusive owner of `replace.rs`, gated/ungated `gate.rs` entry points, and RTL/Aweti evidence. It may add a post-baseline manifest but does not alter history.
