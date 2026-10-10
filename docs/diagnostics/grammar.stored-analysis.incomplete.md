# grammar.stored-analysis.incomplete

Stored analysis not checked

Level: **warning**

## Explanation

PanGloss could not finish comparing a wordform's FieldWorks stored analyses with its own analyses, usually because analyzing the wordform reached the parser's work cap. The description names the wordform and the reason. Whether those stored analyses still parse is unknown; the other wordforms were still compared.

## What to do

No FieldWorks correction is established: the stored analyses may still parse. If the wordform should analyze quickly, look for rules or affixes that can apply to it many times, such as optional phonological rules or null affixes. If the grammar is correct, report the wordform with its description and PanGloss version.

## Background

# How the parser uses a FieldWorks project

The importer reads FieldWorks project data and builds a grammar the parser can run. A diagnostic identifies a source condition or a construction the parser could not represent. Read its description for the named object, field, and observed cause.

Some findings describe source data that is missing or inconsistent. Others describe a valid construction that the current parser skips, approximates, or handles with a reduced set of constraints. The next step depends on the outcome: repair a demonstrated source defect, and preserve valid linguistic data when the finding describes a parser limitation.

A FieldWorks destination is included only when the relevant tool path is known. A missing destination means the finding is grammar-wide, its source owner varies, or no verified editable field is established. Use the diagnostic description to locate the source data, then report a valid case the parser cannot preserve.

[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)
