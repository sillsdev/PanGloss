# grammar.environment.invalid

Invalid phonological environment

Level: **warning**

## Explanation

The environment expression could not be parsed and is ignored as a restriction.

## What to do

In Grammar > Environments, correct the expression for phonological environment 'the named item'. In Lexicon > Lexicon Edit, inspect Allomorphs > Environments. Roots ignore invalid restrictions; ordinary affixes also get an unrestricted pass. An infix still needs a valid position.

## FieldWorks places

| Tool | Field |
|---|---|
| `EnvironmentEdit` | String Representation |
| `lexiconEdit` | Allomorphs > Environments |

## Background

# Allomorphs and environments

A morpheme can have more than one form. These forms are **allomorphs**. An **environment** describes where a form can occur by naming sounds or features around it.

For example, Turkish plural *-lar* follows a stem with a back last vowel, as in *okullar* (“schools”), and *-ler* follows a stem with a front last vowel, as in *evler* (“houses”). The two forms belong to the same plural pattern; the stem's vowel helps select the form.

When a word fails, check that the needed allomorph exists and has a usable form. Then compare its environment with the sounds and features in the word. A missing form is different from an environment that is too narrow or too broad. An infix also needs a position environment.

Environment findings describe different outcomes. Invalid environment syntax may be ignored as a restriction. A reference to a missing named environment is an import error. In a circumfix, an environment can prevent one combination of prefix and suffix halves from loading while other combinations remain available.

Allomorph order matters when alternatives meet the parser's disjunctive-ordering conditions: an earlier form can block a later one. Environments, syntactic constraints, and free fluctuation also affect selection, so inspect the conditions and a parse trace before changing order.

A natural class may be used in an environment. If an explicit segment-list class names a phoneme that cannot be resolved, the parser skips that class. A feature-defined class may remain available while individual unresolved or unsupported constraints are omitted. Check the class definition and the phoneme inventory before changing an otherwise valid form.

The Turkish example illustrates one common pattern; it is not a complete account of Turkish. Real alternations may depend on additional features and exceptions.

[All diagnostic codes](../grammar-diagnostics-reference.md) · [Report format](../grammar-diagnostics.md)
