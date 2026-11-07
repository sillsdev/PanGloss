---
status: accepted
---

# Return multi-language word evidence alongside the vernacular parse

Owner decisions, 2026-11-07, including contact-language correction suggestions and analyzer round 2
(T1–T4), which settles the previously open segmentation and reporting choices. PanGloss loads
contact-language packs using Hunspell dictionary/affix data through a pure-Rust crate, supplemented
by small word lists. Automatic language detection proposes languages;
per-occurrence language candidates accompany the vernacular parse without becoming a linguistic
or human spelling verdict.

## Decision

Use **Wordform** for a written form and **Occurrence** for one instance in source text, rather than
calling either a token. **Word-forming character**, **Segmentation profile**, **Segmentation
alternative**, **Language pack**, and **Language candidate** have their definitions in `CONTEXT.md`.
Language pack is the general term; the existing PanGloss Language Pack remains specifically the
`.pgpack` grammar analysis artifact. No common pack container is selected here.

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

## Segmentation and ordering

The primary segmentation is the FieldWorks-rule split from the writing system's segmentation
profile. Also retain segmentation alternatives at word-forming apostrophes and hyphens and from
partial language-pack matches. Thus both `l'homme` and `l'` + `homme`, and joined and split readings
of rare hyphenated forms, are considered. A hyphen can join parts from different languages.

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

## Core statistics and queryable results

For each language code, report both the number of occurrences and the number of wordforms with
a valid analysis in that language. Also report misspelled-`<language>`, ambiguous, and no-analysis
counts, keeping occurrence and wordform counts distinct. Valid-word evidence can be a grammar
analysis or language-pack spelling acceptance, with the evidence kind retained; dictionary
acceptance does not become a morphological parse. Detection predictions or suggested corrections
do not count as valid analyses of the original form. Count occurrences and wordforms, not the
number of parser results or segmentation paths that support them.

Language ambiguity retains the competing language candidates. No-analysis reports the absence of
a valid analysis among completed checks, while retaining any misspelling evidence separately.
These statistics describe evidence rather than a forced exclusive language partition or a
human spelling judgement. Incomplete and unattempted checks remain explicit.

Return statistics per text and per corpus by default, or for a caller-supplied selection. Finer
granularity, including sentence or paragraph computations, and further statistics are available
on request rather than imposed as default summaries.

Return the core per-occurrence analysis in a form the host can store and query without rerunning
Hunspell. It includes the original wordform and source location, primary and alternative part
relationships, per-part language candidates, valid-language evidence identified as grammar analysis
or pack spelling acceptance, misspelling evidence and suggestions, ambiguity/no-analysis and
completion states, and the identities of the resources used. Preserve enough source context or
references for queries such as which wordforms were
ambiguous, which were English-only, and which sentences contain occurrences of a wordform.
Storage and query infrastructure belong to the host, such as Motif; this decision selects no
PanGloss database, persistence format, or storage policy.

Decision basis: the owner-direction and PanGloss-analyzer rounds 1 and 2 in
`corpus-flow-decisions.md` (2026-11-07), including the correction-suggestions addition and T1–T4.
Research: `hunspell-pangloss.md`,
`corpus-langid-small.md`, and `r-lang-id.md`. Their warnings about ambiguous forms, short-input
detection, limited list coverage, and separate data licences inform this decision; earlier
recommendations to keep PanGloss parser-only or put the component in managed C# are superseded.

Related: [widened purpose](0010-multi-language-spelling-prediction-and-analysis.md) and
[FieldWorks-compatible segmentation](0011-fieldworks-compatible-segmentation.md).
Terms: [CONTEXT.md](../../CONTEXT.md).
