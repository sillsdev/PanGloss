---
status: accepted
---

# Port LibLCM WordMaker as PanGloss's FieldWorks-compatible segmentation mode

Owner decision, 2026-11-07. PanGloss ports FieldWorks/LCModel WordMaker's writing-system-configured
word-boundary rule as its FieldWorks-compatible segmentation mode. LibLCM is the parity reference:
differences in this mode block release. SIL.Machine's Latin tokenizer is a documented comparison
reference in the parity suite, not the compatibility contract.

## Decision

PanGloss reads writing-system `.ldml` directly and also reads a JSON segmentation profile recording
the source `.ldml` identity and hash. Both inputs describe the same writing-system rules; the JSON
representation does not introduce an independent set of boundary semantics.

The mode returns maximal adjacent runs of word-forming characters. It follows LibLCM's Unicode
defaults (letters, nonspacing and combining-spacing marks, and modifier symbols), plus configured
word-forming additions. Apostrophes and hyphens join runs only when that writing system configures
them as word-forming. WordMaker's numeric rule also makes ICU numeric characters word-forming in
ordinary text, even when not configured, so `16th` and numeric-only runs remain words. Where the
caller supplies Chapter Number or Verse Number styles, preserve WordMaker's suppression of that
numeric rule. Plain text does not acquire invented style information.

Text already segmented in a FieldWorks project keeps FieldWorks' segmentation. This decision does
not replace LibLCM inside FieldWorks or ask a caller to resegment its stored words. For new text,
preserve the original surface and source spans; parser-internal normalization must not rewrite the
source or its locations.

## Why

Machine's Latin tokenizer uses its own punctuation and abbreviation heuristics. Its default
apostrophe/hyphen treatment and punctuation tokens differ from writing-system-configured maximal
runs. Porting that tokenizer as the contract would change established FieldWorks word boundaries.
WordMaker is a compatibility mode, not a universal multilingual segmenter: a continuous run in a
script without spaces remains a run rather than receiving inferred lexical boundaries.

## Consequences

- The differential parity suite compares exact word slices and boundaries with LibLCM WordMaker,
  recording the LibLCM revision, segmentation profile, and Unicode-data context. Offset units must
  be explicit when comparing Rust spans with C# UTF-16 ranges.
- Cover configured punctuation and apostrophe/hyphen variants, Unicode defaults, combining marks,
  supplementary characters, numeric characters and style exceptions, U+200B boundaries, whitespace,
  and mixed writing-system runs. Missing reference evidence is not a parity pass.
- Run SIL.Machine's Latin tokenizer over comparison cases and document expected differences.
  Agreement with Machine cannot excuse disagreement with LibLCM in this mode. This segmentation
  reference does not replace C# HermitCrab as the founding oracle for morphological parsing.
- Multiple contact-language packs must be possible without treating one writing system or one
  Latin heuristic as proof of language membership. ADR 0012 records the segmentation alternatives,
  per-part language candidates, and ordering alongside the primary FieldWorks-rule split; these
  do not change the LibLCM parity contract for the primary segmentation.

Decision basis: PanGloss-analyzer owner answers P1/P2 in `corpus-flow-decisions.md` (2026-11-07).
Source research: `tokenizer-pangloss-rust.md`, which inspected LibLCM `d564a719`, FieldWorks
`936ae91fe`, and Machine `18cf242f`; its WordMaker references are
`src/SIL.LCModel/DomainServices/ITextUtils.cs` and the writing-system definitions under
`src/SIL.LCModel.Core/`.

Related: [widened purpose](0010-multi-language-spelling-prediction-and-analysis.md) and
[multi-language word evidence](0012-multi-language-word-evidence.md). Terms: [CONTEXT.md](../../CONTEXT.md).
