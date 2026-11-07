---
status: accepted
---

# Return multi-language word evidence alongside the vernacular parse

Owner decision, 2026-11-07, including the addition that contact-language correction suggestions are
in scope. PanGloss loads contact-language packs using Hunspell dictionary/affix data through a
pure-Rust crate, supplemented by small word lists. Automatic language detection proposes languages;
per-occurrence language candidates accompany the vernacular parse without becoming a linguistic
or human spelling verdict.

## Decision

Keep parser analysis, dictionary acceptance, word-list evidence, language detection, and correction
suggestions distinguishable and attributed to their source and version. Several languages can be
candidates for the same occurrence. A valid vernacular parse remains visible when a contact pack
also recognizes the form; a contact-language hit does not remove the occurrence from parsing or
silently exclude it from grammar-coverage evidence.

Contact-language checking includes Hunspell correction suggestions such as `teh` -> `the`.
Distinguish these observations:

| Evidence | Meaning |
|---|---|
| Valid `<language>` word | The original occurrence has a valid analysis or spelling acceptance under the identified language resource. |
| Misspelled `<language>` word | A language candidate carrying a misspelling flag and correction suggestions, such as an English candidate for `teh` suggesting `the`. |
| Unknown word | Completed checks supply neither valid-word evidence nor a misspelled language-word candidate from the resources checked. |

Misspelled language words count separately from valid words in statistics. Suggesting `the` does
not count `teh` as a valid English word and does not replace the source text. These are evidence
states, not a forced partition into exclusive languages: an occurrence may be valid in one
language while carrying a misspelling candidate in another. A dictionary miss or failed vernacular
parse alone establishes neither a misspelling nor a nonword. Unknown describes the available
evidence, not universal invalidity; unavailable, incomplete, or unattempted checks remain explicit.
People and callers retain the decision about intended language, spelling, and any replacement.

Small word lists provide limited membership, frequency, or detection evidence; absence from a list
is not spelling rejection. Hunspell acceptance is dictionary/affix evidence and does not fabricate
a HermitCrab structured analysis identity. Pack availability, coverage, source, licence, and
compatibility with the Rust engine must be explicit. Spellbook is a researched candidate, not a
dependency selected by this ADR. The detector, list source, and model are also not selected here.

## Open questions

- **Segmentation alternatives.** The owner wants both `l'homme` and `l'` + `homme` sent to the
  parser, and both joined and split alternatives for rare hyphenated forms. A hyphen can join
  material from different languages. How to represent, organize, prioritize, and combine these
  alternatives with pack-specific segmentation remains OPEN; this ADR selects no alternative
  segmentation algorithm and does not change the LibLCM compatibility mode.
- **Statistics and reporting granularity.** The owner wants language code plus a count of words
  with a valid analysis in that language, with misspelled language-word counts separate. The full
  statistic set, counting rules for overlapping candidates and segmentation alternatives, and
  reporting granularity (word, sentence, paragraph, or text) remain OPEN. A per-sentence language
  summary is not selected. Detection predictions and suggested corrections must not inflate
  valid-analysis counts.

Decision basis: the final two sections of `corpus-flow-decisions.md` (2026-11-07) and the owner's
subsequent correction-suggestions addition. Research: `hunspell-pangloss.md`,
`corpus-langid-small.md`, and `r-lang-id.md`. Their warnings about ambiguous forms, short-input
detection, limited list coverage, and separate data licences inform this decision; earlier
recommendations to keep PanGloss parser-only or put the component in managed C# are superseded.

Related: [widened purpose](0010-multi-language-spelling-prediction-and-analysis.md) and
[FieldWorks-compatible segmentation](0011-fieldworks-compatible-segmentation.md).
Terms: [CONTEXT.md](../../CONTEXT.md).
