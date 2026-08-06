# pg-grammar load.rs: `load_metathesis_rule` design notes moved out of comments

Longer argument pulled out of `rust/crates/pg-grammar/src/load.rs`'s `load_metathesis_rule`
implementation comment so the source can carry a one-line pointer instead of the full argument.

## Why there is no `Group` pattern-node kind

`LoadMetathesisRule` has no `VariableFeatures` scope (the DTD's `<MetathesisRule>` has none) and no
default char-def table context, the same convention `load_rewrite_rule` documents for
`<PhonologicalRule>`'s LHS/RHS: `Segment`/`BoundaryMarker` reference char-defs by a
table-independent global IDREF; only a nested `<Segments>` element's own optional
`characterDefinitionTable` attribute needs a fallback, and C# passes `null` there too — ported as
`TableId(0)`, matching every reference/fixture grammar's single-table convention.

**Group-authoring.** The DTD has no `<Group>` element. `XmlLanguageLoader.LoadMetathesisRule`
instead builds a `groupNames` dictionary mapping {the id the `leftSwitch` attribute references → an
internal name, the id the `rightSwitch` attribute references → another internal name} and has
`LoadPatternNodes` wrap whichever single pattern element carries a matching `id` attribute in a
`Group` of that name — so `MetathesisRule.LeftSwitchName` ends up bound to whatever `leftSwitch`'s
IDREF points at (C# names that internal group `"r"`; the naming is an implementation detail — see
the `MetathesisRuleDef` doc for why "left" doesn't mean "physically left").

This port skips minting a `Group` pattern-node kind entirely: since a real grammar's
`<MetathesisRule>` can only ever validly switch-tag a single `<SimpleContext>`/`<Segment>`/
`<BoundaryMarker>` element (a `<Segments>`/`<OptionalSegmentSequence>` switch group is DTD-legal
but fails to compile against the real C# engine — see
`rust/conformance/metathesis/complex_rule/README.md`'s finding), recording each switch as a plain
index into `pattern.nodes` is sufficient and avoids adding an authored-`Group` pattern-node kind
(and the matching `pg_rules::bridge::PatternBridge` case) that would only ever wrap one node.
