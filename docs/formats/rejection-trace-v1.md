# Rejection diagnostics v1

`pangloss parse` and `pangloss batch` can write bounded diagnostics from their ordinary
HermitCrab search. Add `--trace-rejections <path.jsonl>` to publish an artifact and a short
stderr summary. The ordinary result keeps its existing format. The summary lists row statuses,
capture saturation, and frequent reasons among retained events. It occupies at most 2048 bytes
including the newline.

Run `pangloss --describe` and inspect `interfaces.diagnostic` before using the interface.
The current mode is `observations` for cold HC parse and batch. Prefiltering must be off.
Legacy trace, FST, stats, cache, and stats-manifest composition are unsupported. Depth is
reserved for lineage mode. An explicit `--trace-max-depth` returns a typed refusal.

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
pangloss parse "$grammar" kib --trace-rejections "$scratch/refused.jsonl" --trace-max-depth 32
```

The first file includes a completed one-match lookup and a returned analysis. The second
includes a terminal no-seed lookup rejection. The saturated file still includes a completion
for all three input rows and a footer. The last command returns a nonzero exit with a
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
match. An unresolved expectation results in a typed refusal. A syntactically valid target
selector receives a row-local `unsupported_selector` status while ordinary parsing continues.

## Limits and records

| Capture limit | Default | Accepted values |
| --- | ---: | ---: |
| `--trace-max-paths` | 16 per row | 1–64 |
| `--trace-max-events` | 128 per row | 1–4096 |
| `--trace-max-bytes` | 16 MiB per invocation | 1024–67108864 |

Byte reserves divide equally among requested rows with leftover bytes assigned to the
earliest ordinals. Unused row capacity stays in that row. This gives a batch the same
artifact across thread counts. Deadline-driven search may visit different prefixes.
Capture counts encoded UTF-8 evidence bytes including delimiters and required mappings.
The maximum published size is the evidence budget plus 4096 bytes for run controls and
3072 bytes per requested row for completion controls.

- A `header` records grammar identity and hash semantics, search options, capture limits,
  input digest, and the processed scope.
- The row can contain an `input`, typed `mapping` records, local `path` and `event`
  records, and ordinary accepted `output` projections. All joins are local to the row.
- Exactly one `word_end` gives search status, reasons for observed resource stops,
  ordinary output census, capture omissions, and target status.
- The `footer` gives transport status and invocation totals. A complete transport can
  report stopped searches. The destination is published atomically after the footer.

A rejected witness has one terminal owner and the same-row mapping for each referenced
object. Local paths have no parent or depth. A lookup observation can explain a failed
alternative while a sibling goes on to produce an output. MPR operands state whether the
actual set is the root upper or lower bound evaluated by the replay. A surface event includes
the requested spelling and built pattern compared by the owner, plus mapped candidates from
that synthesis. Distinct local witnesses do not by themselves establish shared ancestry.

Verified FieldWorks objects carry GUIDs. A compiled key is scoped to the parser build and
grammar identity. Its mapping can contain authored sources and separate related links,
such as an allomorph's owning entry. A related link cannot provide the object's own GUID.
Accepted output projections are partial and retain the ordinary output ordinal, available
authored or scoped morphology and the root index when it can be joined.

Request metadata follows the [diagnostic request schema](rejection-trace-v1/request-v1.schema.json).
Every record follows the [record schema](rejection-trace-v1/record-v1.schema.json) and the
[closed tuple registry](rejection-trace-v1/event-tuples-v1.json). The binary advertises just
the tuples and capabilities it implements. Generation's
[incumbent exact-chain request](rejection-trace-v1/generate-request-v1.schema.json) has a
separate schema.
