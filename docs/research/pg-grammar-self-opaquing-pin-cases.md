# pg-grammar load.rs: `self_opaquing_pin_semantics_match_node_pins`'s five cases

Moved out of an implementation comment in `rust/crates/pg-grammar/src/load.rs` so the source can
carry a one-line pointer instead of the full case list.

This test pins `RewriteSubruleDef::self_opaquing`'s per-kind formula directly, mirroring C#'s own
dispatch (`AnalysisRewriteRule.cs:26-104`) rather than just eyeballing agreement. Five rules, one
subrule each:

- `prA` (Feature, Simultaneous): RHS pin `Voiced` (voi+) IS feature-unifiable with its
  RightEnvironment `Voiced` (voi+ again, bits overlap) -> `self_opaquing` must be `false`.
- `prB` (Feature, Simultaneous): RHS pin `Voiced` (voi+) is NOT unifiable with its
  RightEnvironment `Voiceless` (voi-, disjoint bits on the same feature) -> `true`.
- `prC`: identical patterns to `prB` but `multipleApplicationOrder` omitted (Iterative) ->
  `false` (the mode gate short-circuits before the unifiability check ever runs).
- `prD` (Epenthesis: empty LHS, Simultaneous): `true` unconditionally, no unifiability precheck
  for this branch.
- `prE` (Narrow: 2-node LHS, 1-node RHS, Simultaneous): `false` — irrelevant field, this kind is
  always unconditionally Simultaneous+Deletion regardless of `rule.mode`.
