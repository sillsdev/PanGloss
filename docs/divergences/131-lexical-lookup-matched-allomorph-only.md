# 131: Lexical lookup proposes only the matched root allomorph, as machine#627 does and hc.dll on master does not

Kind: behavioural until sillsdev/machine#627 merges (one parse can differ from hc.dll on master); then none, and the entry is retired.
Status: temporary; follows an upstream pull request that is approved and not yet merged.
Issues: sillsdev/PanGloss#263, sillsdev/machine#627.

## Decision

hc.dll on master turns every root allomorph that analysis matched into synthesis candidates for all allomorphs of the matched entry. machine#627 (head `d26dd375`) proposes only the matched allomorph, and skips a matched allomorph whose entry's syntactic feature structure does not unify with the input's. PanGloss ports both changes now. The first changes output: a word whose only route to a parse is a sibling allomorph that analysis did not match no longer parses. With a deletion rule `i -> 0 / [+high -cons] _` at Deletion Reapplications 0, an entry with allomorphs `buii` and `bui / _ [C]` and the word `bu`, analysis reaches only `bui`, whose environment fails in synthesis; master parses `bu` through `buii`. The second change only skips synthesis attempts that could not succeed.

## C# site

`Morpher.LexicalLookup` (`Morpher.cs`): master loops `foreach allomorph in entry.Allomorphs` over the distinct matched entries; #627 loops over the matched `RootAllomorph`s, applies `LexEntrySelector`, then `entry.SyntacticFeatureStruct.IsUnifiable(input.SyntacticFeatureStruct)`.

## Rust site

`pg-parse/src/morpher.rs::Morpher::lexical_lookup_filtered`. The matched allomorphs keep the order the root search returns and are deduplicated as C#'s `Distinct()` does. A skipped allomorph under a tracing sink emits the `SynthesizeWord` trace and then a `Failed` trace with `RequiredSyntacticFeatureStruct`, as #627 does. HC-Rust does not emit `SynthesizeWord` for the allomorphs it does propose, a difference from C# that predates this entry and is unchanged.

## Why the filter cannot change a parse

A matched allomorph whose entry's syntactic features do not unify with the input's cannot reach a valid word: the root's features are unified with the word's in synthesis and the word is rejected. Skipping it earlier removes only attempts that fail.

## Verification

Fixture `edge-cases/lexical-lookup-sibling-allomorphs` in fieldworks-parser-conformance (suite commit `1feb48c`), rows taken from hc.dll built from #627. Master fails its word `bu` until #627 merges; the C# conformance driver in preland reports that single row. `pg-parse/tests/lexical_lookup_matched_allomorph.rs` pins the behaviour in-process and fails with the port reverted.

## Retirement

When #627 merges and PanGloss's `machine` pin includes it, delete this entry.
