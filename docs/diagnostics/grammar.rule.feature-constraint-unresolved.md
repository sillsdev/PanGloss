# grammar.rule.feature-constraint-unresolved

Unresolved rule feature constraint

Level: **error**

## Explanation

The rule refers to a feature-constraint object that cannot be resolved.

## What to do

In Grammar > Phonological Rules, inspect the named rule part. Correct a stale constraint reference; if FieldWorks displays a valid constraint, report the rule and missing-object details.

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
