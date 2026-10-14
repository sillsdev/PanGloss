# Suspected upstream Hermit Crab issues

This file holds suspected bugs where PanGloss and C# Hermit Crab (the engine in FieldWorks and
`sillsdev/machine`) agree, so they are not divergences and have no entry in `docs/divergences/`. Each
entry is documented for the owner, who decides whether to file it or whether the suspicion is wrong.
Nothing here is filed or posted without the owner. PanGloss behaviour is never changed to "fix" one.

## A required template slot with no affixes is skipped

**Summary.** A template slot authored as required, but holding no affixes, is dropped when the template
is built, so the slot behaves as optional. A bare stem then parses as a complete word even though its
required prefix or final vowel is missing.

**Behaviour.**
- FieldWorks `Src/LexText/ParserCore/HCLoader.cs` lines 310-312 build each enabled template's slots
  with `.Where(s => s.Affixes.Any(msa => m_morphemes.ContainsKey(msa)))` and add the template only if
  `slots.Length > 0`. A slot with no loaded affix never reaches the engine, whatever its required flag.
- Machine `src/SIL.Machine.Morphology.HermitCrab/AffixTemplateSlot.cs`: `Optional` returns `true`
  whenever the slot has zero rules, even after `Optional` was set to `false`. A zero-rule slot that does
  reach the engine is skipped for the same reason.
- PanGloss agrees on the outcome. The source files were inspected at FieldWorks commit
  `bd0608da7f46a7ba084d93fabca61a99dae5f0da` and Machine commit `18cf242f4b114b0eb9bac304b4b171ca2f499a39`.

**Witnesses (Swahili research project, `swh`).**
- `tu`: the final noun template `T-N` has required prefix slot `S-NC` (noun class, no affixes) and
  optional suffix slot `S-LOC`. Bare `tu` parses completely as "person" and "thing" (PanGloss 0.8.4),
  although the noun-class prefix is required. For C# the result is derived from the source above; no
  C# runtime replay of `tu` was captured.
- `nisem`: the verb template has 14 loaded subject-prefix rules but an empty required final-vowel slot
  `FV`. Truncated `nisem`, `asem` and `usem` parse with exact subject-prefix plus stem readings, missing
  the required final vowel. The report was not replayed natively; the C# result is again source-derived.
- Filling all 15 required slots with authored affixes removes every false positive: `tu`, `nisem`,
  `asem` and `usem` no longer parse.

**Why it may be a bug.** The author marked the slot obligatory and the grammar says so, yet an empty slot
silently erases the obligation. The slot looks enforced and is not, and the word is accepted because the
author has not finished the grammar, which is when a warning would help most.

**Why it may be intended.** FieldWorks routinely has templates and slots that no affix has joined yet,
and skipping them keeps a half-built grammar parsing instead of rejecting every word. Loader and engine
both implement the skip on purpose, in two places.

**Mitigation.** Motif's grammar check warns with `grammar.required-slot-empty` (Motif commit
`b5831ff8d`). Research fills each required slot with real or explicit zero affixes.

**Evidence and held change.** The evidence is in the Motif harness briefs under
`_briefs/harness-2026-10-13/pg-m25/t22/`: `held-branch-report.md`, `acceptance-plan.md`, `reproduce.py`,
`current-failure.log`, `real-witness-0064/` and `upstream-source-evidence/`. A PanGloss change that kept
`optional: false` for empty slots was written, held and **not merged**; it would have diverged from
FieldWorks. It is preserved as `held-divergence.diff` (SHA-256
`fbcb127b72a6a4f40867959c91bfb7ca008d1795cdc2dd4304766079f208f3d2`) in that folder.

**Status: documented for the owner; not filed.**
