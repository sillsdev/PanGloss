# pg-foma segment_natural_class_table_binding_discriminates.rs: design notes moved out of comments

Longer arguments pulled out of
`rust/crates/pg-foma/tests/segment_natural_class_table_binding_discriminates.rs` implementation
comments so the source can carry a one- or two-line pointer instead of the full argument.

## Module doc: why this file exists

This is a discriminating-power proof for the
`conformance-staging/edge-cases/segment-natural-class-table-binding` fixture. That fixture closes a
conformance-suite blind spot: every other multi-table fixture
(`two-table-shared-representation-recall`, `multi-table-metathesis-shared-representation`,
`bistratal-overlapping-segment-representation`) builds its rules' natural classes from
`FeatureNaturalClass` only. `pg_rules::bridge::PatternBridge::nat_class_lanes`'s
`NaturalClassKind::Feature` branch never reads `self.table` at all (a `SymbolicFeature`'s bit
assignment is grammar-wide, not per-table) — so none of those fixtures could ever detect a rule
wrongly resolving its natural classes against the wrong `CharacterDefinitionTable`. Only
`NaturalClassKind::Segments` (`SegmentNaturalClass`) is genuinely table-dependent: its members are
raw per-table `CharDefId`s with no table of their own, resolved via `self.table.get(cd)` (see
`rust/crates/pg-rules/src/cache.rs`'s `owning_table_tests` module, whose two-table/two-stratum probe
grammar this fixture's own `grammar.xml` mirrors).

A fixture that merely passes proves nothing about this specific blind spot — a fixture that would
also pass under a wrong-table resolution is worthless for exactly the failure class this file exists
to catch. This file proves the fixture's own natural classes are genuinely table-dependent, by
constructing the "resolved against the wrong table" comparison directly, via
`pg_rules::bridge::PatternBridge`'s own public `with_table`/`compile_pattern` API — the exact seam
`nat_class_lanes`'s `Segments` branch lives behind — rather than by editing `pg-rules/src` (where
`RuleCache`/`synthesize_with_mpr_cached`, the real per-word cached call path, are `pub(crate)`-only
and unreachable from this crate's tests without such an edit).

`PatternBridge::new` itself defaults to `TableId(0)` (see its own doc) — literally the antipattern
default this whole bug class is about. So `PatternBridge::new(&g)` (no `.with_table(..)` call) is
not a contrived stand-in for the bug; it is the bug's own resolution, reused directly as the "wrong"
comparison arm.

## `fixture_shape_is_the_deliberately_misaligned_two_table_probe_it_claims_to_be`: structural sanity

Exactly 2 tables, 2 strata, the rule's own stratum (S1, "Outer", index 1, non-first) owns table 1 —
and `ncK`'s one member is a `SegmentNaturalClass` referencing table 1's raw index 0 ("k"), the same
raw index table 0's own sole segment ("z") sits at, but with the opposite feature value: the
deliberate misalignment this whole proof depends on.

## `nat_class_k_resolved_against_the_wrong_table_stops_matching_a_real_table_1_k_segment`: the deliverable

`ncK`'s compiled constraint is genuinely table-dependent, and resolving it against the wrong table
(table 0, `PatternBridge::new`'s own default) breaks the exact match the fixture's own ground truth
(`words.yaml`'s `g` -> `"ROOT2|g"`) depends on. `CompileNode::Constraint`'s own doc says the match is
`pg_featstruct::flat_unifiable` — not a hand-rolled substitute, the literal per-arc match rule every
compiled FST this bridge produces uses.

Two resolutions are compiled and compared: the correct one (`.with_table(TableId(1))`, matching what
`RuleCache::build`'s `owning_table_for_prule` resolves to in production) and the wrong one (no
`.with_table(..)` call, table 0's default). The wrong resolution's constraint equals table 0's own
"z" lanes — a different, incompatible feature value — so a real table-1 "k" segment no longer
matches it. That is the exact, concrete way the eleven-site table-zero-default defect class would
have made this fixture's own ground truth unreachable, had it existed in the confirm engine's
`SegmentNaturalClass` resolution path.
