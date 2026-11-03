# grammar.limit.mpr-features

Too many inflection classes and exception features

Level: **error**

## Explanation

PanGloss supports at most 64 inflection classes (every subclass counted), exception features and entry inflection types together. This project has more, so no parser is built.

## What to do

Merge or delete unused items until the total is 64 or fewer: inflection classes and subclasses in Grammar > Category Edit > Inflection Class Info, exception features in the Exception Features list, entry inflection types in Lists > Variant Types.

## FieldWorks places

| Tool | Field |
|---|---|
| `posEdit` | Inflection Class Info |
| `variantEntryTypeEdit` |  |

## Background

# How the parser uses a FieldWorks project

The importer reads FieldWorks project data and builds a grammar the parser can run. A diagnostic identifies a source condition or a construction the parser could not represent. Read its description for the named object, field, and observed cause.

Some findings describe source data that is missing or inconsistent. Others describe a valid construction that the current parser skips, approximates, or handles with a reduced set of constraints. The next step depends on the outcome: repair a demonstrated source defect, and preserve valid linguistic data when the finding describes a parser limitation.

A FieldWorks destination is included only when the relevant tool path is known. A missing destination means the finding is grammar-wide, its source owner varies, or no verified editable field is established. Use the diagnostic description to locate the source data, then report a valid case the parser cannot preserve.

[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)
