# Circumfix recall-parity gate (`tests/phase_c_circumfix.rs`)

The first full end-to-end validation of generator + oracle + gate together, and the first fixture
class that requires the production ENUMERATION path rather than `pg-foma/src/uflexc.rs`.

## Why the production enumeration path, not `uflexc.rs`

`pg-foma/src/emit.rs`'s `classify_affix` reads a circumfix rule's shape (leading AND trailing
insert around one copied span) as `Role::CircumfixPrefix`, and `is_structural_rule` always routes
it through the structural-composite builder — a `pg_parse::Morpher`-driven synthesis of every
composite entry, never literal-lexc concatenation. `uflexc.rs` (the simpler, token-space emitter
GATE 1 uses) explicitly skips `Role::CircumfixPrefix`. So this gate builds its net via
`pg_foma::emit::emit`, the only path that actually covers this construct.

## Recall technique

Same compose-recall helper as GATE 1, but querying with literal surface text (`emit.rs`'s lower
tape is literal orthography, unlike `uflexc.rs`'s token-space lower tape) and, per-word, the exact
tag sequence recovered by re-parsing the oracle's own generated surface word through an
independent `pg_parse::Morpher`. 100% recall is required here, unlike GATE 1: circumfix has no
known compiler gap on the enumeration path.

## The three hand-authored fixtures below the generator-driven gate

- **Ordered multi-insert** (`ORDERED_MULTI_INSERT_XML`): a single-part-LHS prefix rule whose RHS
  is TWO ordered `InsertSegments` actions before `CopyFromInput`. No LHS material is dropped, so
  this never routes through `build_structural_composites` — it exercises
  `crate::emit::insert_action_texts`'s ordinary (non-structural) emission path directly. Before
  the fix this pins, `emit::emit` emitted only the first `InsertSegments` text, silently dropping
  the second and losing recall for the real surface entirely.
- **Null-role structural drop** (`NULL_ROLE_STRUCTURAL_DROP_XML`, in-scope): a 2-part-LHS rule
  whose RHS `CopyFromInput`s only the first part — a null-role subtractive drop.
  `is_structural_rule` admits it (`rhs_drops_lhs_material` is true), so the truncated surface is
  only reachable via `build_structural_composites`'s oracle-backed resynthesis; the ordinary
  two-entry emission path can only ever propose the unmodified root, which must ALSO stay
  reachable as harmless over-generation, confirming the structural path adds rather than replaces.
- **Process-role drop** (`PROCESS_ROLE_DROP_XML`, out-of-scope): the same 2-part-LHS drop shape,
  but the RHS uses `ModifyFromInput` instead of ever `CopyFromInput`ing either part.
  `classify_affix` reads this as `Role::Process`, which `is_structural_rule` never admits — this
  construct must stay honestly unsupported (reported in `EmitReport::uncovered`), never silently
  compiled to any candidate.
