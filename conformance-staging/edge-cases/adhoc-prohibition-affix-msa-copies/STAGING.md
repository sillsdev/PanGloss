# adhoc-prohibition-affix-msa-copies

Synthetic. An ad hoc allomorph prohibition on a prefix entry with two equal MoInflAffMsas (MergeMSAs off), so one prefix allomorph compiles to two morphological rules. HCLoader attaches the prohibition to every copy, on the first side and on the rest side; a loader that kept one allomorph per allomorph identity reached only one prefix copy.

## What it pins

- `da`, `ba`, `ga`: parse.
- `taga` (no prohibition): two analyses, one per prefix copy.
- `tada`: prefix-first, one rule per prefix copy: no analysis.
- `taba`: stem-first, one rule per prefix copy on the rest side: no analysis.

## Oracle

hc.dll at Machine `aab4c59819adb7b366915e09df2fb28c62b8c03a`, self-check (`hc-conformance --fixtures`) PASS on signatures and traced rules. Provenance: `founding-oracle machine-commit=aab4c59819adb7b366915e09df2fb28c62b8c03a verified-date=2026-10-31`.

## FieldWorks witness

`fieldworks/project.fwdata` is backed out of `grammar.xml` via LibLCM, not hand-edited: `tools/xample-projector` `author`, extended with `--msa-copies` and `--variant-of` (extension landed in tools/xample-projector with these cases). FieldWorks 9.3.10.1452, LibLCM 11.0.0.55173.
`author --grammar grammar.xml --name AdhocAffix --msa-copies mrTa1,mrTa2   (ParserParameters gets MergeMSAs=false)`
`phonology-mutations.yaml` `base_sha256` matches the project; the `remove-g-only` case ran through `mutate` (exit 0).

## Verification

FieldWorks project through projector and hc.dll: da 1, ba 1, ga 1, taga 2, tada 0, taba 0. Control with the rules on one prefix copy only (not kept here): tada parses as TA1+DA (rule on copy 2) or TA2+DA (rule on copy 1), likewise taba.

Upstream PR: none yet (sillsdev/machine #480 candidate; PanGloss bugs #99/#100, PRs #101/#102).

PanGloss copy note: the machine #480 copy carries `fieldworks/WritingSystemStore/*.ldml` (stock CLDR), omitted here because the PanGloss import does not read them; `grammar.xml`, `words.yaml`, `fieldworks/project.fwdata` and `fieldworks/phonology-mutations.yaml` are byte-identical to the machine copy.
