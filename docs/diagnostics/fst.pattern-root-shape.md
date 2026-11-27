# fst.pattern-root-shape

Pattern root has no FST route

Level: **warning**

## Explanation

The lexical pattern root is retained in the Grammar but its complete spelling set has no supported FST route.

## What to do

Use the full morphological parser for this grammar. If the pattern is intentional, report the named root and PanGloss version; no FieldWorks correction is established.

Label: **blocks FST**

Present in the converted Grammar because: The root retains a pattern shape that this FST route cannot represent completely.

## Background

# How the parser uses a FieldWorks project

The importer reads FieldWorks project data and builds a grammar the parser can run. A diagnostic identifies a source condition or a construction the parser could not represent. Read its description for the named object, field, and observed cause.

Some findings describe source data that is missing or inconsistent. Others describe a valid construction that the current parser skips, approximates, or handles with a reduced set of constraints. The next step depends on the outcome: repair a demonstrated source defect, and preserve valid linguistic data when the finding describes a parser limitation.

A FieldWorks destination is included only when the relevant tool path is known. A missing destination means the finding is grammar-wide, its source owner varies, or no verified editable field is established. Use the diagnostic description to locate the source data, then report a valid case the parser cannot preserve.

[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)
