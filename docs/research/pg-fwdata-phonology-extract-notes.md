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

## `extract_metathesis_rule`: StrucChange indices

`PhMetathesisRule` declares one field of its own, `StrucChange`, a `String` holding five
space-separated integers, each -1 when absent: `[left env, right switch, middle(:L|:R), left switch,
right env]` (`PhMetathesisRuleTags.kidx*`, read by `GetStrucChangeIndices()`). `HCLoader.LoadMetathesisRule`
(HCLoader.cs ~335, ~2152) names the node at `RightSwitchIndex` "r" and sets it as `LeftSwitchName`, and
names the node at `LeftSwitchIndex` "l" and sets it as `RightSwitchName`; a rule with either switch
unset is skipped. The snapshot carries `left_switch_index` = field 3 and `right_switch_index` =
field 1, so the grammar compiler maps them the same way. `StrucChange` is a `<Str>` element in
fwdata, so it is read with `str_text`. Verified against hc.dll on a LibLCM-authored project
(`crates/pg-parse/tests/data/fw-metathesis`).

## `resolve_phon_context`

Resolves a `PhContextOrVar` guid into a `PhonContext` tree. Shared by phonological rules/
environments (this module) and `MoAffixProcess.InputOS` (`extract::lexicon`) — mirrors
`HCLoader.LoadPatternNode`, HCLoader.cs:2313-2389.

The well-known word-boundary marker (`LangProjectTags.kguidPhRuleWordBdry`,
HCLoader.cs:2351/2489-2498) is excluded from `boundaryMarkers` (`LoadCharacterDefinitionTable`,
HCLoader.cs:2698`). A `.fwdata` export can store that reserved GUID as an owned `PhBdryMarker` record.
The reserved GUID itself identifies the word-boundary anchor, whether or not a `PhBdryMarker`
record resolves. Other resolved marker GUIDs remain literal boundaries. An unresolved ordinary
boundary reference retains the existing `WordBoundary` fallback rather than silently removing the
rule's shape.
