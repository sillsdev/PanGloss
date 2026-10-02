# Trace details v3 (including the v2 fields)

`pangloss parse <grammar> <word> --trace --trace-format=json --trace-details` writes one
`pangloss.trace-details.v3` JSON document. Use `--trace=<path>` when a file must contain only the
JSON; stdout from the managed runner may include diagnostics.

The envelope keeps the existing unmerged trace tree and category counters. It adds evidence needed
to inspect one parse offline without reopening the project:

- `provenance.parser` identifies PanGloss and the trace profile.
- `provenance.grammar` records the loaded source kind, grammar name, and the grammar hash produced
  by the existing identity computation. Snapshot hashes use `snapshot-semantic-sha256-v1`; legacy
  XML hashes use `source-bytes-sha256-v1`. `provenance.writingSystems` records snapshot project
  writing-system tags when available. `hostCapture` is reserved for a host's capture metadata and
  is separate from parser provenance.
- `search` retains completion, cap, timeout, invalid-shape, step, and parser elapsed field. No
  per-node or per-rule durations are emitted.
- `result.analyses` retains every parser result in result order. Each has a unique document-local
  `analysisId` (`analysis-0`, `analysis-1`, ...), its signature strings, a
  `fieldworks-parse-analysis/v1` projection status, and `morphs` only when the producer's
  `project_parse_analysis` succeeds. A projection error is represented by string `error` and
  diagnostic `errorCode`; an unavailable projection never removes the analysis.
- A projected morph has `identity` source IDs and quality, plus `form`, `headword`, `gloss`,
  `msa`, `slot`, `features`, `inflectionClass`, and `guessedString`. Snapshot-backed display
  values carry their writing system and source ID. MSA data retains derivational from/to
  categories, features, classes, all slots, and clitic attachment data. XML and incomplete source
  metadata leave unavailable fields null or marked `source-id-only`.
- Each trace node retains the v1 `type`, `source`, `subrule`, shape, failure reason, and children,
  then adds `sourceIdentity`, `outcome`, `attemptedMorphs`, and `failureContext`. `attemptedMorphs`
  are snapshots of that node's own input/output `Word.morphs`; they are not joined to a result
  analysis unless a future producer event publishes such an association. Circumfix and other
  multi-form records preserve every `sourceFormIds` slot.
- `failureContext` is `null` for successful nodes. For a failed node, `status` is `captured` only
  when the rejection owner published required, actual, or environment evidence through the opt-in
  trace sink. Otherwise it is `unavailable` with the typed `reasonCode`; the renderer never
  replays a gate or infers a cause from diagnostic text.

A reader must explicitly support the v3 profile before accepting it. Unknown future major
versions must be rejected clearly. A details document is self-contained: consumers must not replace its
captured labels, writing systems, or source IDs with a live project lookup.

## Family blocking (v3)

`Blocked` records lexical-family replacement, as decided by `check_blocking`. The rule's
output syntactic features subsume the selected family entry's features; the entry's seed replaces
that output. Optional `blockReason` names this mechanism and `blockedByEntry` is a typed identity
with `kind: "lexEntry"`, `id`, and `quality` (`authored`, `grammar-local`, or `synthetic`). These
fields are captured from the selected entry, without a second blocking check. `sourceIdentity`
still identifies the applied rule, and `outputShape` is the replacement's form.

The synthetic `trace-family.xml` grammar emits this event for `zodut`:

```json
{
  "type": "Blocked",
  "source": "past2",
  "outputShape": "vem",
  "blockReason": "LexicalFamilyReplacement",
  "blockedByEntry": {"kind": "lexEntry", "id": "eVem", "quality": "authored"},
  "children": []
}
```

These additions are specific to the details envelope. Ordinary trace JSON retains its fields.

## Lookup completion (v3)

Optional `lookupResult` belongs to `LexicalLookup`. The lexicon or guesser owner publishes it
after building its result vector. `mode` is `lexicon` or `guesser`; `completed` is `true`;
`matchCount` counts materialized root candidate words returned to the caller, including allomorph
expansion and guesser per-pattern deduplication. It counts neither distinct lexical entries nor
eventual successful analyses. `status` is `zeroMatches` when the count is zero, otherwise `matches`.
Absence means completion was not captured. Lookup completion is separate from `search.completed`:
later synthesis may stop early even after a particular lookup finishes.

For the synthetic exact-span `matinolu` lookup:

```json
{"type":"LexicalLookup","lookupResult":{"status":"zeroMatches","completed":true,"matchCount":0,"mode":"lexicon"}}
```

For a guesser lookup, the same fields can read
`{"status":"matches","completed":true,"matchCount":1,"mode":"guesser"}`.

## Template paths and partial candidates (v3)

Every rich node has `stepId`, such as `step-12`, from its trace arena handle. It is unique only
within that document and does not associate the node with a result analysis.

Optional `slots` on template output events records the evaluator's retained path, in slot index
order. Each item has `slotIndex`, a `slotIdentity` containing the template identity and index
(`quality: "grammar-local"`), the optional loaded slot `name`, `status`, and `selectedRule`.
An `Applied` slot names the first retained producer rule. Local equivalent-word deduplication
still occurs; this identity does not claim that no other rule could produce an equivalent word.
`OptionalSkipped` records the branch that continued without the slot. `RequiredUnfilled` records
the residual branch that could not continue without filling a required slot; it may coexist with
successful alternative branches. `NotReached` names slots not visited on that residual branch.
An empty rule batch remains optional under the existing evaluator contract. A budget exit does
not invent a slot result, and an unfilled batch does not identify a single rule-level cause.

The silent-rule path of synthetic `sipu` has these slot outcomes (identities and names omitted
here for space):

```json
{"slots":[
  {"slotIndex":0,"status":"OptionalSkipped","selectedRule":null},
  {"slotIndex":1,"status":"Applied","selectedRule":{"kind":"morphRule","id":"mrVacuous","quality":"authored"}},
  {"slotIndex":2,"status":"OptionalSkipped","selectedRule":null}
]}
```

Optional `partialParseCause` identifies the completion gate that actually emitted `PartialParse`:
`RemainingAnalyzedRules`, `RemainingRulesInStratum`, `NonFinalTemplateAppliedLast`, or
`ApplicableTemplatesNotApplied`. The latter two appear on stratum output events; the former two
on `Failed`. For example:

```json
{"type":"Failed","stepId":"step-12","failureReason":"PartialParse","partialParseCause":"RemainingAnalyzedRules"}
```

Current owners cannot establish a unique prior causal event, so `causedByStepId` is absent.
Consumers must not attribute these failures to a preceding sibling or to the final successful
rule merely because it is nearby.

## Typed rejection operands (v3)

Optional `failureContext.evidence` is a discriminated payload, captured by the rejecting owner.
The existing string `required`, `actual`, and `environment` fields remain diagnostic displays.
`evidence.failedObject` resolves the rejected allomorph, morpheme, or rule using the existing
identity computation (`kind`, `id`, `quality`). Allomorph identities optionally include
`sourceFormIds`, preserving every available FieldWorks MoForm GUID slot, including null slots.

The variants are:

| `kind` | Additional recorded fields |
| --- | --- |
| `syntacticFeatures` | `operands.required` and `operands.actual` are serialized feature structures; `operands.representation` is `grammar-local-feature-struct`. Numeric feature IDs and symbolic bitsets are grammar-local, not authored expressions. |
| `mprFeatures` | `operands.required`, `operands.excluded`, and `operands.actual` are arrays of the actual MPR feature operands. Each authored feature has `id`, `name`, and `quality`; an unresolvable local operand has `index` and `quality: "grammar-local"`. The event's reason distinguishes required, excluded, and head productivity restriction checks. |
| `coOccurrence` | `constraintOwner`, `ruleIndex`, `require`, `adjacency`, `others`, and `actual`. `others` is the ordered partner operand list; `actual` is the ordered morph sequence tested. More than one partner is retained. `require: false` is exclusion. No missing partner is fabricated. |
| `environments` | `constraintOwner`, `span`, and `alternatives`. The span has `start`, inclusive `end`, and `coordinateSystem: "interior-inclusive"` (anchors excluded). Each evaluated alternative has `index`, `require`, `accepted`, optional `sourceIdentity`, optional `authoredText`, and `sourceStatus`. |

The synthetic compound `numobel` rejects its unlicensed head with these MPR operands:

```json
{"kind":"mprFeatures","failedObject":{"kind":"morphRule","id":"cr1","quality":"authored"},
 "operands":{"required":[{"id":"mpr1","name":"M1","quality":"authored"},{"id":"mpr2","name":"M2","quality":"authored"}],
 "excluded":[],"actual":[]}}
```

A syntactic operand payload can have this shape (feature IDs and bits are illustrative):

```json
{"kind":"syntacticFeatures","failedObject":{"kind":"allomorph","id":"subX","quality":"authored"},
 "operands":{"representation":"grammar-local-feature-struct",
 "required":{"entries":[[0,{"Symbolic":2}]]},"actual":{"entries":[[0,{"Symbolic":1}]]}}}
```

For guessed roots, `failedObject` identifies the guessed sentinel and `constraintOwner` identifies
the real pattern object whose restrictions were consulted. A required co-occurrence failure
might carry:

```json
{"kind":"coOccurrence",
 "failedObject":{"kind":"morpheme","id":"ROOT","quality":"authored"},
 "constraintOwner":{"kind":"morpheme","id":"ROOT","quality":"authored"},
 "ruleIndex":0,"require":true,"adjacency":"Anywhere",
 "others":[{"kind":"morpheme","id":"SUFFIX","quality":"authored"}],
 "actual":[{"kind":"morpheme","id":"ROOT","quality":"authored"}]}
```

Snapshot compilation preserves authored environment GUIDs and representation text where it loads
an authored definition. XML can retain an explicitly authored environment ID and direct text;
it does not reconstruct an expression from compiled patterns. Synthesized concatenative and
circumfix environment combinations may lack a single authored source. Such metadata remains
unavailable. Provenance is excluded from semantic environment equality so it cannot change
allomorph free fluctuation.

An environment rejection with available snapshot provenance has this payload shape:

```json
{"kind":"environments",
 "failedObject":{"kind":"allomorph","id":"lex_entry:root#allo0","quality":"grammar-local"},
 "constraintOwner":{"kind":"allomorph","id":"lex_entry:root#allo0","quality":"grammar-local"},
 "span":{"start":0,"end":2,"coordinateSystem":"interior-inclusive"},
 "alternatives":[{"index":0,"require":true,"accepted":false,
   "sourceIdentity":{"kind":"environment","id":"environment-guid","quality":"authored"},
   "authoredText":"/_t","sourceStatus":"captured"}]}
```

`sourceStatus: "captured"` means an authored identity or text was retained; either field can
independently be null. A blank authored ID has null `sourceIdentity`, while actual authored text
can still be captured. `sourceStatus: "unavailable"` with null identity/text means no authored
source was retained. Single-source prefix/suffix environments retain their snapshot GUID and
expression; a composite circumfix environment has no single source identity. Alternatives are recorded during the same short-circuit predicate evaluation; a failure
visited every rejecting alternative, and success does not re-evaluate remaining alternatives.
Generic `Pattern` and other unsupported operand payloads retain `failureContext.status:
"unavailable"`; a generic reason is not upgraded to a guessed explanation.

## Compound and phonological analysis attempts (v3)

Compound analysis emits `CompoundingRuleAnalysis` directly from the attempt owner on both
success and rejection, preserving the attempted subrule and existing rejection code. An example:

```json
{"type":"CompoundingRuleAnalysis","source":"Compound","subrule":0,"inputShape":"fasu","failureReason":"Pattern"}
```

`Pattern` can include aggregate matching or root-filter rejection and carries no invented
specific cause. Boolean-only phonological non-unapplication retains its input and subrule and
adds optional `nonUnapplicationReason`, without a `failureReason`:

```json
{"type":"PhonologicalRuleAnalysis","subrule":0,"inputShape":"n",
 "nonUnapplicationReason":{"status":"unavailable","unavailableReason":"evaluator-returned-boolean-only"}}
```

## Historical examples

The examples below were emitted under v2 and retain their original schema labels.

The XML example [trace-details-v2-matinlu.json](examples/trace-details-v2-matinlu.json) was emitted by
PanGloss from `conformance-staging/filter-passes/exact-span/grammar.xml` with the word `matinlu`.
That grammar has structural IDs and no authored MSA GUIDs, so it intentionally shows successful parser branches
alongside explicit unavailable FieldWorks projections.

The snapshot example [trace-details-v2-snapshot.json](examples/trace-details-v2-snapshot.json) was emitted from
the reproducible [canonical snapshot input](examples/trace-details-v2-sample.snapshot.json) with the surface
`kumata`; it demonstrates available morph projection with stable IDs, writing systems, categories, slots, and glosses.
