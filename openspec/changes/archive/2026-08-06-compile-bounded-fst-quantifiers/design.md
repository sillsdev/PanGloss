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

## Dependencies

This change is authored on the reified compilation model (`reify-compilation-plans`) rather than the
old hardcoded `should_run`/`probe_would_refuse`/`partition_entries` branching: bounded-quantifier
expansion is a `Plan` node/strategy the enumerator selects among. Its capability boundary — which
optional/bounded-repetition configurations compile faithfully within budget — is a configuration-
predicate registered with `add-capability-characteristics-check`, confirm-only-by-default per ADR
0001 unless a proven no-false-negative admission-filter argument exists.
