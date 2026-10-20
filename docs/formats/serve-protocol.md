# `pangloss serve` protocol (version 1)

`pangloss serve` keeps one compiled project resident and answers parse requests over stdin and stdout, so a host pays the project load and grammar compile once instead of on every word. Measured on a 22,237-entry project: load and compile cost 7-9 s per process, the parse itself 70-90 ms.

```
pangloss serve --project <path.fwdata|snapshot.json> [--step-cap N|unbounded] [--work-cap N|unbounded] [--word-timeout-ms N] [--guess] [--timing]
```

`--project` accepts whatever `pangloss parse` accepts as `<grammar>`. The cap, timeout and guess flags mean what they mean for `parse` and apply to every request. Unknown options are refused.

A host can gate on this mode: `pangloss --describe` carries `"interfaces": {"serveProtocol": 1}` and lists a `serve` command.

## Streams

- stdout carries protocol lines only: one JSON object per line, flushed after every line.
- stderr carries logs and grammar warnings, in the same text `parse` prints.
- Requests are processed one at a time, in order. Each request yields exactly one response, in request order.

## Startup

After loading and compiling, one line:

```json
{"type":"ready","protocol":1,"loadMs":8421}
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
- End of input on stdin exits 0 without a response. Cancellation is the host killing the process.

## Responses

```json
{"id":"1","type":"result","result":{...}}
{"id":"1","type":"error","code":"bad_request","message":"..."}
{"id":"2","type":"bye"}
```

`result` is exactly the JSON document `pangloss parse` prints for that word with the same flags:

- `traceDetails: true` is `pangloss parse <project> <word> --trace --trace-format json --trace-details`: the trace-details envelope (see `trace-details-v2.md`). Every field is identical except the wall-clock measurements: `elapsedNs`, `timedNs`, `timingOverrunNs`, `unattributedNs` and every `*ElapsedNs` field (`selfElapsedNs`, `analysisSelfElapsedNs`, ...).
- `traceDetails: false` is the JSON trace tree `pangloss parse <project> <word> --trace --trace-format json` prints after its signature line, or `null` when no trace exists (a word that fails shape validation). If the parse is incomplete (capped, timed out, invalid shape), the response is an `error` with code `parse_incomplete`, as the cold command exits non-zero; use `traceDetails: true` to get a trace for such words.

With `--timing`, a `result` response also carries `"timing":{"parseMs":N,"renderMs":N}`.

### Error codes

The process keeps running after every `error`.

| code | meaning |
|---|---|
| `malformed_json` | the line is not valid JSON, or not valid UTF-8; `id` is `null` |
| `bad_request` | not an object, or `id`, `op`, `word` or `traceDetails` missing or of the wrong type; `id` is `null` when it could not be read |
| `unknown_op` | `op` is a string other than `parse` or `shutdown` |
| `parse_incomplete` | see above (`traceDetails: false` only) |
| `render_failed` | the trace document could not be built |
| `internal` | the parser panicked on this word; the details are on stderr |

## Versioning

`protocol` in the ready line and `interfaces.serveProtocol` in `--describe` change only for an incompatible wire change. Adding optional fields or error codes does not bump it.
