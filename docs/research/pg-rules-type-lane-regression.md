# Regression: the `Type` / boundary-lane fix

`type_lane_gate.rs` pins a fix to `PatternBridge::char_def_lanes` (`pg-rules/src/bridge.rs`): a
literal `<BoundaryMarker>` pattern node must match only boundary shape nodes, and a literal
`<Segment>`/natural-class pattern node must match only segment shape nodes, never the reverse.

## Root cause

Before the fix, `char_def_lanes` returned a boundary char-def's `feature_lanes()` as-is, which was
an empty `Vec` — `pg-grammar/src/chardef.rs` never attached a `Type` lane, or any lane at all, to
boundaries. `pg_fst`'s `flat_unifiable` treats an absent lane as unconstrained, so a length-0
constraint vector canonicalizes to "matches any segment": the confirmed root cause of the
`meN-`/`peN-`-prefix boundary-environment bug.

These tests drive the real `PatternBridge` → `pg_fst::Transduce` path end-to-end, not just
inspecting stored lane bits, because the bug's symptom lives in how those bits get consumed, not
just how they are stored — a test that only checked `feature_lanes()[type_idx]` would pass even if
some consumer still special-cased boundaries as unconstrained.

## Two width regimes

- `zero_feat_grammar` mirrors Sena exactly: no `<PhonologicalFeatureSystem>` at all, so
  `phon_features.len()` goes from 0 to 1 (the synthetic `Type` feature is the only one).
- `feature_grammar` mirrors Indonesian/Amharic: one real symbolic feature, so `len()` goes from N
  to N+1, and additionally pins that a `FeatureNaturalClass` keyed on a real feature still matches
  exactly the right segments post-fix — no regression on real phonological matching.
