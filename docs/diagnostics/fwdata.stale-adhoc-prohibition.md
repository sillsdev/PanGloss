# fwdata.stale-adhoc-prohibition

Stale ad hoc prohibition

Level: **warning**

## Explanation

An ad hoc morpheme rule names an inflectional affix whose slot is outside every enabled template.

## What to do

In Grammar > Ad hoc Rules, inspect Key Morpheme and Other Morpheme(s); in Grammar > Category Edit, inspect the affix's Affix Templates. Restore the intended assignment if the affix should be usable.

## FieldWorks places

| Tool | Field |
|---|---|
| `AdhocCoprohibEdit` | Key Morpheme |
| `AdhocCoprohibEdit` | Other Morpheme(s) |
| `posEdit` | Affix Templates |

## Background

# How the parser uses a FieldWorks project

The importer reads FieldWorks project data and builds a grammar the parser can run. A diagnostic identifies a source condition or a construction the parser could not represent. Read its description for the named object, field, and observed cause.

Some findings describe source data that is missing or inconsistent. Others describe a valid construction that the current parser skips, approximates, or handles with a reduced set of constraints. The next step depends on the outcome: repair a demonstrated source defect, and preserve valid linguistic data when the finding describes a parser limitation.

A FieldWorks destination is included only when the relevant tool path is known. A missing destination means the finding is grammar-wide, its source owner varies, or no verified editable field is established. Use the diagnostic description to locate the source data, then report a valid case the parser cannot preserve.

[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)
