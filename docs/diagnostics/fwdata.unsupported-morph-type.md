# fwdata.unsupported-morph-type

Unsupported morph type

Level: **warning**

## Explanation

The allomorph uses a morph type this importer cannot represent.

## What to do

In Lexicon > Lexicon Edit, check the warning's named morph type; select a supported type only if it matches the intended analysis.

## FieldWorks places

| Tool | Field |
|---|---|
| `lexiconEdit` | Morph Type |

## Background

# How the parser uses a FieldWorks project

The importer reads FieldWorks project data and builds a grammar the parser can run. A diagnostic identifies a source condition or a construction the parser could not represent. Read its description for the named object, field, and observed cause.

Some findings describe source data that is missing or inconsistent. Others describe a valid construction that the current parser skips, approximates, or handles with a reduced set of constraints. The next step depends on the outcome: repair a demonstrated source defect, and preserve valid linguistic data when the finding describes a parser limitation.

A FieldWorks destination is included only when the relevant tool path is known. A missing destination means the finding is grammar-wide, its source owner varies, or no verified editable field is established. Use the diagnostic description to locate the source data, then report a valid case the parser cannot preserve.

[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)
