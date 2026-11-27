# grammar.stored-analysis.extra-reading

Parse has no stored analysis

Level: **info**

## Explanation

PanGloss parses this wordform to a Reading that none of its approved FieldWorks analyses matches, so the parser offers a Reading the approved set does not contain. It is a fact about the grammar, not an error: the approved set may be incomplete, or the grammar may overgenerate. The same words that carry an extra Reading are listed per word, with their missing and matched counts, under stored_analysis_words. An extra can also come from a recorded Rust-versus-C# parser divergence (for example the metathesis entries 127, 132 and 133 in docs/divergences); it is reported anyway. Guessed roots are not reported.

## What to do

Compare the named morphs with the wordform's approved analyses in Lexicon > Lexicon Edit. If the Reading is correct, approve it. If it is wrong, find the allomorph, environment or rule that lets it parse and narrow it.

## FieldWorks places

| Tool | Field |
|---|---|
| `lexiconEdit` | Analysis |

## Background

# How the parser uses a FieldWorks project

The importer reads FieldWorks project data and builds a grammar the parser can run. A diagnostic identifies a source condition or a construction the parser could not represent. Read its description for the named object, field, and observed cause.

Some findings describe source data that is missing or inconsistent. Others describe a valid construction that the current parser skips, approximates, or handles with a reduced set of constraints. The next step depends on the outcome: repair a demonstrated source defect, and preserve valid linguistic data when the finding describes a parser limitation.

A FieldWorks destination is included only when the relevant tool path is known. A missing destination means the finding is grammar-wide, its source owner varies, or no verified editable field is established. Use the diagnostic description to locate the source data, then report a valid case the parser cannot preserve.

[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)
