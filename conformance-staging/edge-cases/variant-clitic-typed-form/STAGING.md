# variant-clitic-typed-form

Synthetic. A sense-less FieldWorks entry `=ke` of the enclitic morph type whose variant ref names the stem `ta`. HCLoader splits every entry's forms into stem and clitic buckets and runs LoadLexEntries and LoadMorphologicalRules for each (HCLoader.cs:299-305), so the variant is loaded in the Clitics stratum as a stem copy and as a clitic rule, both under `ta`'s MSA (LoadLexEntryOfVariant HCLoader.cs:782-856; LoadMorphologicalRules HCLoader.cs:896-924). PanGloss built variants for the Morphology bucket only.

## What it pins
- `ta`, `ke`, `take`, `keke` parse; `tu` has no analysis.

## Oracle
hc.dll at Machine `aab4c59819adb7b366915e09df2fb28c62b8c03a` on `grammar.xml` and on the HC XML the real HCLoader emits for `fieldworks/project.fwdata` (XampleProjector `project`): ta, ke, take, keke parse once in both; tu none. Provenance: `founding-oracle machine-commit=aab4c59819adb7b366915e09df2fb28c62b8c03a verified-date=2026-11-03`.

## FieldWorks source witness
`fieldworks/project.fwdata` was authored by `tools/xample-projector author --variant-of eTe=eTa` (LibLCM), then ONE edit: the variant's MoStemAllomorph MorphType was changed from stem to enclitic (d7f713e1-e8cf-11d3-9764-00c04f186933), which `author` refuses to produce for a variant. No LDML is carried.
