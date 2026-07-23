## Decisions

Execution order and ownership follow `openspec/changes/STAGING.md`.

- Create `pg-foma/src/lower.rs` as the sole pattern/environment-to-compiler-IR module.
- Treat `pg-grammar/src/model.rs` and the Rust HermitCrab port as frozen consumers; do not add model
  variants or alter HermitCrab semantics.
- Carry character-table identity, anchors, polarity, groups, alternation, and quantifier metadata
  explicitly. Unsupported nodes return typed disposition evidence rather than disappearing.
- Existing behavior must remain byte-identical with the new seam disabled/enabled for currently
  supported rules.

## Ownership and verification

Exclusive files: new `pg-foma/src/lower.rs` and its unit tests; narrowly scoped call-site adaptation
in `pg-foma/src/replace.rs` and `gate.rs`. Do not modify `emit.rs`.

Run from `rust/`:

- `cargo test -p pg-foma --test phase_c_multi_table`
- `cargo test -p pg-foma --test phase_c_right_to_left`
- `cargo test -p pg-foma --test phase_c_simultaneous`
- `cargo test -p pg-foma --lib lower`
