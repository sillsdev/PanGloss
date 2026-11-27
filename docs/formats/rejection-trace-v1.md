# Rejection diagnostics v1

`pangloss parse` and `pangloss batch` can write bounded diagnostics from their ordinary
HermitCrab search. Add `--trace-rejections <path.jsonl>` to publish an artifact and a short
stderr summary. The ordinary result keeps its existing format. The summary lists row statuses,
capture saturation, and frequent reasons among retained events. It occupies at most 2048 bytes
including the newline.

Run `pangloss --describe` and inspect `interfaces.diagnostic` before using the interface.
Cold HC parse and batch publish `lineage` artifacts with prefiltering off.
Legacy trace, FST, stats, cache, and stats-manifest composition are unsupported.
A traced batch with `--cost-column` writes work steps and marks the final cost field
`-`. Lineage capture changes allocation and state retention within the parser, so
the fixed-weight cost of the ordinary search is unavailable in that run.

## Run the examples

Run from the repository root with a built `pangloss` on PATH. The XML fixture is synthetic.

```sh
grammar=rust/crates/pg-cli/tests/data/trace-family.xml
scratch=$(mktemp -d)
pangloss parse "$grammar" kib --trace-rejections "$scratch/accepted.jsonl"
pangloss parse "$grammar" zzz --trace-rejections "$scratch/no-root.jsonl"
printf '%s\n' kib zzz kib > "$scratch/words"
pangloss batch "$grammar" "$scratch/words" "$scratch/ordinary.tsv" \
  --threads 1 --work-columns --trace-rejections "$scratch/saturated.jsonl" \
  --trace-max-paths 1 --trace-max-events 1 --trace-max-bytes 1024
pangloss parse "$grammar" kibes --trace-rejections "$scratch/shallow.jsonl" --trace-max-depth 1
pangloss parse "$grammar" kib --trace-rejections "$scratch/refused.jsonl" --trace-max-depth 0
```

The first file includes a completed one-match lookup and a returned analysis. The second
includes a terminal no-seed lookup rejection. The saturated file still includes a completion
for all three input rows and a footer. The shallow trace retains terminal witnesses with
at most one parent edge. The last command returns a nonzero exit with a
`PANGLOSS_DIAGNOSTIC_ERROR` line containing JSON and leaves the destination untouched.

## Requests and revision checks

Optional `--trace-options <request.json>` metadata carries an expected grammar hash and hash
semantics, expected writing system, pass ID, and request IDs keyed by zero-based source row.
Batch ordinals follow the ordinary nonempty trimmed input lines. `--start` selects a suffix
with its original ordinals. Duplicate spellings retain separate ordinals. Spelling policy is
exact and a caller making distinct passes can supply a pass ID for each invocation.

```json
{
  "schemaVersion": "pangloss.diagnostic.v1",
  "spellingPolicy": "exact",
  "passId": "original",
  "targets": [{"row": 0, "requestId": "example"}]
}
```

Writing-system checks require a single imported vernacular writing system and an exact tag
match. An unresolved expectation results in a typed refusal.

Add a `selector` to a row in `targets` to ask about an exact reading. On snapshots and
FieldWorks projects, the selector's zero-based `rootIndex` identifies a lexical root among
the ordered `morphs`. An authored occurrence carries `entryGuid` and `msaGuid`. It may
also constrain `senseGuid`, the ordered `formGuids`, and `inflTypeGuid`. A
`structural_zero` occurrence carries its attested `inflTypeGuid`. Repeated occurrences
keep separate positions. Split runs of one applied morph share a position. A sense
that shares a compiled MSA with other senses, including through `MergeMSAs`, can
leave membership unknown even when its entry and representative MSA match. The
CLI advertises the selector types it supports for authored sources. An XML
grammar still emits diagnostic records and returns `unsupported_selector` for
an authored selector.

A syntactic feature selector compares the exact set of explicitly present value GUIDs
at an authored path up to eight features deep. Supplying unverifiable or unsupported
identities, explicit part placements, or other feature domains yields a row-local
`unsupported_selector`. The same row still returns its ordinary parse results.

## Limits and records

| Capture limit | Default | Accepted values |
| --- | ---: | ---: |
| `--trace-max-paths` | 16 per row | 1–64 |
| `--trace-max-depth` | 32 edges per witness | 1–256 |
| `--trace-max-events` | 128 per row | 1–4096 |
| `--trace-max-bytes` | 16 MiB per invocation | 1024–67108864 |

Byte reserves divide equally among requested rows with leftover bytes assigned to the
earliest ordinals. Unused row capacity stays in that row. This gives a batch the same
artifact across thread counts. Deadline-driven search may visit different prefixes.
Capture counts encoded UTF-8 evidence bytes including delimiters and required mappings.
Increase `--trace-max-bytes` when large batches need more evidence per row.
An in-memory lineage arena grows as the evaluator visits states, up to one node
per 64 bytes of the row's evidence allocation. Target-aligned witnesses have
capture priority. Lower-priority witnesses can fill unused priority slots at
row completion.
The maximum published size is the evidence budget plus 4096 bytes for run controls and
3072 bytes per requested row for completion controls.

- A `header` records grammar identity and hash semantics, search options, capture limits,
  input digest, and the processed scope.
- The row can contain an `input`, typed `mapping` records, lineage `path` and `event`
  records, and ordinary accepted `output` projections. All joins are local to the row.
- Exactly one `word_end` gives search status, reasons for observed resource stops,
  ordinary output census, capture omissions, and target status.
- The `footer` gives transport status and invocation totals. A complete transport can
  report stopped searches. The destination is published atomically after the footer.

A rejected witness joins to its first evaluated terminal event and the same-row mapping
for each referenced object. Captured parent edges share ancestry across witnesses. A
cut can leave a terminal witness with an omitted parent. A merge observation joins the
discarded route to the evaluator's retained state and includes its ancestry
within the depth and byte limits. A merge that could involve the target sets
`lineage_lost` when equivalence of selector evidence is unproved. An exact match
stays known even if its output payload or witness is omitted.

An observation may describe a failed alternative, skipped optional slot, or rewrite
fallthrough while another path continues. MPR operands carry the evaluated polarity
and group. Lookup MPR operands also identify the root upper or lower bound that the
feasibility check evaluated. Environment evidence carries an end-exclusive span in the
evaluator's interior coordinates. A surface event includes the requested spelling and
built pattern the owner compared.

An `allomorph_blocked` rejection (synthesis, validity) records that an earlier-indexed
allomorph of the same morpheme was eligible at the used allomorph's span, so HermitCrab's
allomorph precedence refused the used one. The subject is the used (blocked) allomorph and
the object with role `blocker` is the earlier allomorph that won; both carry the same grammar
ids and, on a FieldWorks snapshot, form guids as every other allomorph reference. The tuple
is advertised in `--describe` under `interfaces.diagnostic.eventTuples`; a parser that does
not list it records no stop for such a path. The same reason is also recorded with stage `morphology`
when synthesis breaks at an earlier allomorph whose input pattern matched, before the allomorph the analysis
recorded was tried and so before any word exists to validate (#444): the subject is the recorded (blocked)
allomorph and the `blocker` is the earlier one that matched. The registry's reserved
`disjunctive_allomorph` tuple is the same C# failure without a named blocker and is never emitted.

Verified FieldWorks objects carry GUIDs. Compiled keys are scoped by the parser build
and grammar identity. Mappings preserve subject bindings and related identities, such
as an allomorph's owning entry or a slot's template. Authored occurrence references
resolve to objects of the corresponding kinds. Accepted paths join to the ordinary
output ordinal, including when its payload was omitted.

Target membership uses the ordinary census before capture limits. A completed census
can prove absence, while incomplete search or unresolved identity can leave membership
unknown. Causal status additionally depends on coverage and a retained attributable
terminal event. `matchCount` counts matching ordinary outputs independently of the
preview, and every `matchedOutputIds` entry joins a retained output. Path target evidence
can bind the root and uniquely identifiable applied occurrences before a
complete match exists. Invalid shape is a complete empty census with no search stop.

A required-slot failure on the analysis side can precede lexical lookup. If no
root or other proof links that history to the selector, the row can report
`membership: absent` with `status: unknown` and `owner_unobserved`. This can occur
even for an unrelated target on grammars with affix templates. Capture limits
cannot resolve it. Proof of compatibility or incompatibility for those histories
remains an open decision for Motif.

A `required_slot_uncoverable` rejection (analysis, template; the same `template_slot`
operands as `required_slot_unfilled`) replaces the `required_slot_unfilled` event of a
template when a target is requested and the target provably cannot pass through that
template: its authored non-root morphs all resolve to affix morphemes and a required
slot of the template holds no rule of any of them, so every output through the template
carries a morpheme the target lacks. The operand slot is that required slot, not the slot
the walk failed at. The event resolves the history as unrelated to the target, so it adds
no `owner_unobserved`. When every template holding a rule of the target's affixes
(and none of them standalone) is uncoverable the event also counts as an attributable
terminal, and a target with otherwise exhausted coverage reports `status: rejected`.
Rows whose target has a structural-zero or unresolved morph, no affix morph, a slot
holding a compounding rule, or any other history stay as they were. A parser that does
not list the tuple in `--describe` never emits it.

Request metadata follows the [diagnostic request schema](rejection-trace-v1/request-v1.schema.json).
Every record follows the [record schema](rejection-trace-v1/record-v1.schema.json) and the
[closed tuple registry](rejection-trace-v1/event-tuples-v1.json). The binary advertises just
the tuples and capabilities it implements. Generation's
[incumbent exact-chain request](rejection-trace-v1/generate-request-v1.schema.json) has a
separate schema.
