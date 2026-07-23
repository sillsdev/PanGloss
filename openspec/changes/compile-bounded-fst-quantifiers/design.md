## Decisions

- Depend on merged `lower-fst-pattern-environments`.
- Compile only ledger-enumerated optional/bounded variants proven regular and within existing work
  budgets. No finite cutoff may masquerade as unbounded Kleene semantics.
- Preflight the product of alternatives/repetitions and report a typed budget or unsupported result.
- Promote ledger rows individually after positive, negative, and interaction witnesses pass.

## Ownership and verification

Exclusive files: `pg-foma/src/lower.rs`, quantifier regions of `replace.rs` and `gate.rs`, and
`pg-foma/tests/phase_c_quantifier.rs`. Do not modify `emit.rs`.

Run from `rust/`:

- `cargo test -p pg-foma --test phase_c_quantifier`
- `cargo test -p pg-foma --test phase_c_multi_table`
- `cargo test -p pg-foma --test phase_c_right_to_left`
- `cargo test -p pg-foma --test phase_c_simultaneous`
