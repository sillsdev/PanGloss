# grammar.rule.rule-feature-unresolved

Unresolved phonological rule feature

Level: **error**

## Explanation

A phonological rule feature reference cannot be resolved.

## What to do

In Grammar > Phonological Rules, inspect Required Properties and Excluded Properties on the named rule. Reselect an intended existing property if the reference is stale; if FieldWorks shows a valid selection, report the loading failure.

## FieldWorks places

| Tool | Field |
|---|---|
| `PhonologicalRuleEdit` |  |

## Background

# How the parser uses a FieldWorks project

The importer reads FieldWorks project data and builds a grammar the parser can run. A diagnostic identifies a source condition or a construction the parser could not represent. Read its description for the named object, field, and observed cause.

Some findings describe source data that is missing or inconsistent. Others describe a valid construction that the current parser skips, approximates, or handles with a reduced set of constraints. The next step depends on the outcome: repair a demonstrated source defect, and preserve valid linguistic data when the finding describes a parser limitation.

A FieldWorks destination is included only when the relevant tool path is known. A missing destination means the finding is grammar-wide, its source owner varies, or no verified editable field is established. Use the diagnostic description to locate the source data, then report a valid case the parser cannot preserve.

[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)
