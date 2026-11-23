# `pangloss serve` protocol (version 2)

`pangloss serve` keeps one compiled project resident and answers parse requests over stdin and stdout, so a host pays the project load and grammar compile once instead of on every word. Measured on a 22,237-entry project: load and compile cost 7-9 s per process, the parse itself 70-90 ms.

```
pangloss serve --project <path.fwdata|snapshot.json> [--work-cap N|unbounded] [--word-mem-cap-mib N] [--word-timeout-ms N] [--guess[=true|false]] [--timing]
```

`--project` accepts whatever `pangloss parse` accepts as `<grammar>`. The cap, timeout and guess flags mean what they mean for `parse` and apply to every request. Unknown options are refused, including the removed `--step-cap`.

Version 2 removes the `steps` field from every `result` response and from the trace-details `search` object, and removes `--step-cap`. A client written for version 1 must refuse a server that reports 2, because it would read a missing `steps` as zero.

Root guessing follows the project: a FieldWorks project guesses unless its `<HC><GuessRoots>` is false, and a HermitCrab XML grammar never guesses. `--guess` (or `--guess=true`) forces it on and `--guess=false` forces it off.

A host can gate on this mode: `pangloss --describe` carries `"interfaces": {"serveProtocol": 2}` and lists a `serve` command.

## Streams

- stdout carries protocol lines only: one JSON object per line, flushed after every line.
- stderr carries logs and grammar warnings, in the same text `parse` prints.
- Requests are processed one at a time, in order. Each request yields exactly one response, in request order.

## Startup

After loading and compiling, one line:

```json
{"type":"ready","protocol":2,"loadMs":8421}
```

`loadMs` is wall-clock load plus compile. With `--timing` the line also carries `"timing":{"importMs":N,"compileMs":N}` (`importMs` is the fwdata import, or the snapshot read for `.json`).

If startup fails, one line and a non-zero exit:

```json
{"type":"fatal","code":"load_failed","message":"..."}
```

Codes: `bad_arguments` (command line refused), `load_failed` (import or compile failed).

## Requests

One JSON object per line. Blank lines are ignored.

```json
{"id":"1","op":"parse","word":"bakhanhun","traceDetails":true}
{"id":"2","op":"shutdown"}
```

- `id` is required and must be a string; it is echoed in the response.
- `parse`: `word` is a non-empty string. `traceDetails` is optional (default `false`).
- `shutdown`: the process answers `bye`, then exits 0.
- End of input on stdin exits 0 at once (see Closing stdin). Cancellation is the host killing the process.

## Responses

```json
{"id":"1","type":"result","exitCode":0,"status":"ok","workSteps":901,"guessed":false,"result":{...}}
{"id":"1","type":"error","code":"bad_request","message":"..."}
{"id":"2","type":"bye"}
```

A request that ran a parse always yields a `result`, including a parse that stopped at a cap or timeout. `error` is for requests that never ran.

`result` carries what the equivalent cold command printed to stdout, and `exitCode` is the cold command's exit code. The cold argv is the command line `<project>` and the server's own cap, timeout and `--guess` flags, so each mode equals exactly one cold invocation:

| `traceDetails` | cold argv | `result` | `exitCode` |
|---|---|---|---|
| `true` | `pangloss parse <project> <word> --trace --trace-format json --trace-details` | the JSON document the command prints (the trace-details envelope, see `trace-details-v2.md`), as a JSON value | always 0 (the command exits 0 even when capped or timed out; read `search.capped` / `search.timedOut` in the document) |
| `false` (default) | `pangloss parse <project> <word> --trace --trace-format json` | the command's whole stdout as one JSON string: the line `<word>\t<signature>\n`, then `guessed:\t<true\|false>\n` only when the server runs with `--guess` or `--guess=true` (never because the project guesses), then the JSON trace tree (absent when no trace exists) | 1 when the parse was incomplete (capped, timed out, invalid shape), else 0; the string is still the full stdout |

For `traceDetails: true`, every field is identical to the cold document except the wall-clock measurements: `elapsedNs`, `timedNs`, `timingOverrunNs`, `unattributedNs` and every `*ElapsedNs` field. For `traceDetails: false` the string is byte-identical.

Every `result` response, in both modes, also carries the word's search counts beside `result`, outside the cold stdout:

- `status`: `ok`, `CAP` when the work cap stopped the parse, `MEMCAP` when the per-word parser-state ceiling (100 MiB by default) or the affix-matching capture-state ceiling (256 MiB by default) stopped it, `TIMEOUT` when the
  time limit stopped it, or `SKIPPED` for an invalid shape. This optional field is absent from
  older servers.
- `workSteps`: the word's work, meaning the total of work steps the parse reserved. This is the quantity `--work-cap` bounds.

This is the same count as the `work_steps` column that `pangloss batch --work-columns` writes (see `batch-tsv.md`) and, for `traceDetails: true`, as `search.workSteps` in the document. They are deterministic for a given project, word, cap settings and PanGloss version, so compare parse cost with `workSteps`, not with wall-clock time. A parse stopped by `--word-timeout-ms` is the exception: its counts depend on this computer's speed. Treat the field as optional: older servers omit it.

Every `result` response also carries `guessed`: `true` when the analyses were guessed from a lexical pattern rather than found in the lexicon, whatever the `--guess` flag. A project that guesses by default can return guessed analyses without the `guessed:` line, so read this field, not the string. Treat it as optional: older servers omit it.

With `--timing`, a `result` response also carries `"timing":{"parseMs":N,"renderMs":N}`.

### Error codes

The process keeps running after every `error`.

| code | meaning |
|---|---|
| `malformed_json` | the line is not valid JSON, or not valid UTF-8; `id` is `null` |
| `bad_request` | not an object, or `id`, `op`, `word` (empty counts) or `traceDetails` missing or of the wrong type; `id` is `null` when it could not be read |
| `unknown_op` | `op` is a string other than `parse`, `generate` or `shutdown` |
| `request_too_large` | the line exceeds 1,048,576 bytes (newline excluded); the line is discarded, `id` is `null` |
| `render_failed` | the trace document could not be built (the cold command would exit 1 with no output) |
| `internal` | the parser panicked on this word; the details are on stderr |

## The `generate` op

`generate` runs the `pangloss generate --request` v1 document inside the server and keeps one result per seed. It needs a project with authored FieldWorks identity (a snapshot or fwdata); an HC XML project gets `authored_identity_unavailable`. `--describe` declares it as `interfaces.serveOps.generate`, and a test checks that declaration against what the op emits.

```json
{"id":"g1","op":"generate","request":{"schemaVersion":1,"root":{"entryGuid":"..","msaGuid":"..","msaClass":"MoStemMsa"},"morphemes":[{"entryGuid":"..","msaGuid":"..","msaClass":"MoInflAffMsa"}]},"trace":true,"maxTraceSteps":50000}
```

Optional fields: `trace` (default `false`; `true` adds each surface's `steps`), `maxSeeds` (default 10000), `maxTraceSteps` (default 50000; counts trace nodes kept across every seed), `maxWork` (lowers the 50,000,000-unit generation cap for this request) and `wordTimeoutMs` (a wall-clock deadline; without it the server's `--word-timeout-ms` applies). Unknown fields are `bad_request`. The parse `--work-cap` is never the generation cap.

The response is a `result` with `complete`, `stopReason`, `workUnits`, `seedChoices`, `traceSteps`, the effective `limits`, and `seeds`. A seed is one choice of allomorphs: `seedKey` (`r<root allomorph>/p<selection>`), `allomorphs`, `surfaces` and `traceTruncated`. `allomorphs` lists the root, then each requested morpheme in request order, each `{role, entryGuid, msaGuid, allomorphId, formGuid, shape, ruleName}`: `role` is `root`, `nonHead` or `rule`; an affix is a rule, so it has a `ruleName` and a null `allomorphId`, `formGuid` and `shape`, while an entry has `allomorphId`, `formGuid` and `shape` (the chosen allomorph's spelling) and a null `ruleName`. Seeds come in visit order, surfaces are sorted, and a surface reached by two seeds appears under both. Everything is deterministic except a stop on `deadline`.

With `trace: true` a surface is `{surface, derivations, steps, stepsUnresolved, alternatives}`. `derivations` counts the words of this seed that reached the surface; it is 1 unless the seed branched (for example two allomorphs of one affix both matched). Each step is `{stratum, rule:{kind,id,name,guid}, input, output, subrule}` in application order (not sorted), with `+` marking morpheme boundaries. A step list is only given when it is proven: it starts at the seed's shape, every `input` is the previous `output`, and every step is a node on that word's own derivation in the trace tree, never one borrowed from a sibling word. So:

- one derivation: `steps` is that derivation, `alternatives` is `[]`;
- several distinct derivations reached the surface: `steps` is `[]` and `alternatives` holds each one (sorted, at most 16);
- a derivation could not be proven (the trace was truncated, several rule sequences fit equally, or the work budget ran out): `steps` is `[]`, `alternatives` is `[]` and `stepsUnresolved` is `true`. A step list is never a guess.

Without `trace` a surface is `{surface, derivations}`. The attribution runs inside the same work budget and deadline as generation, so it can end a request with `work_limit` or `deadline`. The `version` in the `generate` capability versions the request and response shapes of this op.

`stopReason` is `seed_product_exceeds_budget`, `work_limit`, `deadline`, `trace_limit` or `seed_limit`, or `null` when `complete` is `true`. Any stop sets `complete: false` and still returns the seeds finished so far, including the seed in which the cap hit (its surfaces may be incomplete); they are not the full set. Request errors (`bad_request`, `request_too_large`, `unsupported_schema_version`, `unsupported_msa_class`, `unsupported_form_selection`, `authored_identity_unavailable`, `unknown_entry`, `unknown_msa`, `msa_not_owned_by_entry`, `ambiguous_identity`) are `error` responses; a cap is never an error.

## Closing stdin

End of input on stdin exits 0 immediately, even in the middle of a parse: the in-flight and any queued requests are abandoned and get no response. Because the exit is immediate, an abrupt EOF can leave a partial last line on stdout; a host must discard an unterminated final line. Send `shutdown` for a graceful exit that answers `bye` after the current request.

## Versioning

`protocol` in the ready line and `interfaces.serveProtocol` in `--describe` change only for an incompatible wire change. Adding optional fields or error codes does not bump it.
