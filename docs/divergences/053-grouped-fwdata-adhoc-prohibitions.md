# 053 — Grouped ad hoc prohibitions were omitted from `.fwdata` imports

## Kind
Behavioural.

## Status
Reverted-in-rust.

## C# site
FieldWorks `HCLoader.cs:340-350` at source revision
`089eb9027b6d81be7883c40960f0de3ffa04b699` enumerates the allomorph and morpheme ad hoc
prohibition repositories separately. It filters concrete rules by their own `Disabled` flag,
primary reference, and nonempty rest list. Group objects organize concrete rules; their state does
not replace the concrete rule checks. `LoadAllomorphCoOccurrenceRules` and
`LoadMorphemeCoOccurrenceRules` attach the rule data later in `HCLoader.cs`.

## Rust site
`pg-fwdata/src/xml.rs::ALLOWED_CLASSES` must retain `MoAdhocProhibGr` so its members survive graph
parsing. `pg-fwdata/src/extract/inventory.rs::class_role` treats it as a carrier, not a tracked
snapshot atom. `pg-fwdata/src/extract/morphology.rs::extract_adhoc_prohibitions` now enumerates the
concrete `MoMorphAdhocProhib` and `MoAlloAdhocProhib` graph records, independent of whether each is
listed directly by `MoMorphData` or owned by one or more groups. It preserves the order of each
rule's `RestOfMorphs`/`RestOfAllos` targets and reports broken group links.

## What differed
The importer followed only `MoMorphData.AdhocCoProhibitions` as though every item were a concrete
rule. `MoAdhocProhibGr` was not an allowed XML class, so the parser discarded the group before
extraction could read its `Members`; a grouped prohibition therefore vanished and never blocked a
parse. This was a real importer behavior bug, not only an inventory health warning.

## Correct behavior
The FieldWorks loader reads concrete repository instances regardless of how they are organized for
users. Active concrete members therefore enforce their prohibition whether referenced directly or
through a group. The concrete rule's `Disabled` value controls it; a group's `Disabled` value does
not disable otherwise active members. The importer retains disabled and no-op source rules in the
snapshot; grammar compilation skips them as HCLoader does when constructing parser rules. Flat and
grouped projects containing the same concrete rules must produce the same effective parses.

## Pinning fixture and conformance coverage
`rust/crates/pg-cli/tests/fwdata_conformance_gate.rs::grouped_fwdata_adhoc_rules_match_flat_hcloader_semantics`
constructs grouped and flat `.fwdata` twins from
`machine/conformance/edge-cases/deep-optional-affix-nesting/fieldworks/project.fwdata`, whose bare
`k` and twelve-prefix `xxxxxxxxxxxxk` parses are pinned by the FieldWorks-oracle conformance case.
The FieldWorks export and its `words.yaml` are pinned by the Machine conformance submodule revision
`18cf242f4b114b0eb9bac304b4b171ca2f499a39`.
It includes nested group ownership, morpheme and allomorph prohibitions, disabled concrete rules,
an empty rule, and a dangling group member. It compares imported concrete rules and parser results
for a prohibited sequence and an unrelated word. The test's FieldWorks source provenance is pinned
to the `HCLoader.cs:340-350` implementation above. The carrier classification is also pinned by
`pg-fwdata/src/extract/inventory/tests.rs::carrier_classes_are_never_tracked`.

This adds the missing importer-path discriminator. Before this case, co-occurrence coverage came
through HC XML that had already flattened FieldWorks grouping, while no sample `.fwdata` contained
`MoAdhocProhibGr`. The existing gate consequently never exercised this importer path.

## Evidence and limits
The parser and fixture changes are present in PanGloss; the `pg.ps1` gate could not run in the
managed jail because its required cgroup memory cap is unavailable. The FieldWorks source was
inspected at the revision above, but this change does not claim a new execution of the C# loader.
The owner should run the listed PanGloss gates before committing.

## Upstream
No Machine issue or upstream change is needed. This was an importer omission in PanGloss; FieldWorks
`HCLoader` already loads the concrete repository rules.
