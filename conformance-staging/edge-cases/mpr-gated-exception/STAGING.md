# STAGING: mpr-gated-exception

## Why this fixture exists

Mimics two of the **Indonesian-shaped** pathologies named in `docs/conformance-staging-plan.md`'s
pathology catalog:

1. **Placeholder-nasal assimilation + junction deletion (deletion-junction model).** `mrNPfx` inserts
   a placeholder nasal segment before the root; `prNasalAssimBilabial`/`prNasalAssimAlveolar` resolve
   it to a real place-matching nasal depending on the root-initial consonant; `prObstruentDeletion`
   then deletes a following VOICELESS obstruent — the cascade behind real Indonesian
   `meN+tulis -> menulis` (voiceless obstruent deletes) vs. `meN+bawa -> membawa` (voiced obstruent
   survives), with invented roots (`tulik`/`balo`) standing in for the real corpus lexicon.
2. **An MPR-gated rule exception (`excludedMPRFeatures`) where a corpus word's correct parse
   REQUIRES the exception to be honored** — the P6 flag-diacritics recall case named in the plan
   doc ("Indonesian's own corpus happens not to exercise it; the mimic must"). `mrSuf`'s
   `MorphologicalInput` carries `excludedMPRFeatures="mprException"`; `eVokad` carries that feature
   (`ruleFeatures="mprException"`), so `mrSuf` must be blocked for it, forcing its only correct
   derivation through the unrelated, ungated `mrSufAlt`.

## What it pins

- `menulik` / `membalo`: the assimilation-then-selective-deletion cascade produces the right surface
  AND the right morpheme chain for both the deleting (voiceless-initial) and non-deleting
  (voiced-initial) roots — the contrast is the actual pin, not either row alone.
- `vokadan` (VOKAD + `mrSuf`) has **no valid derivation** — `expect_fail: true`. This is the
  load-bearing assertion: an engine that ignores `excludedMPRFeatures` would wrongly accept it.
- `vokadi` (VOKAD + `mrSufAlt`) is VOKAD's actual correct derived form — the positive control that
  proves the exception doesn't just block everything, it correctly routes to the one rule that
  remains available.
- `sanitan` (a non-excluded root through the same `mrSuf`) is the control proving `mrSuf` itself
  works normally absent the MPR feature.

## A finding this fixture required: full feature differentiation is load-bearing, not just style

An early draft gave every vowel (a/e/i/u/o) an identical feature bundle (all `featPlace=vocalic`,
nothing else distinguishing them) and let `l`/`k`/`s`/`v`/`d` all share `featPlace=fPlaceOther`. This
loaded and ran, but `pg_parse`'s own shape-rendering correctly treats feature-identical segments as
indistinguishable, collapsing them into a bracketed alternates class in the signature (e.g.
`t[aeiuo][lvd][aeiuo][ks]` instead of the literal `tulik`) — confirmed empirically (see
"Verification"). This obscured the actual assimilation/deletion pin without being wrong per se. Fixed
by adding `featHigh`/`featBack` (fully distinguishing the 5 vowels) and splitting `featPlace` further
for the consonants, matching every other fixture in this suite's own "G2a" full-specification-AND-
full-differentiation convention (see any `machine/conformance` grammar's own segment-table comment).
Every signature below is now literal, as intended.

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Authored fresh for
this task; `words.yaml` signatures captured by driving `pg_parse::Morpher::parse_word` directly (a
throwaway in-repo test — see "Verification" below). Per `docs/conformance-staging-plan.md`'s
oracle-discipline note, machine acceptance must re-verify against the C# founding oracle before
graduation.

## Verification

Signatures were captured via a throwaway test driving `pg_parse::Morpher::parse_word` directly over
every word in `words.yaml` (equivalent to `pangloss batch grammar.xml words.txt out.tsv`'s signature
column, without needing a release build of the `pg-cli` binary — a from-scratch release build in
this task's environment took over 30 minutes under heavy concurrent load and was abandoned in favor
of a debug-profile `pg-parse` test driving the same engine). Output transcribed into `words.yaml`
above. Cross-checked in-repo by `rust/crates/pg-parse/tests/conformance_fixtures_gate.rs`'s
`all_discovered_fixtures_match_oracle` test (dual-root discovery, default `cargo test --workspace`
suite) — that test is what actually gates CI; the throwaway dump test was deleted after transcription.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/mpr-gated-exception/`. On acceptance, delete this staged copy in the
same change (graduation guard enforces this mechanically).

## Coverage-tag addition: `LeftToRightRewrite` (post-G9)

`CharacteristicKind::LeftToRightRewrite` (`Disposition::Proven`) had NO fixture tagging it at all
before this change, anywhere in the suite -- left-to-right is the pervasive default rewrite direction
(`Dir::LeftToRight` whenever a `PhonologicalRule` omits `multipleApplicationOrder="rightToLeftIterative"`),
so it is easy to exercise by accident and easy to forget to tag on purpose. `menulik`'s parse above now
carries `exercises: ["RewriteRule direction (Dir): left-to-right"]` (`constructs.txt` row 29): both
`prNasalAssimAlveolar` and `prObstruentDeletion` genuinely have no `multipleApplicationOrder` attribute
(confirmed against this directory's own `grammar.xml`, not assumed), and both rules actually FIRE to
produce this word's surface form -- not merely present-but-unused. No signature, `parses:`, or ground
truth changed; this is a tag addition only.
