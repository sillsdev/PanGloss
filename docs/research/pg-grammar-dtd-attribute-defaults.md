# pg-grammar load.rs: DTD attribute defaults pinned by `dtd_attribute_defaults_match_spec`

Moved out of an implementation comment in `rust/crates/pg-grammar/src/load.rs` so the source can
carry a one-line pointer instead of the full list.

C#'s DTD-validating XML reader (`XmlLanguageLoader.cs:209-218` + `HermitCrabInput.dtd:259`, `final
(true | false) "true"`) materializes the DTD default into the parsed tree, so an omitted attribute
reads as its DTD default in C#. The loader must match. `dtd_attribute_defaults_match_spec` pins
every DTD `ATTLIST` default reachable from `load.rs` (a full sweep against
`HermitCrabInput.dtd`, not just a single spot-check), so a future change can't silently
reintroduce a mismatch of this class:

- `AffixTemplate final` "true" (the original bug this test was written for)
- `Slot optional` "false"
- `MorphologicalRule blockable` "true", `partial` "false", `multipleApplication` "1"
- `Allomorph isBound` "false"
- `MorphologicalOutput redupMorphType` "implicit"
- `MorphologicalPhonologicalRuleFeatureGroup matchType` "any", `outputType` "overwrite"
- `Stratum morphologicalRuleOrder` "linear"
- `PhonologicalRule multipleApplicationOrder` "leftToRightIterative"

`isActive` "yes" is exercised pervasively elsewhere via `Node::is_active`. `Stratum`'s
`cyclicity`/`phonologicalRuleOrder` attributes are not modeled by this loader at all — a separate
architectural scope-cut, not a wrong-default bug, and neither attribute appears in any of the
three reference grammars.
