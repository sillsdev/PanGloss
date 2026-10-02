# migration.inferred-segment-with-feature-rule

Unlisted character matches a feature class

Level: **error**

## Explanation

A character inferred from an allomorph has no authored phonological features yet matches a feature-defined natural class.

## What to do

If the character is a language phoneme, define its In Orthography as and Phonological Features in Grammar > Phonemes. Otherwise correct the unintended allomorph spelling in Lexicon > Lexicon Edit.

## FieldWorks places

| Tool | Field |
|---|---|
| `phonemeEdit` | In Orthography as |
| `phonemeEdit` | Phonological Features |
| `lexiconEdit` | Allomorphs > Form |

## Background

# Modelling a grammar the parser can use

A FieldWorks project can contain analyses that the parser cannot fully use. Check what the parser imported, then compare its behavior with words whose analyses you know. A clean project save does not show that every form, environment, feature, and rule made it into the grammar.

## Start with a word and its analysis

For a word such as *walked*, check that the lexicon contains the expected stem *walk*, that its grammatical analysis has the category needed by the affix, and that the past-tense affix has a usable form. A gloss such as “move on foot” helps people recognize the entry, but the gloss does not make a missing stem or affix parse. If the entry or analysis is absent, adding a phonological rule will not supply it.

## Sounds and spellings

The parser reads sounds from the first phoneme set. A phoneme or boundary marker without a usable spelling cannot be found in a word. If two spellings normalize to the same sequence, the parser keeps the earlier item, so check the diagnostic for both items before changing either one.

Natural classes can fail in different ways. An explicit segment-list class is skipped when one listed phoneme cannot be resolved. A feature-defined class can remain available while unresolved or unsupported constraints are omitted. Check both the inventory and the named class before changing a well-formed analysis.

## Allomorphs and environments

An allomorph needs a usable form for the role it plays. An empty prefix or suffix form cannot load as an affix rule, and an infix without a position environment cannot load through that path. A malformed environment expression may be ignored as a restriction; an unresolved named environment is an import error. In a circumfix, an environment can prevent one pairing of halves from loading without excluding every pairing.

When alternatives meet the parser's disjunctive-ordering conditions, the first listed allomorph can block a later one. Environments, syntactic constraints, and free fluctuation also affect selection. Inspect a parse trace and the allomorph conditions before reordering forms.

## Categories, slots, and features

A stem without a grammatical category may not accept affixes that require one. An unclassified affix has neither inflectional nor derivational classification and can attach too freely. An inflectional affix that belongs to no template slot is not loaded through a slot. A template is available only when it has a slot containing a loaded affix rule; an empty slot or unusable affix can leave a template with nothing to build.

In Grammar > Category Edit, inspect the category's Affix Templates and Affix Slots; in Lexicon > Lexicon Edit, check the affix's Grammatical Info. and Slots. A partial affix can still be retained when it belongs to no slot.

Feature values and inflection classes are linked by references, not by matching names. If a feature or class reference cannot be resolved, use the diagnostic to find the owner and field. Keep valid linguistic data when the parser lacks support for it, and report that limitation.

## Constructions the parser skips or approximates

Some FieldWorks constructions do not have the same behavior in the parser:

- Reduplication patterns such as `[C^1][V^1]-` are not loaded as affix rules.
- Metathesis rules are skipped; they are not run as approximations. Words that rely on a rule may fail or differ.
- Custom strata are replaced by the parser's default strata.
- Some imported metathesis rules are approximated during conversion. The snapshot compiler subsequently skips these rules too; it does not run the approximation. Read both findings to distinguish the conversion loss from the compiler omission.

Model a construction when it is part of the analysis, then check affected words and report valid behavior the parser cannot preserve.

## Measure one change

After a source change, import the project again and check the words expected to change along with a few that should remain the same. A parse trace can show which stem, allomorph, environment, and rule steps were used. Change one cause at a time so the next result is useful evidence.

[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)
