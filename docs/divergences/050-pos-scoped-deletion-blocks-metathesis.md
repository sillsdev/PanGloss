# 050 — A part-of-speech-scoped deletion blocks metathesis analysis of other words

Kind: behavioural (shared: both engines)
Status: open — [Machine issue #520](https://github.com/sillsdev/machine/issues/520)

## Sites

- C# site: deletion unapplication in `AnalysisRewriteRule` (inserts optional nodes before the part of speech is known) and metathesis unapplication (`AnalysisMetathesisRule`), which is suspected of not matching across the inserted optional node (mechanism unconfirmed).
- Rust site: `pg-rules/src/rewrite.rs` deletion unapplication and `pg-rules/src/metathesis.rs` analysis; same observable behaviour.

## Difference from the grammar's meaning

Adding `prHDel` (h -> nothing / V _ V, `requiredPartsOfSpeech="posRedup"`) to the `Morphology` stratum of `metathesis-phase-isolation` makes `nui` (`NIU`) and `mu+i` (`MI+3SGU`) unparseable in both engines, although forward generation still produces them and the rule can never apply to those roots. Both engines agree, so this is not a C#/Rust divergence; it is a shared completeness bug recorded here because the reference grammar semantics and both engines disagree.

## Evidence

- Reproduced 2026-09-25 with the PanGloss v0.4.0 release binary: with the variant grammar, `pangloss parse` returns `-` for both words; with the committed grammar it returns `NIU|nui` and `MI+3SGU|mu+?i`. C# reproduction and minimal grammar in #520.
- The committed fixture uses `V _ a` so it does not trigger this; no fixture pins the bug yet.

## Next step

A conformance edge-case fixture with the `V _ V` variant, expected parses from forward generation, marked as an expected failure in both engines until the fix lands.
