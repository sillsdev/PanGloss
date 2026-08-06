# G4 trace event nesting (`tests/trace_gate.rs`)

The five previously-unwired analysis-side trace events (`begin_unapply_stratum`/
`end_unapply_stratum`/`begin_unapply_template`/`end_unapply_template`/`lexical_lookup`) are pinned
against a synthetic, single-stratum, single-template grammar small enough to hand-derive the exact
expected tree shape. See the wiring sites' own doc comments in `pg-rules/src/stratum.rs`'s
`analyze`/`analyze_template`/`template_unapply_slots` and `pg-parse/src/morpher.rs`'s
`lexical_lookup_filtered`.

## Cursor reassignment discipline

`BeginUnapplyStratum`/`EndUnapplyStratum` (for `input` itself)/`BeginUnapplyTemplate` never
reassign the trace cursor, mirroring the already-wired synthesis-side `begin_apply_stratum`/
`end_apply_stratum`/`begin_apply_template`, so all three fire as DIRECT children of root.

## Ordering

Children are appended in call order: Begin before End; End(`input`) before BeginUnapplyTemplate.
This port evaluates `apply_templates`/`apply_mrules` eagerly, so the `input`-itself
`EndUnapplyStratum` is placed textually before that computation starts — see `analyze`'s own doc
comment for why that reproduces C#'s lazy-`IEnumerable` event order.

## The two levels of `TemplateAnalysisOutput`

The mandatory slot's OWN level always exits `unapplied=false` for the ORIGINAL word
(`AnalysisAffixTemplateRule.cs:71-72`) — also a direct child of root, no cursor reassignment there
either; `TreeTraceSink::end_unapply_template` only sets `.output` when `unapplied`, so "no
`output`" identifies this exit.

The RECURSED (fully-consumed) level exits `unapplied=true` for the UNAPPLIED word
(`AnalysisAffixTemplateRule.cs:77-78`) — and that word's trace cursor was already reassigned by the
already-wired rule-level `MorphologicalRuleAnalysis` event (`morph.rs::ana_affix_cached_traced`),
so this exit nests UNDER that rule event, not as a second sibling of the bookends above.

The per-word `EndUnapplyStratum` for the surviving unapplied word is subject to the exact same
cursor-reassignment rule (`AnalysisStratumRule.cs:141-143`, C#'s `output.Add(...)`
unconditional-then-trace idiom): a SECOND `StratumAnalysisOutput` must nest under a
`MorphologicalRuleAnalysis` event too, rather than becoming a second direct child of root.
