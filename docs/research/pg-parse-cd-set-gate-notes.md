# pg-parse cd_set_gate.rs: char-def-set fix regression notes

Regresses the char-def-set fix for `InsertSimpleContext`/`OutputAction::InsertContext` natural-class
insertion: it must render/match as exactly the class's real members, never the whole char-def table.

## The bug this pins

On a grammar with zero phonological features (Sena's actual "mbali" situation), the pre-fix
lane-only representation was no constraint at all: every char-def's lanes are `&[]`, so
`flat_unifiable(&[], &[])` is vacuously true for the entire table. A natural-class insertion then
rendered/matched as the whole char-def table's inventory instead of the class's real members.

## Two hand-built grammars

- `zero_feat_segments_class_renders_only_its_members` mirrors Sena's situation directly: a grammar
  with zero authored phonological features (`g.phon_features.is_empty()` — `len()` is never 0
  post-fix because of the always-appended synthetic `Type` feature, so `is_empty()` is the correct
  check). Root `"bali"` plus a prefix inserting the 2-member `Segments`-kind `nc_nasal` class must
  render as `[mn]bali`, exactly the class's members, not the whole 9-char-def table.
- The feature-bearing pair mirrors Indonesian/Amharic: one symbolic feature (voice), five segments —
  b/d/g/a voiced, p voiceless — so a `Segments`-kind class of `{b, d}` shares its lane-union with `g`
  and `a` without being identical to the whole voiced set.
  - `feature_grammar_segments_class_narrows_tighter_than_lane_union`: `nc_bd` is `Segments`-kind with
    exactly `{b, d}`, but `g` and `a` share the same voice lane. The pre-fix lane-union
    representation would have admitted `g` and `a` too; the fix must render exactly `[bd]p`, not
    `[bdga]p`.
  - `feature_grammar_feature_class_behavior_is_unchanged`: `nc_voiced` is `Feature`-kind (`voi+`),
    matching `b`/`d`/`g`/`a` but excluding `p`. This must render as the full lane-unifying set,
    `[bdga]`, unchanged by the fix — a `Feature`-kind class's char-def set is derived from the lanes,
    not an independent restriction.

## Why the root text is `"p"`, not `"a"`, in the feature-bearing tests

`p` is the table's only `voi-` segment, so it is `FeatureStruct`-unique and its own rendering stays
a plain `"p"`. Rooting on `"a"` would (correctly) also render the root's own segment as `[bdga]`
— confirmed against the C# oracle (`CharacterDefinitionTable.cs:125`,
`new ShapeNode(cd.FeatureStruct.Clone())`: a feature-bearing char-def's segmented node carries no
`StrRep` at all, so `GetMatchingStrReps` genuinely unifies `a` against `b`/`d`/`g` too, since this
minimal fixture gives all four an identical `Type+voi+` `FeatureStruct`). That is the fix working as
designed, not a bug, but it would conflate the assertion about the inserted class node with an
unrelated (also-correct) change to the root node's own rendering. `p` isolates the assertion to just
the inserted class's narrowing.

## `root_word`

Builds the root's shape the way production code actually does (`Morpher::set_root_allomorph`), via
`segment_with_features`, which attaches each node's real per-char-def phonological lanes, not the
bare feature-less `pg_grammar::segment::segment`. This matters for the feature-bearing tests: with
unfilled (unconstrained) lanes the root's own concrete segment would misrender regardless of this
fix, testing the wrong thing. At `feat_width == 0` (the zero-feature test) the two are identical by
construction, so the char-def-set fix is the only discriminator available there.
