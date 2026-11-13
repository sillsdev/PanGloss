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
caller supplies Chapter Number or Verse Number styles, LibLCM WordMaker treats ALL characters
in those styled runs as non-word-forming, including letters, configured word-forming additions,
and numeric characters. The style exclusion applies to the entire word-forming predicate, not
just its numeric rule. LibLCM is the parity reference for this behavior. Plain text does not
acquire invented style information.

A character can be both word-forming and punctuation without a contradiction. Under owner rule
Q6′, a dual-role character such as `’` (U+2019) or a hyphen yields segmentation alternatives with
and without it as word material. Keep the FieldWorks-rule split as primary and offer the other
readings alongside it; this does not change the primary mode's LibLCM parity contract.

An occurrence counts as analysed when its primary split or ANY segmentation alternative is
analysed. Misses of the other alternatives are ignored: they contribute no failure, misspelling,
or no-analysis count and produce no red flag. ADR 0012 applies this rule to per-part analysis,
statistics, and returned occurrence outcomes.

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

- Extend `rust/dotnet-harness/` (`dotnet-harness`) to run LibLCM WordMaker as the native differential
  parity reference. The suite compares exact word slices and boundaries, recording the LibLCM
  revision, segmentation profile, and Unicode-data context. Offset units must be explicit when
  comparing Rust spans with C# UTF-16 ranges. This is a required parity seam, not a claim that
  the existing FFI harness already exercises WordMaker.
- Cover configured punctuation and apostrophe/hyphen variants, Unicode defaults, combining marks,
  supplementary characters, numeric characters, U+200B boundaries, whitespace, and mixed
  writing-system runs. Chapter Number and Verse Number cases cover letters and configured
  word-forming additions as well as numbers, all excluded. Missing reference evidence is not a
  parity pass.
- Run SIL.Machine's Latin tokenizer over comparison cases and document expected differences.
  Agreement with Machine cannot excuse disagreement with LibLCM in this mode. This segmentation
  reference does not replace C# HermitCrab as the founding oracle for morphological parsing.
- Multiple Dictionary packs and Language Packs must be possible without treating one writing system
  or one Latin heuristic as proof of language membership. ADR 0012 records the segmentation
  alternatives, per-part language candidates, and ordering alongside the primary FieldWorks-rule
  split; these do not change the LibLCM parity contract for the primary segmentation.

Decision basis: PanGloss-analyzer owner answers P1/P2, the general Q6′ alternative-success rule,
and the Spec seams parity-harness decision in `corpus-flow-decisions.md` (2026-11-07).
Source research: `tokenizer-pangloss-rust.md`, which inspected LibLCM `d564a719`, FieldWorks
`936ae91fe`, and Machine `18cf242f`; its WordMaker references are
`src/SIL.LCModel/DomainServices/ITextUtils.cs` and the writing-system definitions under
`src/SIL.LCModel.Core/`.
The styled-run exclusion was checked directly in LibLCM `d564a719`:
`WordMaker.IsWordforming` in `src/SIL.LCModel/DomainServices/ITextUtils.cs` applies the label
exclusion before both ordinary word-forming classification and numeric acceptance.

Which tokens are then counted as words is [ADR 0016](0016-not-a-word-tokens.md); it runs after this segmentation and never changes it.

Related: [widened purpose](0010-multi-language-spelling-prediction-and-analysis.md) and
[multi-language word evidence](0012-multi-language-word-evidence.md). Terms: [CONTEXT.md](../../CONTEXT.md).
