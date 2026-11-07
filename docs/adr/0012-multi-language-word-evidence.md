---
status: accepted
---

# Return multi-language word evidence alongside the vernacular parse

Owner decisions, 2026-11-07, including contact-language correction suggestions and analyzer round 2
(T1–T4), which settles the segmentation and reporting choices, and T9, which settles pack naming
and evidence sources. PanGloss loads Dictionary packs using Hunspell dictionary/affix data through
a pure-Rust crate, supplemented by small word lists and an optional n-gram profile. Automatic
language detection proposes languages; per-occurrence language candidates accompany the vernacular
parse without becoming a linguistic or human spelling verdict.

## Decision

Use **Wordform** for a written form and **Occurrence** for one instance in source text, rather than
calling either a token. **Word-forming character**, **Segmentation profile**, **Segmentation
alternative**, **Dictionary pack**, **PanGloss Language Pack**, and **Language candidate** have their
definitions in `CONTEXT.md`. A Dictionary pack is the spelling and language-evidence bundle:
Hunspell `.dic`/`.aff` data, a word list, and an optional n-gram profile. Language Pack retains only
its `.pgpack` grammar analysis meaning; there is no Language Pack umbrella term.

A contact language can also be analyzed by an additional PanGloss Language Pack loaded alongside
the vernacular one, for example a supplied Tagalog/Filipino grammar. Language candidates can draw
evidence from Dictionary packs or Language Packs. Dictionary acceptance and suggestions remain
distinct from the morphological analyses a Language Pack returns, with each source identified.
These names select neither a Dictionary pack container nor a deployment or loading policy.

Keep parser analysis, dictionary acceptance, word-list evidence, language detection, and correction
suggestions distinguishable and attributed to their source and version. Several languages can be
candidates for the same occurrence. A valid vernacular parse remains visible when a Dictionary
pack or another Language Pack also recognizes the form; a contact-language hit does not remove the
occurrence from parsing or silently exclude it from grammar-coverage evidence.

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
parse alone establishes neither a misspelling nor a nonword. Misses of competing segmentation
alternatives do not become misspelling candidates when another alternative is analysed, under
the Q6′ rule below. Unknown describes the available evidence, not universal invalidity;
unavailable, incomplete, or unattempted checks remain explicit.
People and callers retain the decision about intended language, spelling, and any replacement.

Small word lists provide limited membership, frequency, or detection evidence; absence from a list
is not spelling rejection. Hunspell acceptance is dictionary/affix evidence and does not fabricate
a HermitCrab structured analysis identity. Pack availability, coverage, source, licence, and
compatibility with the Rust engine must be explicit. Spellbook is a researched candidate, not a
dependency selected by this ADR. The detector, list source, and model are also not selected here.

## Segmentation and ordering

The primary segmentation is the FieldWorks-rule split from the writing system's segmentation
profile. Also retain segmentation alternatives at word-forming apostrophes and hyphens and from
partial matches in Dictionary packs or Language Packs. Thus both `l'homme` and `l'` + `homme`, and
joined and split readings of rare hyphenated forms, are considered. A hyphen can join parts from
different languages.

A character that can be both word-forming and punctuation, such as `’` or a hyphen, is not a
configuration contradiction. Produce alternatives with and without it as word material, while
retaining the primary FieldWorks-rule split.

Send every segmentation alternative to the parser as well as the primary split. Retain language
candidates for each part, together with that part's parse and pack evidence. A partial pack match
does not suppress the unmatched part or bypass parsing of an alternative.

Order results as follows:

1. The primary FieldWorks-rule split.
2. Alternatives where every part has a valid analysis.
3. The remaining alternatives.

This ordering prioritizes the returned readings; it does not discard alternatives. Keep their
source spans and part relationships so a host can show the competing readings. The primary
segmentation's LibLCM parity contract remains unchanged.

Under owner rule Q6′, an occurrence is analysed if its primary split or ANY alternative is
analysed. A multipart reading, primary or alternative, is analysed when every part has a valid
analysis; a partial match alone does not establish analysis of the whole occurrence. Once one reading is analysed,
misses of the other alternatives are ignored, not counted as failures, misspellings, or
no-analysis, and not flagged. A missed primary split does not override an analysed alternative,
and a missed alternative does not override an analysed primary split.

For example, if the reading without a surrounding `’` is analysed and the reading retaining it
misses, the occurrence is analysed without a failure or misspelling flag from the missed reading.
The same rule applies when a joined hyphenated form is analysed but a split alternative misses,
or vice versa when all parts of the split alternative are analysed.

## Core statistics and queryable results

For each language code, report both the number of occurrences and the number of wordforms with
a valid analysis in that language. Also report misspelled-`<language>`, ambiguous, and no-analysis
counts, keeping occurrence and wordform counts distinct. Valid-word evidence can be a Language Pack
grammar analysis or Dictionary pack spelling acceptance, with the evidence kind retained; dictionary
acceptance does not become a morphological parse. Detection predictions or suggested corrections
do not count as valid analyses of the original form. Count occurrences and wordforms, not the
number of parser results or segmentation paths that support them.

Apply Q6′ before aggregating occurrence and wordform statistics or deriving red flags at any
reporting granularity. Count an occurrence as analysed if any reading is analysed, count it only
once per language supported by its valid analyses, and aggregate wordforms from those occurrence
outcomes. Misses from the other alternatives contribute no failures, misspelled-language counts,
no-analysis counts, or flags. They do not manufacture language ambiguity; competing positive
language evidence remains available.

Language ambiguity retains the competing language candidates. No-analysis requires that neither
the primary split nor any alternative is fully analysed after completed checks; a missed
alternative alone cannot establish it. Retain misspelling evidence separately only when it is
not a suppressed miss of another segmentation alternative after a successful reading.
These statistics describe evidence rather than a forced exclusive language partition or a
human spelling judgement. Incomplete and unattempted checks remain explicit.

Return statistics per text and per corpus by default, or for a caller-supplied selection. Finer
granularity, including sentence or paragraph computations, and further statistics are available
on request rather than imposed as default summaries.

Return the core per-occurrence analysis in a form the host can store and query without rerunning
Hunspell. It includes the original wordform and source location, primary and alternative part
relationships, per-part language candidates, valid-language evidence identified as Language Pack
grammar analysis or Dictionary pack spelling acceptance, misspelling evidence and suggestions,
ambiguity/no-analysis and completion states, and the identities of the resources used. Preserve
enough source context or references for queries such as which wordforms were ambiguous, which were
English-only, and which sentences contain occurrences of a wordform.
The returned occurrence outcome and flags follow Q6′ so that querying stored results cannot
reintroduce ignored alternative misses as errors or counts.
Storage and query infrastructure belong to the host, such as Motif; this decision selects no
PanGloss database, persistence format, or storage policy.

Decision basis: the owner-direction and PanGloss-analyzer rounds 1 and 2, round 3 T9, and Q6′ in
`corpus-flow-decisions.md` (2026-11-07), including the correction-suggestions addition and T1–T4.
Research: `hunspell-pangloss.md`,
`corpus-langid-small.md`, and `r-lang-id.md`. Their warnings about ambiguous forms, short-input
detection, limited list coverage, and separate data licences inform this decision; earlier
recommendations to keep PanGloss parser-only or put the component in managed C# are superseded.

Related: [widened purpose](0010-multi-language-spelling-prediction-and-analysis.md) and
[FieldWorks-compatible segmentation](0011-fieldworks-compatible-segmentation.md).
Terms: [CONTEXT.md](../../CONTEXT.md).
