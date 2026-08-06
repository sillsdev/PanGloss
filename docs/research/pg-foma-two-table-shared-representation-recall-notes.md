# pg-foma two_table_shared_representation_recall.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/tests/two_table_shared_representation_recall.rs`
implementation comments so the source can carry a one- or two-line pointer instead of the full
argument.

## What this file proves

The recall-side counterpart to `tests/cover_bistratal_overlapping_segment_representation.rs` (the
refusal/verdict-side pin). Fixture:
`conformance-staging/edge-cases/two-table-shared-representation-recall`. Proves, in three steps,
over the real production compile path (`pg_foma::replace`, never a hand-rolled token-math
simulation):

1. **The loss is real.** A rule net compiled the pre-fix way (`SegAlphabet::token`, table-blind, no
   aliasing) never fires when fed a token drawn from a different table's raw index for the same
   spelling, even though the oracle (`pg_parse::Morpher`, which resolves every segment via genuine
   feature-lane unification, never a raw-index comparison) correctly analyzes the corresponding
   surface word.
2. **The fix closes it.** The same rule, compiled via the current (fixed)
   `pg_foma::replace::compile_and_compose_rules_with_budget`, does fire on that exact material:
   cross-table representation aliasing (`crate::replace::RepresentationAliasMap`/
   `SegAlphabet::render_tokens`) renders the rule's atom as a union over every table's own token for
   the shared spelling.
3. **Containment holds end to end.** The full compiled pipeline (lexc composed with rules), decoded
   via `apply_up`, finds exactly the analyses `pg_parse::Morpher` finds — no more, no less — for
   every word in the fixture.

## A separate, orthogonal, out-of-scope finding surfaced while authoring this fixture

`pg_parse::Morpher::parse_word_opts("y", ..).signature()`'s surface half renders empty (`"ROOT1|"`,
not `"ROOT1|y"`) for the cross-stratum-synthesized analysis in this fixture, even though the
morpheme-level analysis (root identity, `structured`) is exactly correct. Confirmed not
multi-table-specific (an equivalent single-table environment-free feature-changing rule renders its
surface half correctly, `"ROOT|y"`), and confirmed to persist regardless of which table is
`TableId(0)`. This looks like a genuine (if narrow) `pg_parse`/`pg_rules` synthesis-side
stratum-bookkeeping gap: `pg_rules::stratum::synthesize_stratum_traced` never updates a candidate
`Word`'s own `.stratum` field the way `analyze`'s un-apply direction does, so
`Morpher::surface_of`'s `g.strata[w.stratum.0].table` lookup for a root synthesized past its own
entry stratum may resolve the wrong table — a different crate, a different bug class, out of scope
for this file's `pg-foma`-only boundary. This file's own containment check therefore compares
morpheme-level `structured` analyses (root + morpheme ids), never the surface-string half of
`signature()`.
