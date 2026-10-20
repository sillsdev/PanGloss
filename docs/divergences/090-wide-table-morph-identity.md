# 090: Morphological and allomorph-environment matching lost segment identity on wide featureless tables

Kind: behavioural.
Status: fixed-in-rust. C# was already correct; no Machine issue (Rust-only implementation bug).
Evidence: synthetic code-constructed snapshot regressions; CEB-14 repro on a copy of a Cebuano project, not committed.

## C# site

`SIL.Machine.Morphology.HermitCrab` matches every shape node by its `StrRep` char-def set; a
literal or a natural class in an affix `Lhs` or allomorph environment distinguishes segments
whatever the number of character definitions. (`Allomorph.cs::IsWordValid` environment clause; the
affix process `Lhs` is matched in `AffixProcessRule` synthesis.) No width limit exists.

## Rust site

`pg-rules/src/bridge.rs::id_lane_width` returns `None` when the table has more than 64 char-defs,
and `pg-rules/src/morph.rs::segs_of`, `compile_parts` and `rewrite.rs::compile_env_impl` then
dropped the identity lane. On a grammar with no phonological features (every authored phoneme
featureless, so feature lanes cannot tell segments apart) a literal or `[Class]` in a suffix's
left environment matched every segment. The suffix's `Lhs` is where a left environment lives, so
`hun` with `/ [V] _` applied after any stem, and the disjunctive re-check then rejected the plain
form `un` everywhere. A right environment, and any project with at most 64 char-defs
(authored phonemes plus provisional letters), was unaffected. Slot, template, inflectional versus
derivational MSA and digraph phonemes are not factors.

## What changed

When the id lane is unavailable and the grammar has no phonological features, those three sites
use the chunked `StrRepMatcher` lanes that phonological rewrite rules already use. Tables of at
most 64 char-defs keep the id lane unchanged. Featured grammars were not covered by this entry and
still lost identity above 64 char-defs; entry 091 closes that.

## Can it change a parse?

Yes, only by removing a wrong match: affix `Lhs` and allomorph environments on featureless
grammars with over 64 char-defs now discriminate segments as C# does. The analysis-side `Lhs`
(`build_analysis_lhs`) is unchanged and still over-approximates there; synthesis re-validates.

## Coverage

`pg-cli/tests/slotted_suffix_left_environment.rs`: `slotted_left_environment_survives_a_large_letter_inventory`
and `unslotted_left_environment_survives_a_large_letter_inventory` fail without the fix; the two
small-inventory controls pass either way.
