# 134: hc.dll reports a family-blocked derivation and the relative's own lookup as two analyses; PanGloss reports one

Kind: behavioural (hc.dll multiplicity; low severity).
Status: permanent until the Machine seed equality changes. Upstream: the fixture `edge-cases/family-blocked-generation`, kept in `conformance-staging/` since the suite retired it at df2b0e7 (a family is not FieldWorks-producible, cannot-say row 8; pinned row `aat` -> `[B|aat, B|aat]`); the `pangloss_expectations` row (`staging:edge-cases/family-blocked-generation`) in `oracle-conformance-known-divergences.json` excuses PanGloss's single `B|aat`. PanGloss #423 introduced the fixture.

## Decision

With stem family `famF` = {A `aa`, B `aat`} and suffix T (`t`), parsing `aat` reaches B twice in hc.dll: by plain lexical lookup, and through A+T, which `CheckBlocking` replaces by a fresh seed of the family relative B. Both end as B with no rules, but `Word.ValueEquals` sees different shapes (the fresh seed versus the looked-up word), so `Morpher.SynthesizeSequential`'s `HashSet<Word>` keeps both and the signature `B|aat` appears twice. PanGloss deduplicates analyses by `WordKey`, which treats them as equal, so it reports `B|aat` once. PanGloss keeps deduplicating on purpose; it does not emit the duplicate.

## C# site

`Word.CheckBlocking` (Word.cs:535-558); `Morpher.SynthesizeSequential` dedup through `FreezableEqualityComparer` / `Word.ValueEquals` (Word.cs:594-614).

## Rust site

`pg-rules` `WordKey` dedup (`morpher.rs` `dedup_key`).

## Verification

`conformance-staging/edge-cases/family-blocked-generation` (row `aat`), with the `pangloss_expectations` row for `aat`.
