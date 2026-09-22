# Trace details v2

`pangloss parse <grammar> <word> --trace --trace-format=json --trace-details` writes one
`pangloss.trace-details.v2` JSON document. Use `--trace=<path>` when a file must contain only the
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

A v1 reader may ignore the v2 additions only if it has an explicit v2 adapter. Unknown future major
versions must be rejected clearly. A v2 document is self-contained: consumers must not replace its
captured labels, writing systems, or source IDs with a live project lookup.

The XML example [trace-details-v2-matinlu.json](examples/trace-details-v2-matinlu.json) was emitted by
PanGloss from `conformance-staging/filter-passes/exact-span/grammar.xml` with the word `matinlu`.
That grammar has structural IDs and no authored MSA GUIDs, so it intentionally shows successful parser branches
alongside explicit unavailable FieldWorks projections.

The snapshot example [trace-details-v2-snapshot.json](examples/trace-details-v2-snapshot.json) was emitted from
the reproducible [canonical snapshot input](examples/trace-details-v2-sample.snapshot.json) with the surface
`kumata`; it demonstrates available morph projection with stable IDs, writing systems, categories, slots, and glosses.
