# 125: PanGloss applies the stored `maxApps` of an exocentric compound rule; HCLoader ignores it

Kind: behavioural (loader only).
Status: permanent (accepted-divergence). Decided 2026-11-03: PanGloss follows the setting the user stored.

## What differs

FieldWorks stores a maximum-applications value for every compound rule: `HCMaxCompoundRulesDlg.cs:63` lists all `IMoCompoundRule`, endocentric and exocentric, and the value reaches `HCLoader` through the `<CompoundRule maxApps>` settings read at `HCLoader.cs:122-123`.

| Rule kind | C# `HCLoader` | PanGloss |
|---|---|---|
| Endocentric | `LoadEndoCompoundingRule` looks the value up and sets `MaxApplicationCount` (`HCLoader.cs:1943-1945`) | applies it |
| Exocentric | `LoadExoCompoundingRule` (`HCLoader.cs:1971-2050`) never reads it; the rule keeps the default of 1 | applies it |

The HC XML path honours it for both kinds: machine `XmlLanguageLoader.TryLoadCompoundingRule` (`XmlLanguageLoader.cs:1155-1158`, machine `74ee09bf`) reads `multipleApplication` for every `CompoundingRule` element, and `XmlLanguageWriter.WriteCompoundingRule` (`XmlLanguageWriter.cs:803-804`) writes it. So HCLoader's exocentric path is the odd one out, not the engine.

A project whose exocentric rule has a stored value above 1 parses compounds that stack that rule more than once in PanGloss; FieldWorks's own parser stops at one application. A rule left at the default parses identically.

## Why

The user set the number in the dialog that lists the rule. Reproducing the C# result would silently discard it, and the engine and the XML loader already support it.

## Rust site

`pg-grammar/src/compile/compounding.rs`: one lookup of `compound_rule_max_applications` (default 1) passed to both `build_endo` and `build_exo`.

## Coverage and verification

Source reading only. No local grammar has an exocentric rule (ceb, swh, tam: none; amharic and mbugwe have one endocentric rule each) and no test pins the exocentric value.
