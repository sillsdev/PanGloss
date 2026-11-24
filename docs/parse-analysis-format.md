# Ordered parse morphology

Consumers can compare each reading with a FieldWorks approved analysis using the selected source objects. Search completion is reported separately, so a partial finding never claims that parsing finished.

## Request

`pangloss batch grammar.fwdata words.txt out.tsv --analyses analyses.jsonl` writes the ordinary conformance TSV and an additional UTF-8 JSONL artifact from the same parser outcomes. The sidecar does not trigger another parse. Without `--analyses`, batch output is unchanged.

The sidecar cannot share a path with the grammar, word list, TSV, or statistics cache. A sidecar request with a nonzero `--start` is refused because resumption would mix evidence from different invocations. Failed or cancelled invocations may leave partial files; consumers must require successful process termination and exactly one row per requested case before accepting them.

## Row contract

Every nonblank input case receives one row, in input order, including repeated surface strings:

```json
{"schema":"fieldworks-parse-analysis/v1","index":0,"word":"example","elapsedMs":12,"capped":false,"timedOut":false,"invalidShape":false,"analyses":[{"morphs":[{"form":"11111111-1111-1111-1111-111111111111","msa":"22222222-2222-2222-2222-222222222222","inflType":null,"guessedString":null}]}],"unavailable":[]}
```

All fields are required. Consumers reject unknown members, duplicate JSON properties, unknown schemas, invalid GUIDs, missing cases, and mismatched indices or words.

The row schema is [fieldworks-parse-analysis-v1.schema.json](fieldworks-parse-analysis-v1.schema.json). Duplicate properties and cross-row case alignment require validation beyond JSON Schema.

| Field | Meaning |
| --- | --- |
| `index`, `word` | Zero-based batch case and exact input text. An index identifies a request occurrence, not a FieldWorks wordform. |
| `elapsedMs` | Nonnegative elapsed parsing time for the case, identical to its TSV timing. |
| `capped` | The per-word work cap interrupted the search. |
| `timedOut` | The per-word time budget interrupted the search. |
| `invalidShape` | Input could not be segmented. Both limit flags are false and both result arrays are empty. |
| `analyses` | Successfully projected readings, retaining multiplicity. Morph order is significant. |
| `unavailable` | One diagnostic string per reading that could not be projected authoritatively. Such a reading is not silently counted as no analysis. |

Both limit flags may be true. Findings already obtained remain in the arrays, and later cases continue independently. A consumer must display **INCOMPLETE — parsing did not finish** for each interrupted case, even if every approved expectation is already matched.

## Morph identity

Each analysis contains an ordered `morphs` array. `form` is the selected source MoForm GUID; `msa` is the source MoMorphSynAnalysis GUID; `inflType` is the optional source LexEntryInflType GUID. GUIDs are nonempty lowercase hyphenated text. Neither dense compiler ordinals nor owner identities substitute for the selected form.

This shape follows FieldWorks `ParseAnalysis` / `ParseMorph` and the morphology comparison used by `ParseAnalysis.MatchesIWfiAnalysis`: ordered Form/MSA/InflType references, with conditional `guessedString` comparison against a bundle's literal writing-system forms. Sense and word-level category are excluded. An absent inflection type means absence, never a wildcard.

Projection follows `HCParser.GetMorphs`: omit synthetic null-affixes, retain circumfix halves, suppress repeated ordinary annotations for one morpheme, and place infixes before the preceding output morph. Source metadata is preserved during `.fwdata` compilation. XML-authored IDs do not establish FieldWorks provenance, even when GUID-shaped, so direct XML inputs remain explicitly unavailable in this profile.

Runtime fabricated roots without authored Form/MSA identities are unavailable. Guessed text cannot create or replace those identities. The wire supports conditional guessed text when authoritative source references exist; it does not promise that every parser result can currently be projected.

## Morph spans (opt-in)

`pangloss batch ... --analyses analyses.jsonl --morph-spans` adds `morphSpans` to every reading in `analyses`, beside `morphs`. Without the flag the field is absent and rows are unchanged; the flag is refused without `--analyses` and with `--stats`, `--partials`, `--engine fst`, `--prefilter` or a diagnostic run. A consumer that rejects unknown members must accept `morphSpans` before it passes the flag.

```json
{"morphs":[...],"morphSpans":{"surface":"mentanukam","segmentCount":10,"morphemes":[{"item":0,"spans":[{"segments":[0,3],"offsets":[0,3],"text":"men"}],"reason":null,"rule":null}],"unattributed":[]}}
```

The spans come from the synthesized word that verified the reading, through the same function as `serve generate` (`morphSpans` in `docs/formats/serve-protocol.md`, "Morpheme spans"), so shape, units and reasons are the same.

| Field | Meaning |
| --- | --- |
| `surface` | The surface in NFD and lower case, which can differ from `word`; `offsets` index it. Read `text` and do not slice `word`. |
| `segmentCount` | Segments of the surface, as `match` counts them. |
| `morphemes` | One entry per element of the reading's `morphs`, in order; `item` is that index. |
| `morphemes[].spans` | Runs `{segments, offsets, text}`: half-open segment indexes, half-open UTF-8 byte offsets into `surface`, and `surface[offsets]`. An infix or circumfix gives several runs; an empty list means no segment is left. `null` when no sound span exists. |
| `morphemes[].reason`, `rule` | Set exactly when `spans` is `null`: `span_lost_by_rule` (with `rule`), `span_unknown_truncation`, `span_ambiguous_copy`, `nested_morph`, `compound_unsupported`, `repeated_morpheme`, `morph_not_recorded`, `segments_not_aligned`. |
| `unattributed` | Runs `{segments, offsets, text, reason, rule}` no morph claims (`inserted_by_rule` for epenthesis, `no_morph_owner`, `unrequested_morpheme`, or the reason of a null morph whose segments are in doubt). With the morph runs they tile `[0, segmentCount)`. |
| `text` | `null` when `offsets` is `null`. |

`morphSpans` is `null` for a reading with no span answer. A wrong span is worse than none, so a null is always preferred to a guess:

- The two halves of a circumfix (one morpheme, two projected morphs with the same morpheme id) each get their own run, e.g. `kabutangan` is `ka|butang|an`.
- `repeated_morpheme` applies only where the records cannot be told apart: a morpheme the reading applied more than twice, or a morph whose records belong to another morpheme. A morpheme applied twice with one form is folded into one morph in `morphs`, and its span is the union of its runs.
- A null affix FieldWorks omits from `morphs` has no item; any segment it owns is `unrequested_morpheme`.
- A word with a truncation, an ambiguous reduplication copy or a compound has every morph null with the reason above.
- Readings under `unavailable` have no spans.

## Comparison and provenance

Approved-analysis correctness is positive subset matching: every approved morphology must be found; extra parser readings do not constitute failure. Exact parser conformance instead compares the complete multiset. Neither comparison may infer complete search from the number of findings.

The TSV's legacy signature is a separate, lossy projection. Machine's conformance harness still uses that TSV; this sidecar does not imply that Machine has adopted the richer wire profile. Structured identity checks are necessary to distinguish same-owner allomorphs and inflection types that the TSV cannot distinguish.

The row does not invent source/model fingerprints. Motif retains source bytes, input words, TSV, sidecar, stderr, executable identity, and effective limits as one invocation, hashes the artifacts, and freezes approved expectations from that retained source. Consumers must retain equivalent provenance for reproducible comparisons.
