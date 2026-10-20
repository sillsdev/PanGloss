# 091: Featured grammars lost segment identity in allomorph and rewrite environments

Kind: behavioural.
Status: fixed-in-rust. C# believed correct from the source (StrRep is part of every node's feature structure); not yet verified by running C# on a wide featured grammar. Rust-only implementation bug, so no Machine issue unless that check says otherwise.
Evidence: synthetic code-constructed snapshot regressions in `pg-cli/tests/wide_table_segment_identity.rs`.

## C# site

Every shape node carries its `StrRep` char-def set as a feature, so a literal or a natural class in
an affix `Lhs`, an allomorph environment or a rewrite-rule environment names specific segments
whatever the feature system and the number of character definitions. (`Allomorph.cs::IsWordValid`
environment clause; rewrite-rule environment matching in `RewriteRuleSpec`s.) No width limit.

## Rust site

Entry 090 gave featureless grammars chunked `StrRepMatcher` lanes. `StrRepMatcher::new` returns
`None` when the grammar has phonological features, so a featured grammar had identity in only one
place: the single-`u64` id lane of allomorph and affix matching, available only up to 64 char-defs.
Two gaps remained. Above 64 char-defs no identity lane existed at all. At any size, rewrite-rule
environments (`compile_env_impl` with `id_lane` false) never carried identity on a featured grammar.
A segment with no feature values (an inferred or provisional letter) unifies with every authored
segment, so `/ a _` or `/ k _` matched it: a suffix allomorph licensed after `a` applied after the
letter `x`, and `a -> o / k _` fired after `x`.

## What changed

`StrRepMatcher::identity` builds the chunked lanes for any grammar; they sit after the feature
lanes. They are used by `segs_of` and `compile_parts` when the id lane is unavailable, by
`compile_env_impl` for allomorph and rewrite environments, and by `MutShape::segs` for rewrite
inputs, which also keep their `Members` identity sets on a featured grammar. A feature-changing
rewrite resets a node's identity to unknown (all ones), so identity only ever removes a wrong match.
A `[Class]` of segment members compiles to one alternative per 63-id chunk it touches instead of one
per member. The featureless path (`StrRepMatcher::new`, the LHS targets) is unchanged; `rewrite_identity`
gained only its featured branch, which now also clears the node's identity set.

## Can it change a parse?

Yes, only by removing a wrong match: environments on featured grammars now discriminate segments
that feature lanes cannot tell apart, as C# does. A node whose identity is unknown (inserted or
feature-rewritten) still matches any literal, so the over-approximation that remains is the one
already present at 64 char-defs and below. The analysis-side `Lhs` (`build_analysis_lhs`) and the
rewrite `Lhs` targets are unchanged and still over-approximate on featured grammars; synthesis
re-validates.

## Coverage

`pg-cli/tests/wide_table_segment_identity.rs`: the featured suffix and rewrite cases at 80 and 420
char-defs, the featured rewrite case at 14 and 64 char-defs, and the featured suffix case at 65
fail without the fix. Featureless and featured cases at exactly 64 and 65 char-defs pin the boundary.
