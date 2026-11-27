# 128: Undoing a featureless `ModifyFromInput` recovers every root, where hc.dll restricts the spelling

Kind: behavioural (partial-feature grammars; see the policy in `README.md` and `docs/adr/0009-partial-feature-grammars.md`).
Status: permanent, owner decision 2026-11-05. v0.16.0 (#121, PR #187) had aligned with hc.dll; that alignment is reverted.

## Decision

A morphological rule that rewrites a vowel to a segment class with `ModifyFromInput`, in a grammar whose phonemes carry no features, is undone in analysis without restricting the node's spelling. The fixture `featureless-modify-from-input` (V = {a i}, class {i}, suffix s): PanGloss recovers root `kata` from `kitas`, root `tasi` from `tisis` and root `pasa` from `pisas`, exactly as it does for the same grammar with a phonological feature system. hc.dll recovers only `kita`; `tisis` and `pisas` have no analysis there.

Why: PanGloss serves featureless (XAmple-style) grammars, full HermitCrab grammars and everything between, and a phoneme builds features up organically. A parse must not depend on whether a phoneme has been given a feature yet. hc.dll's result here is an accident of missing features, not of the grammar, so PanGloss keeps the generous behaviour as a true, recorded divergence.

## C# site

`AnalysisMorphologicalTransform.cs` (line 34, `AddCapturedPartNodes`) adds the context's `AntiFeatureStruct()` to the node; `HermitCrabExtensions.cs` (`AntiFeatureStruct`, line 29) negates symbolic features but copies built-in ones, including `StrRep`. A segment with authored features carries no `StrRep` (`CharacterDefinitionTable.Add`), so only a featureless node ends with `StrRep` = its own spelling united with the class's, and the lexical root must unify with that set.

## Rust site

`pg-rules/src/morph.rs`, `generate_shape`: undoing a `ModifyFromInput` clears the node's `char_def` (`NO_CHAR_DEF`), so lookup falls back to lane unification and every root the lanes admit is found.

The FST half of #187 stays (`MutShape::from_shape` in `rewrite.rs` keeps a node's spelling set; `surface_probe.rs` and `preexpand.rs` render only that set). The set is `Unrestricted` for a node that came from a concrete segment, so the probe offers every matching spelling: the FST over-proposes relative to hc.dll and proposes the `kata`-rooted parses; confirmation keeps the parser's answer.

## Verification

hc.dll (machine `4d9b454d`, `hc-conformance` self-check) on the fixture's PanGloss expectations: 3 of 8 words mismatch (`kitas` got KITA only, `tisis` and `pisas` got none), pinned in `rust/tools/oracle-conformance-known-divergences.json`. Tests: `pg-parse/tests/featureless_modify_from_input.rs` (featureless and featured grammars agree). Fixture `conformance-staging/edge-cases/featureless-modify-from-input` records both answers.
