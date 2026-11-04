# variant-complex-form-ref-senseless-entry

Synthetic. A sense-less FieldWorks entry `tu` whose one LexEntryRef is a complex-form ref (ComplexEntryTypes set, VariantEntryTypes empty, RefType 1) to the stem `ta`. HCLoader treats every entry ref of a sense-less entry as a variant source (LoadLexEntries, HCLoader.cs:639-668; GetInflTypes yields the plain type when VariantEntryTypes is empty, HCLoader.cs:706-710), so `tu` parses as `ta`'s Noun MSA. PanGloss modelled the ref as `EntryRef::ComplexForm` and never built it (pangloss issue for gap variants).

## What it pins
- `ta`, `tu`: both parse as `TA`; `tata` has no analysis.

## Oracle
hc.dll at Machine `aab4c59819adb7b366915e09df2fb28c62b8c03a`, `hc-dotnet-wrapper.sh batch`, on `grammar.xml` and on the HC XML the real HCLoader emits for `fieldworks/project.fwdata` (XampleProjector `project`, FieldWorks 9.3.10.1452): ta 1 parse, tu 1 parse in both. Provenance: `founding-oracle machine-commit=aab4c59819adb7b366915e09df2fb28c62b8c03a verified-date=2026-11-03`.

## FieldWorks source witness
`fieldworks/project.fwdata` was authored from the grammar by `tools/xample-projector author --grammar <variant-form grammar> --variant-of eTu=eTa` (LibLCM), then ONE edit was made to the LexEntryRef: it gained `<ComplexEntryTypes>` (the stock "comp. of" type 1f6ae209-141a-40db-983c-bee93af0ca3c) and `RefType` 1, which `author` cannot produce. The edit is the ordinary UI shape of a sense-less compound entry. No LDML is carried (the PanGloss import does not read it).
