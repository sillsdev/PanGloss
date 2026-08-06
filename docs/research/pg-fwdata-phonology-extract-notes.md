# pg-fwdata extract/phonology.rs: HCLoader correspondence notes

Longer arguments pulled out of `rust/crates/pg-fwdata/src/extract/phonology.rs` implementation
comments so the source can carry a one- or two-line pointer instead of the full argument.

## `extract_phoneme_set`

`HCLoader` only ever loads the first phoneme set (HCLoader.cs:204); this port does the same,
warning if there is more than one — an unusual project configuration this format has no place to
keep the rest of.

## `code_representations`

`PhPhoneme.CodesOS`/`PhBdryMarker.CodesOS` is an owned sequence of `PhCode`, each carrying one
`Representation` `MultiUnicode`; this flattens every code's forms together, dotted-circle stripped.

## `first_code_representation`

The first `PhCode`'s representation for a phoneme/boundary-marker guid, used only by
`crate::extract::lexicon` to build `MoInsertPhones`' concatenated literal string
(`HCLoader.cs:1388-1406`: `termUnit.CodesOS[0]`, vernacular-default for phonemes, best-vernacular
for boundary markers).

## `feature_constraint_variables` (rewrite rule extraction)

`PhRegularRule.FeatureConstraints` is a virtual LCM property (`OverridesLing_Lex.cs`,
`GetFeatureConstraintsExcept(null)`) — the raw `.fwdata` record has no such field. It is the
deduplicated walk of every natural-class context's `PlusConstr` then `MinusConstr` lists, visiting
`StrucDesc`, then each RHS's `StrucChange`/`LeftContext`/`RightContext`, in order — recomputed here
over the already-extracted context trees so the snapshot carries the same variable scope HCLoader
sees (it assigns per-rule variable names by this collection order).

`collect_feature_constraint_vars` is the recursive walk `PhRegularRule.CollectVars` does
(`OverridesLing_Lex.cs`): sequence and iteration contexts recurse into their members; a
natural-class context contributes its `PlusConstr` list then its `MinusConstr` list, first
occurrence wins (deduplicated).

## `resolve_rule_features`

`ReqRuleFeats`/`ExclRuleFeats` are lists of `PhPhonRuleFeat` wrapper guids; the guid this format
actually wants is each wrapper's `Item` (an `MoInflClass` or `CmPossibility`) —
`HCLoader.LoadMprFeatures`, HCLoader.cs:2610-2623.

## `extract_metathesis_rule`: the model gap

Undetected until real-project verification found neither fixture project exercises metathesis rules
at all. `docs/snapshot-format.md` documents `MetathesisRule.left_switch_index`/`right_switch_index`
as coming from `PhMetathesisRule.LeftSwitchIndex`/`RightSwitchIndex` integer fields. Those fields do
not exist in the current LCM schema (`MasterLCModel.xml`, `PhMetathesisRule` class num 130): the
only field `PhMetathesisRule` itself declares is `StrucChange`, a `String` — an ordered sequence of
integers separated by a space, each a 1-based position into the (inherited from `PhSegmentRule`)
`StrucDesc` sequence, giving the output order. `HCLoader.cs` doesn't read two switch-index integers
either; it calls a compiled-only `GetStrucChangeIndices()` helper that parses this same string
(HCLoader.cs:2119-2161, `PhMetathesisRuleTags.kidx*`).

Since `pg-snapshot`'s model can only represent a simple two-element swap (not an arbitrary
permutation), this parses `StrucChange` and takes the leftmost and rightmost 1-based positions that
differ from the identity permutation as `left_switch_index`/`right_switch_index` (0-based) — exact
for a plain A...B -> B...A swap (the common case), an approximation for anything fancier (warned).
Unverified against real data: neither Sena 3 nor Amharic contains a single `PhMetathesisRule`.

## `resolve_phon_context`

Resolves a `PhContextOrVar` guid into a `PhonContext` tree. Shared by phonological rules/
environments (this module) and `MoAffixProcess.InputOS` (`extract::lexicon`) — mirrors
`HCLoader.LoadPatternNode`, HCLoader.cs:2313-2389.

The well-known word-boundary marker (`LangProjectTags.kguidPhRuleWordBdry`,
HCLoader.cs:2351/2489-2498) is excluded from `boundaryMarkers` (`LoadCharacterDefinitionTable`,
HCLoader.cs:2698) — it never appears as its own `PhBdryMarker` record a user could define, so
"doesn't resolve to a real `PhBdryMarker`" is exactly the structural signature of the `#` anchor.
This also means a genuinely dangling ordinary boundary reference degrades to the same harmless `#`
interpretation rather than silently vanishing from the pattern — preferable to dropping the rule's
shape entirely.
