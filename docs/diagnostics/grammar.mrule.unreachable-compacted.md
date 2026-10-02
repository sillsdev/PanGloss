# grammar.mrule.unreachable-compacted

Unreachable affix omitted

Level: **info**

## Explanation

An affix rule unreachable from the compiled grammar was compacted.

## What to do

No change is needed if the affix is intentionally unused. Otherwise inspect its category's Affix Templates in Grammar > Category Edit and its Grammatical Info. in Lexicon > Lexicon Edit.

## FieldWorks places

| Tool | Field |
|---|---|
| `posEdit` | Affix Templates |
| `lexiconEdit` | Grammatical Info. |

## Background

# Reading why a word fails

A parse result answers whether the parser found an analysis within the search it completed. **Parsed** means it found one. **No parse** with a completed search means it found no analysis. A timeout or other search limit means the search stopped early; it does not show that no analysis exists.

For a completed no-parse result, compare the word with its expected analysis. Check spelling and the phoneme inventory first. Then check the stem, grammatical category, affix form, environment, and rules that should build the rest of the word. A missing gloss alone does not prevent a parse, while a missing stem or required category can.

Read the parse trace to see the furthest step reached and which alternatives were tried. A rule-order issue and allomorph disjunctive order are separate mechanisms. Reordering helps only when the trace and the relevant ordering conditions point to it.

Read the diagnostic for a missing or unsupported allomorph, natural class, feature, or rule before editing valid source data. An unresolved member in an explicit segment-list class can cause the whole class to be skipped; unresolved feature constraints can be omitted while the rest of a feature-defined class remains. A metathesis rule that the compiler marks unsupported is skipped.

If the search stopped at a limit, rerun with suitable resources before drawing conclusions from the partial trace. If it completed without a parse, use the trace and diagnostics to test one likely cause at a time.

[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)
