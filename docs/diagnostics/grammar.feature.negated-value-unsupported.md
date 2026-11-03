# grammar.feature.negated-value-unsupported

Negated feature value

Level: **error**

## Explanation

A feature structure holds a negated value. The parser cannot represent it and FieldWorks' own HermitCrab parser fails to load the project too, so no parser is built.

## What to do

Find the feature structure named in the finding and remove the negated value, or rewrite it as a positive value: in Grammar > Category Edit > Inflection Features, in Lexicon > Lexicon Edit under Grammatical Info, or in the rule's feature fields.

## FieldWorks places

| Tool | Field |
|---|---|
| `posEdit` | Inflection Features |
| `lexiconEdit` | Grammatical Info |

## Background

# How the parser uses a FieldWorks project

The importer reads FieldWorks project data and builds a grammar the parser can run. A diagnostic identifies a source condition or a construction the parser could not represent. Read its description for the named object, field, and observed cause.

Some findings describe source data that is missing or inconsistent. Others describe a valid construction that the current parser skips, approximates, or handles with a reduced set of constraints. The next step depends on the outcome: repair a demonstrated source defect, and preserve valid linguistic data when the finding describes a parser limitation.

A FieldWorks destination is included only when the relevant tool path is known. A missing destination means the finding is grammar-wide, its source owner varies, or no verified editable field is established. Use the diagnostic description to locate the source data, then report a valid case the parser cannot preserve.

[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)
