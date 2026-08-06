# pg-grammar compile/reachability.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-grammar/src/compile/reachability.rs`
implementation comments so the source can carry a one-line pointer instead of the full argument.
This module is the mrule/morpheme-scoped sibling of `super::natclass::compact_to_referenced`; read
that function's doc first — this module mirrors its used-set + remap-with-`expect` pattern
exactly, extended to cover the extra wrinkle `MRuleId` has that `NatClassId` doesn't: an
owner-registry back-reference, not just read-only structural edges.

## Why `mrules` needs this at all

`compile_project` already filters *templates*: a disabled `MoInflAffixTemplate` never becomes an
`AffixTemplateDef`. That filtering does not propagate to the affix-process rule(s) whose only slot
lives in that disabled template. `lexicon::build`'s own doc explains why: an `Msa::Inflectional`
MSA with at least one slot is `template_only` (HCLoader's `LoadMorphologicalRule`,
HCLoader.cs:887-892: `if (inflMsa.SlotsRC.Count > 0) s = null` — the rule is reachable only through
`AddMorphologicalRule`'s per-slot fallout, `stratum.MorphologicalRules` never sees it). Such a rule
is still built unconditionally (`LoadMorphologicalRules` is called per-MSA regardless of template
membership) and still lands in `acc.mrules`/`acc.slot_rules[slot_guid]` — but if every slot
referencing it belongs to a disabled template, nothing in the final `Grammar` ever records that
`MRuleId` anywhere HCLoader's own exporter would visit (neither a stratum's own `mrules` list nor
an enabled template's slot content). HCLoader's XML export walks exactly those two places, so this
reference set is precisely "every place the legacy exporter looks."

## Why the allomorph-owner registry needs a cascade, unlike `natclass`

`super::natclass::compact_to_referenced` only ever reads a `NatClassId` from structural sites
(patterns/environments/rules) — dropping an unreferenced class breaks nothing else, because
nothing else's own identity depends on that class's position in the `Vec`. `Grammar::allomorph_owners`
is different: it's an owner registry, indexed by `AllomorphId`, and every
`RootAllomorphDef`/`AffixAllomorphDef` in the grammar carries its own `id` field that must
round-trip back to its position in that registry (`compile::tests::assert_grammar_ids_are_internally_consistent`
checks exactly this, and several `pg-rules` consumers enumerate `allomorph_owners` in registry
order and dereference every entry unconditionally — a stale `AllomorphOwner::Affix` pointing at an
`MRuleId` outside the now-shorter `mrules` `Vec`, or silently aliasing an unrelated surviving rule
after the shift, is a live bug, not a merely-unobserved one). Dropping an `MRuleId` that owns
allomorphs therefore requires: dropping its `AllomorphOwner::Affix` rows from `allomorph_owners`
too, remapping every surviving `AllomorphId` to a dense index, and fixing up every surviving
allomorph's own `id` field plus any `AllomorphCoOccurrenceRuleDef.others` reference through the
same table (steps 3-4 in `compact_mrules`).

## Morpheme co-occurrence rules

Separately, `MorphemeCoOccurrenceRuleDef`s live on `Grammar::morphemes[i].co_occurrence`
(`strata_assign_co_occurrence` in `compile/mod.rs` populates them from the snapshot's
`MoMorphAdhocProhib` ad-hoc rules, keyed by MSA guid via `acc.msa_guid_index`) — entirely
independent of the `mrules`/`allomorph_owners` cascade above. `Grammar::morphemes` is never
compacted (nothing else needs `MorphemeId` to stay dense — the gate's own multiset comparisons
resolve `MorphemeId` by content, never by raw index), so no id remap is needed there. But a
morpheme whose sole mrule was just dropped above is exactly as unreachable as that mrule was, and
HCLoader's own XML export skips such a morpheme's `MorphemeCoOccurrenceRules` too (same "walks
only what's stratum/template-reachable" principle) — so any co-occurrence rule keyed to (or
targeting, via `others`) such a morpheme must be dropped too, which is what
`trim_unreachable_morpheme_coocurrence` does.
