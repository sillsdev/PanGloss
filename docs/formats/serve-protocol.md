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
- End of input on stdin exits 0 at once (see Closing stdin). Cancellation is the host killing the process.

## Responses

```json
{"id":"1","type":"result","exitCode":0,"result":{...}}
{"id":"1","type":"error","code":"bad_request","message":"..."}
{"id":"2","type":"bye"}
```

A request that ran a parse always yields a `result`, including a parse that stopped at a cap or timeout. `error` is for requests that never ran.

`result` carries what the equivalent cold command printed to stdout, and `exitCode` is the cold command's exit code. The cold argv is the command line `<project>` and the server's own cap, timeout and `--guess` flags, so each mode equals exactly one cold invocation:

| `traceDetails` | cold argv | `result` | `exitCode` |
|---|---|---|---|
| `true` | `pangloss parse <project> <word> --trace --trace-format json --trace-details` | the JSON document the command prints (the trace-details envelope, see `trace-details-v2.md`), as a JSON value | always 0 (the command exits 0 even when capped or timed out; read `search.capped` / `search.timedOut` in the document) |
| `false` (default) | `pangloss parse <project> <word> --trace --trace-format json` | the command's whole stdout as one JSON string: the line `<word>\t<signature>\n`, then `guessed:\t<true\|false>\n` only when the server runs with `--guess`, then the JSON trace tree (absent when no trace exists) | 1 when the parse was incomplete (capped, timed out, invalid shape), else 0; the string is still the full stdout |

For `traceDetails: true`, every field is identical to the cold document except the wall-clock measurements: `elapsedNs`, `timedNs`, `timingOverrunNs`, `unattributedNs` and every `*ElapsedNs` field. For `traceDetails: false` the string is byte-identical.

With `--timing`, a `result` response also carries `"timing":{"parseMs":N,"renderMs":N}`.

### Error codes

The process keeps running after every `error`.

| code | meaning |
|---|---|
| `malformed_json` | the line is not valid JSON, or not valid UTF-8; `id` is `null` |
| `bad_request` | not an object, or `id`, `op`, `word` (empty counts) or `traceDetails` missing or of the wrong type; `id` is `null` when it could not be read |
| `unknown_op` | `op` is a string other than `parse` or `shutdown` |
| `request_too_large` | the line exceeds 1,048,576 bytes (newline excluded); the line is discarded, `id` is `null` |
| `render_failed` | the trace document could not be built (the cold command would exit 1 with no output) |
| `internal` | the parser panicked on this word; the details are on stderr |

## Closing stdin

End of input on stdin exits 0 immediately, even in the middle of a parse: the in-flight and any queued requests are abandoned and get no response. Because the exit is immediate, an abrupt EOF can leave a partial last line on stdout; a host must discard an unterminated final line. Send `shutdown` for a graceful exit that answers `bye` after the current request.

## Versioning

`protocol` in the ready line and `interfaces.serveProtocol` in `--describe` change only for an incompatible wire change. Adding optional fields or error codes does not bump it.
