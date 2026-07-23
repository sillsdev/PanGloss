## Decisions

- Depend on merged bounded-quantifier work so `replace.rs` has one owner at a time.
- Use the frozen `MetathesisRule` switch identities and HermitCrab behavior as the oracle.
- Build a dedicated swap relation; do not encode metathesis as iterative ordinary replacement.
- Boundary movement, direction, environments, feature classes, and multiple tables receive separate
  witnesses and budget accounting.

## Ownership and verification

Exclusive files: metathesis regions of `pg-foma/src/replace.rs`, `gate.rs`, shared `lower.rs`, and
`pg-foma/tests/phase_c_metathesis.rs`. `pg-rules/src/metathesis.rs` is oracle/read-only except for a
demonstrated bug fix in a separately recorded task.

Run from `rust/`:

- `cargo test -p pg-foma --test phase_c_metathesis`
- `cargo test -p pg-rules --test rewrite_gate`
- `cargo test -p pg-foma --test phase_c_multi_table`
- `cargo test -p pg-foma --test phase_c_quantifier`
