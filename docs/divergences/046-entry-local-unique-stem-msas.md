# 046 — Entry-local stem MSA deduplication from FieldWorks

## Kind
Behavioural (loader-side). Duplicate stem MSAs formerly produced multiple lexical
entries with distinct MSA identities. This ports the first-representative selection
in [FieldWorks PR #1148](https://github.com/sillsdev/FieldWorks/pull/1148).

## Status
Open, waiting on FieldWorks PR #1148, which was an open draft on 2026-09-24. Until
it merges, shipping FieldWorks still builds one HermitCrab entry per duplicate stem
MSA, and this port makes PanGloss diverge from it. Merge this port only after the
FieldWorks PR merges, and follow any change to its selection rule first.

## C# site
FieldWorks `Src/LexText/ParserCore/HCLoader.cs`, `UniqueStemMSAs` and its two
`LoadLexEntries` call sites, PR head
`fd517004b31558c6c7fd13967a030b421986a127`.

The equality contract is `MoStemMsa.EqualsMsa(IMoMorphSynAnalysis)` in liblcm
`src/SIL.LCModel/DomainImpl/OverridesLing_MoClasses.cs`, inspected at
`d564a719b1cce16c25ebea53a537393cb757f5d1`. It compares part of speech,
inflection class, equivalent morphosyntactic features, from-parts-of-speech,
and production restrictions. GUIDs, sense glosses, and slots are not compared.
`DomainObjectServices.AreEquivalent` treats null and an empty feature structure
as equivalent. Feature specifications and reference collections are unordered.

## Rust site
`rust/crates/pg-grammar/src/compile/lexicon.rs`: ordinary stem entry construction
and entry-linked variant stem selection. Snapshot extraction retains all authored
MSAs. Sense-linked variants retain the explicitly referenced MSA; affix-rule
construction retains its existing iteration. Equivalent MSAs in different lexical
entries are never combined.

## Can it change a parse?
Yes, intentionally: only the first equivalent stem MSA's lexical identity and gloss
survive the entry-local selection. Distinct grammatical analyses must survive.
This is not a claim that before/after parse-identity multisets are identical, nor
a port of affix-rule deduplication suggested by the PR's broader motivation.

## Which duplicate survives
The first in the entry's MSA collection order. The kept MSA supplies the entry's
gloss (the first sense or subsense that uses it) and the MSA identity in parse
output, so senses that use a dropped duplicate disappear from parse output. Aweti
`kãᴷ` keeps the MSA used by subsense `tb.smoked` and drops the one used by sense
`to.dry`; Sena `amyali` keeps `girl` and drops `virgin`. When no sense or subsense
uses the kept MSA, FieldWorks falls back to the placeholder gloss `ksQuestions`;
none of the five sample projects has that shape. This port mirrors the selection
rule rather than improving on it.

## Evidence
`pg-grammar/src/compile/tests/unique_stem_msas.rs` (10 loader tests) and
`pg-grammar/src/compile/lexicon/unique_stem_tests.rs` (4 equality tests). With the
deduplication disabled, 6 of the 14 fail; the other 8 pin boundaries that hold
either way (distinct MSAs survive, affix MSAs untouched, sense-linked variants).

Real projects, compiled with and without the deduplication:

| Project | Duplicate stem MSAs | Compiled entries | Morphological rules |
| --- | --- | --- | --- |
| Aweti | 1 (`kãᴷ`) | 855 → 854 | 136 → 136 |
| Sena | 1 (`amyali`) | 1,384 → 1,383 | 85 → 85 |
| Amharic, Indonesian, Mbugwe | 0 | unchanged by construction | unchanged |

Duplicates were counted from the `.fwdata` XML independently of PanGloss. No
exported conformance grammar or live patched FieldWorks HCLoader run is claimed; an
HC XML fixture cannot exercise this snapshot-to-grammar selection.

## Boundaries and remaining evidence
The snapshot feature model carries closed and nested complex values, but not all
LCM feature-structure metadata such as TypeRA and feature disjunctions. Equality
in this port must be described within that representable snapshot model, not as
complete equivalence over every LCM object.

The upstream patch registers only retained MSAs in its morpheme dictionary and
returns without loading a prohibition when a referenced MSA is absent. PanGloss
already refuses an active prohibition whose other MSA reference cannot resolve.
This port must not weaken that guard or invent aliases for discarded identities.
Projects that refer to a discarded MSA may therefore encounter that existing
refusal; `unique_stem_missing_duplicate_in_active_prohibition_still_refuses` pins it.

## Upstream
The implementation source is FieldWorks PR #1148, linked above. This is a FieldWorks loader adaptation, not a change to
Machine's HC XML parser, so no Machine issue applies. The PR deduplicates stem MSAs
only: the Aweti reduplication entry `ll` keeps 9 derivational affix MSAs (6 of them
duplicates) and so 9 rules, in FieldWorks and here alike.
