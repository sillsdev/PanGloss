# 090: Morphological and allomorph-environment matching lost segment identity on wide featureless tables

Kind: behavioural.
Status: fixed-in-rust. C# was already correct; no Machine issue (Rust-only implementation bug).
Evidence: synthetic code-constructed snapshot regressions; CEB-14 repro on a copy of a Cebuano project, not committed; before/after batch comparisons for the analysis-side `Lhs`, below.

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

### Analysis-side `Lhs`

`build_analysis_lhs` (affix and compounding analysis) constrained each `InsertSegments` literal by
the id lane only, so on the same wide featureless tables an affix literal such as a suffix `un`
matched any segments in analysis: a suffix was unapplied from every word long enough, and synthesis
then rejected the result. Parses were unchanged but the search was not: on a featureless Cebuano
project with over 64 char-defs the over-matching cost about 50x in steps. The literal now carries the same chunked
`StrRepMatcher` constraint, which `segs_of` input lanes already supply on this path, and only when
the inserted shape's table is the matched one.

C#'s `InsertSegments.GenerateAnalysisLhs` adds each inserted node's feature structure as a
constraint, and `CharacterDefinitionTable.Add` gives a segment a `StrRep` only when it is defined
without features, so on a featureless grammar a literal matches only itself. A featured node has
no `StrRep` and unifies with a featureless literal, so the constraint is applied only when the
grammar has no phonological features (`StrRepMatcher::new`); featured wide tables (Amharic) keep
the feature-only over-approximation, which synthesis re-validates.

Remaining: an `InsertContext` (natural-class insertion) in `build_analysis_lhs` still carries no
identity on tables over 64 char-defs and over-matches; synthesis re-validates it.

## Can it change a parse?

Yes, only by removing a wrong match: affix `Lhs` and allomorph environments on featureless
grammars with over 64 char-defs now discriminate segments as C# does. The analysis-side change
cannot: it removes only analysis branches that synthesis rejected, as C# never explores them.

## Coverage

`pg-cli/tests/slotted_suffix_left_environment.rs`: `slotted_left_environment_survives_a_large_letter_inventory`
and `unslotted_left_environment_survives_a_large_letter_inventory` fail without the fix; the two
small-inventory controls pass either way.

`analysis_suffix_literal_matches_only_itself_on_a_large_letter_inventory` reads the suffix rule's
analysis outputs from the stats collector: a word not ending in `un` or `hun` yields none. With the
analysis-side fix removed it fails (`left: 2`, both allomorphs unapplied); the 0-extra-letter
control `analysis_suffix_literal_matches_only_itself` passes either way.

Parse neutrality of the analysis-side fix, release builds before (`fa3bfb18`) and after,
`pangloss batch --threads 2` with the default step cap and no wall timeout, compared with
`parse_compare.py --multiset`: every row IDENTICAL, no caps, no timeouts, on three private
Cebuano lists (200, 563 and 7,206 words; steps down 54x, 46x and 55x) and on the Amharic (673
words), Sena (6,146) and Indonesian (70) reference grammars (steps unchanged). The Aweti and
Mbugwe grammars have at most 64 char-defs, so the new branch cannot run on them; they were not
rerun.
