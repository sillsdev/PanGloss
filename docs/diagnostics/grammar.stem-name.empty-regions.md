# grammar.stem-name.empty-regions

Stem allomorph label has no feature sets

Level: **warning**

## Explanation

The stem allomorph label has no non-empty inflection feature set.

## What to do

In Grammar > Category Edit, inspect Stem Allomorph Labels > Feature Sets and supply an intended Feature Set if the label should constrain an allomorph. Otherwise remove an unintended label assignment in Lexicon > Lexicon Edit.

## FieldWorks places

| Tool | Field |
|---|---|
| `posEdit` | Stem Allomorph Labels > Feature Sets |

## Background

# Stems and the lexicon

The lexicon stores words and the grammatical information the parser uses to build analyses. An entry can have a stem, a gloss, a grammatical category, and one or more allomorphs. These pieces serve different purposes.

For an illustrative English example, *walk* is a stem, “move on foot” is a gloss, and *verb* is its category. The gloss helps a person recognize the entry; it does not supply a missing stem or category. A form such as *walked* needs the stem and a usable past-tense affix with the required grammatical links.

When a word fails, check that the expected stem exists in the relevant writing system. Then check its category and grammatical analysis. A stem without a category may not combine with affixes that require one. A category by itself cannot replace a missing stem.

For an affix, check that its analysis is classified as intended and that the needed allomorph has a usable form. Inflectional affixes are loaded through template slots. An inflectional affix outside every slot has no slot through which the template can load it, but a partial rule can still be retained; check the finding against the intended template restrictions. An unclassified affix, with neither inflectional nor derivational classification, is a different case and can attach too freely.

If the source analysis looks complete but the parser still cannot use it, read the diagnostic for the exact missing reference or unsupported construction. Preserve a valid analysis and report a parser limitation instead of guessing a replacement.

The examples are illustrative. Real entries and analyses depend on the language and the choices made in its FieldWorks project.

[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)
