---
status: accepted
---

# Partial-feature grammars: PanGloss keeps the generous behaviour where hc.dll's comes from missing features

Owner decision, 2026-11-05. PanGloss deliberately covers a continuum:

- **XAmple-style featureless grammars**: phonemes are spelled letters with no phonological features at all.
- **Partial-feature grammars**: some phonemes carry some features, others none, so a feature may be defined for one series and absent from another. This is the normal path from the first to the third.
- **Full HermitCrab feature grammars**: every phoneme fully specified.

A phoneme builds up features organically, one at a time, and every point on the way must work without a complete feature set. HermitCrab (hc.dll) was written for the last kind of grammar. Where its behaviour on a partial or featureless grammar is an artifact of a feature that is missing (a built-in spelling that survives only for featureless segments, a subsumption test that fails when a segment lacks a value), PanGloss does not reproduce it. It keeps the generous behaviour that a grammar with the features would show, and records the difference as a true divergence.

## Consequences

- A parse must not change because an author adds a feature to a phoneme that already behaved as the feature would say. Missing features are compatible with a condition (unification), never a failure.
- Each such behaviour is its own ledger entry under `docs/divergences/` and a staged fixture whose `words.yaml` records PanGloss's answers (`# oracle-provenance: forward-synthesis`), with hc.dll's mismatch pinned in `rust/tools/oracle-conformance-known-divergences.json`. Current entries: 128 (undoing a featureless `ModifyFromInput` recovers every root) and 129 (a segment with no value for a feature satisfies `[feature:-]` in synthesis targets and environments).
- This is not a license to improve on hc.dll elsewhere. A difference that does not come from a missing feature remains a parity bug under the oracle hierarchy in `CLAUDE.md`. A fully featured grammar still parses exactly as hc.dll does.
- The FST must never under-propose relative to the parser; over-proposal is fine, because confirmation uses the parser.
- v0.16.0 briefly aligned with hc.dll on both behaviours (#121, #180); 0.16.x reverts them.
- The conformance suite (`sillsdev/fieldworks-parser-conformance`) records hc.dll's answers; fixtures that exercise partial features are marked there as known PanGloss divergences.

Related: `docs/adr/0008-provisional-definitions.md` (underdefined projects), `docs/divergences/README.md` ("Partial-feature grammars").
