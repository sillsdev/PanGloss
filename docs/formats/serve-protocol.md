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
| `unknown_op` | `op` is a string other than `parse`, `generate`, `inventory`, `match`, `effects` or `shutdown` |
| `request_too_large` | the line exceeds 1,048,576 bytes (newline excluded); the line is discarded, `id` is `null` |
| `render_failed` | the trace document could not be built (the cold command would exit 1 with no output) |
| `internal` | the parser panicked on this word; the details are on stderr |

## The `generate` op

`generate` runs the `pangloss generate --request` v1 document inside the server and keeps one result per seed. It needs a project with authored FieldWorks identity (a snapshot or fwdata); an HC XML project gets `authored_identity_unavailable`. `--describe` declares it as `interfaces.serveOps.generate`, and a test checks that declaration against what the op emits.

```json
{"id":"g1","op":"generate","request":{"schemaVersion":1,"root":{"entryGuid":"..","msaGuid":"..","msaClass":"MoStemMsa"},"morphemes":[{"entryGuid":"..","msaGuid":"..","msaClass":"MoInflAffMsa"}]},"trace":true,"maxTraceSteps":50000}
```

Optional fields: `trace` (default `false`; `true` adds each surface's `steps`), `maxSeeds` (default 10000), `maxTraceSteps` (default 50000; counts trace nodes kept across every seed), `maxWork` (lowers the 50,000,000-unit generation cap for this request) and `wordTimeoutMs` (a wall-clock deadline; without it the server's `--word-timeout-ms` applies). Unknown fields are `bad_request`. The parse `--work-cap` is never the generation cap.

The response is a `result` with `complete`, `stopReason`, `workUnits`, `seedChoices`, `traceSteps`, the effective `limits`, and `seeds`. A seed is one choice of allomorphs: `seedKey` (`r<root allomorph>/p<selection>`), `allomorphs`, `surfaces` and `traceTruncated`. `allomorphs` lists the root, then each requested morpheme in request order, each `{role, entryGuid, msaGuid, allomorphId, formGuid, shape, ruleName}`: `role` is `root`, `nonHead` or `rule`; an affix is a rule, so it has a `ruleName` (null when the grammar gives that rule no name, as for an unnamed affix process) and a null `allomorphId`, `formGuid` and `shape`, while an entry has `allomorphId`, `formGuid` and `shape` (the chosen allomorph's spelling) and a null `ruleName`. `entryGuid`, `msaGuid` and an entry's `formGuid` are null when the grammar carries no GUID for them. Seeds come in visit order, surfaces are sorted, and a surface reached by two seeds appears under both. Everything is deterministic except a stop on `deadline`.

With `trace: true` a surface is `{surface, derivations, steps, stepsUnresolved, alternatives}`. `derivations` counts the words of this seed that reached the surface; it is 1 unless the seed branched (for example two allomorphs of one affix both matched). Each step is `{stratum, rule:{kind,id,name,guid}, input, output, subrule}` in application order (not sorted), with `+` marking morpheme boundaries. `rule.name` is null for a rule the grammar leaves unnamed. A step list is only given when it is proven: it starts at the seed's shape, every `input` is the previous `output`, and every step is a node on that word's own derivation in the trace tree, never one borrowed from a sibling word. So:

- one derivation: `steps` is that derivation, `alternatives` is `[]`;
- several distinct derivations reached the surface: `steps` is `[]` and `alternatives` holds each one (sorted, at most 16);
- a derivation could not be proven (the trace was truncated, several rule sequences fit equally, or the work budget ran out): `steps` is `[]`, `alternatives` is `[]` and `stepsUnresolved` is `true`. A step list is never a guess.

Without `trace` a surface is `{surface, derivations}`. The attribution runs inside the same work budget and deadline as generation, so it can end a request with `work_limit` or `deadline`. The `version` in the `generate` capability versions the request and response shapes of this op.

`stopReason` is `seed_product_exceeds_budget`, `work_limit`, `deadline`, `trace_limit` or `seed_limit`, or `null` when `complete` is `true`. Any stop sets `complete: false` and still returns the seeds finished so far, including the seed in which the cap hit (its surfaces may be incomplete); they are not the full set. Request errors (`bad_request`, `request_too_large`, `unsupported_schema_version`, `unsupported_msa_class`, `unsupported_form_selection`, `authored_identity_unavailable`, `unknown_entry`, `unknown_msa`, `msa_not_owned_by_entry`, `ambiguous_identity`) are `error` responses; a cap is never an error.

## Op `inventory`: the grammar's natural classes and features

`interfaces.serveOps.inventory` in `pangloss --describe` declares the op, its fields, errors and stop reasons; a binary without that key lacks the op. It is read-only and additive, and does not change the protocol version. It is a serve op rather than a `--describe` field because its content depends on the loaded grammar, and `--describe` is static.

```json
{"id":"i1","op":"inventory"}
```

The request takes only `maxWork` and `wordTimeoutMs`, with the limits `match` has (a field the op does not know is `bad_request`). The listing spends one work unit per class and per feature value; a request that runs out answers `complete:false` with `stopReason` `work_limit` or `deadline` and the classes and features listed so far, never a silent partial list.

```json
{"id":"i1","type":"result","op":"inventory","complete":true,"stopReason":null,
 "classes":[{"token":"V","name":"V","fullName":null,"id":"ncV","kind":"segment"},
            {"token":"Voiced","name":"Voiced","fullName":null,"id":"ncVoiced","kind":"feature"}],
 "features":[{"name":"voice","id":"featVoice","values":["+","-"]}]}
```

- `classes` is every natural class of the grammar in grammar order. `kind` is `segment` (an explicit segment list) or `feature` (a feature structure).
- `token` is exactly the text `match` accepts between brackets: `[V]`. `match` resolves `[x]` by class `name` first and, failing that, by class `id` (the grammar.xml id, or the FieldWorks guid), so both spellings are accepted. When two classes share a name the first wins that name, and the later one is reached by its `id`; when a class's `id` equals another class's name, the name wins. `token` is the spelling that reaches the class: its name, else its `id`, else `null` (a name that is empty, or starts with `+`, `-` or a Greek letter, or holds `]`, cannot be written in brackets). Parsing `[token]` gives back the class it came from. What `[token]` then selects depends on `match`'s `segmentTest`: with `"identity"` it selects exactly the class's members; under the default `"engine"` a `segment` class selects what rule application's test admits, the union of its members' features, which can include segments the class does not list (in the inventory fixture `[V]`, a class of `a i u`, selects `aiunŋ`). Send `segmentTest:"identity"` to see exactly the listed members. A test matches `[token]` against a word holding every segment, with `"identity"`, and checks, for every class of a fixture that includes names with spaces, brackets and non-ASCII letters, that the selected segments are that class's, a second test pins the default's superset, and that the one class with `token: null` is selected by neither its name nor its `id`. Matching is case-sensitive, so `[voiced]` is an `unknown_natural_class` (a host that lowercases for completion must send the token as listed).
- `name` is the grammar's one label for the class, the one `match` resolves (`null` when the grammar gave none). For a grammar compiled from a FieldWorks project that label is the natural class's Abbreviation, the text environment strings cite.
- `fullName` is the class's Name in the analysis writing system, as FieldWorks shows it (`Vowels` for the Abbreviation `V`), or `null` when the class has no Name, or when the grammar was loaded from HermitCrab XML, which has one label and so no separate Name. It is always present. `match` does not accept it: a host resolves a `fullName` to the class's `token` itself.
- `features` is every phonological feature the grammar defines, in grammar order, with `values`, the names of its symbols in order. The engine's own `Type` feature (segment or boundary) is not listed.

## Op `match`: pattern text over a word list

`interfaces.serveOps.match` in `pangloss --describe` declares the op, its constructs, error codes, statuses, stop reasons and limits; a binary without that key lacks the op. It is additive and does not change the protocol version.

```json
{"id":"m1","op":"match","pattern":"#[Stop]([V])*","words":["aba","kuŋ"],"boundaryMarker":"+","maxWords":100000,"maxMatchesPerWord":1000}
```

- `pattern` (string, at most 4096 bytes) is HermitCrab pattern text. `words` (array of strings) or `wordsFile` (path, one word per line, blank lines skipped) gives the words; send exactly one. `boundaryMarker`, `maxWords` (default 100000, at most 1000000), `maxMatchesPerWord` (default 1000, at most 100000), `maxWork` (matcher work units per word; default 50,000,000, at most the server's `--work-cap`) and `wordTimeoutMs` (per-word wall clock, at most 600000; default the server's `--word-timeout-ms`, else 10000) are optional. A field the op does not know is refused with `bad_request`, as for `generate`.
- Words are segmented with the surface character-definition table `parse` uses (the last stratum's), through `segment`. The matcher is the synthesis environment matcher (optional boundary nodes), so an unanchored pattern returns every span. `segmentTest` (optional) picks how a segment is tested against a pattern node: `"engine"` (the default) is rule application's own test, where a segment matches by its features, so a segment-list class like `{i,u}` accepts the union of their feature values and a literal `/i/` accepts every segment with `i`'s feature bundle (and an underspecified segment, as the engine does); `"identity"` is the older exact test, where a literal matches only its own segment and a class only its members. Any other value is `bad_request`. `--describe` lists `segmentTests` and `defaultSegmentTest`.

Pattern text, with whitespace between items ignored:

| Item | Meaning |
|---|---|
| `[Name]` | a natural class of the grammar, by name, else by xml id (see op `inventory` for the spelling of every class) |
| `/ŋk/` | literal segments, segmented against the surface table |
| `( items )` | optional |
| `( items )*` | optional and repeated |
| `#` | word edge: leading `#` anchors the start, trailing `#` the end |
| `+` | morpheme boundary; needs `boundaryMarker` |
| `target / left _ right` | an environment: the target's span, kept where the left and right sides hold; see below |

Feature bundles and alpha variables, `[+voice -son]` and `[αback]`, are matched by the engine's own pattern compiler. Each bundle becomes a natural class made from its valued features, so `[+voice]` selects what a grammar class with those values selects. `+` and `-` pick the feature's symbol of that name; a feature with no such symbol (a feature whose values are not `+` and `-`) is a typed `unsupported_construct` (`construct` `featureValue`, `offset` of the word). `[αback]` binds a variable to the feature and `[-αback]` requires the opposite; one Greek letter is one variable across the whole pattern, so `[αvoice][αvoice]` finds two neighbours of one voice, and different letters agree independently (`[αvoice βson][αvoice βson]`). A letter belongs to one feature: the same letter on two features (`[αvoice] _ [αson]`, `[αvoice αson]`) is a typed `unsupported_construct` with `construct` `alphaAcrossFeatures` (listed under `gated` in `--describe`), because their symbols are not comparable. A bundle may mix values and variables (`[+son αvoice]`) and names a feature once. A segment with no value for a feature (and no `defaultSymbol`) is selected by both `[+f]` and `[-f]` and binds any alpha, as rule application treats it, so a `[+voice]` hit is not proof the segment is voiced. Variables bind in a rule's order, target then left then right, so one variable agrees across the target and both sides of an environment. A bundle error is `pattern_parse` (a repeated feature, a word with no sign, a sign with no name) or `unknown_feature`. Feature names are checked first.

Environments, `target / left _ right`, are matched with the engine's own environment compiler and checker, the code a phonological rule's environment runs through, and the sides are tested exactly as rule application tests them (a literal names one segment, a class exactly its members). Only the target is widened by the default `segmentTest:"engine"`: its spans are a superset of the positions rule application rewrites, so a position a rule rewrites is always a reported span. The extras are positions the rule leaves unchanged (its output equals its input) and overlapping spans, since a rule applies leftmost and non-overlapping. For example `/p/ / [V] _ [V]` marks each `p` between vowels. A test feeds every simultaneous rewrite rule of the conformance suite to `match` and asserts that it reports every position the rule changes (130 rules over short words of each grammar's own segments, 0 missed; the `intermediate-alpha-target` fixture is left out because it runs on another rule's output). With `segmentTest:"identity"` the target too is exact, which can miss a position rule application rewrites (a segment sharing a feature bundle with the literal, a member-less segment of a feature-union class, an underspecified segment).

- Each match is the target's span: `segments`, `offsets`, `left` and `right` mean what they do for a plain pattern, so `left` and `right` are the rest of the word either side of the target, not the extent the sides matched. The target is any pattern text, or empty (`/ [V] _ [V]`), which is an insertion point: it reports a zero-width match at every position where the sides hold.
- Every target span where both sides hold is listed, overlapping spans included, unlike a rule, which applies leftmost and non-overlapping: `[V]([V])* / _` on `aaa` lists six spans. Read the list as where the environment holds, not as the positions one rule application would change. A zero-width match beside a `+` boundary is reported at the segment index, so the positions before and after the boundary share one index.
- Either side may be empty, and both may (`[V] / _` is `[V]`). `#` opens the left side (`# _`) or closes the right (`_ #`) and means a word edge, as in a rule; the target takes no `#`. A side may hold classes, literals, `( )` and `( )*` groups and `+`. Boundaries are transparent to a side unless it names `+`, as in synthesis.
- Write the separator as `" / "`, a slash with spaces on both sides, which is the canonical form for a host building the string. The `/` needs whitespace or `_` after it (`[V] / [Stop] _`); a `/` followed by other text opens a literal (`/ŋ/`). The `_` is required: `[V] / [Stop]` is a `pattern_parse` error expecting `'_'`, as is `_` with no `/`.
- The sides run under the same per-word work budget and deadline as the target, so an environment that exhausts either ends the word with `work_limit` or `deadline`.

Without `boundaryMarker`, words carry no boundaries: a pattern with `+` is refused with `boundary_needs_marked_input`, and a word containing a boundary character is `unsegmentable` (`reason: boundary_in_unmarked_word`). With `"boundaryMarker":"+"` (which must be a boundary character definition of the surface table, else `unknown_boundary_marker`) the words are pre-cut, `+` matches the marker, and the marker is not a segment: it takes no segment index and no offsets. A marker with no `+` in the pattern is transparent, as boundaries are in synthesis.

Result:

```json
{"id":"m1","type":"result","op":"match","complete":true,"stopReason":null,"words":[
 {"word":"aba","status":"ok","matches":[{"segments":[0,1],"offsets":[0,1],"left":[0,0],"right":[1,3]}]},
 {"word":"axa","status":"unsegmentable","matches":[],"at":{"charIndex":1,"offset":1},"reason":"no_character_definition"}]}
```

- `words` is in request order. `segments` is half-open `[start,end)` over the word's segments, from 0; `offsets` are half-open UTF-8 byte offsets into the word (into its NFD form, given as `normalized`, when the word is not NFD). `left` and `right` are the segment ranges before and after the match. `matches` is sorted and has no duplicates.
- `status` is `ok`, `unsegmentable` or `incomplete`. An unsegmentable word is reported, counted, and does not make the response incomplete.
- Every word runs under its own work budget and deadline. A word that exhausts either is `incomplete` with `stop: "work_limit"` or `"deadline"` and no matches (partial spans are dropped), the response is `complete:false` with that `stopReason` (the first), and the remaining words still run. `work_limit` is deterministic; `deadline` is wall-clock and is the one nondeterministic stop.
- Zero-width matches: a pattern whose every item is optional (`([V])*`, `#([V])*`, `#`) can match the empty string, so it reports an empty match `[p,p]` at every segment position its `#` anchors allow (`offsets` and `right` follow from `p`), beside its non-empty matches. `+` alone reports the zero-width span at each boundary; an unanchored pattern with a required item reports none.
- `unsegmentable` words carry `reason`: `no_character_definition` (with `at`), `boundary_in_unmarked_word` (the request has no `boundaryMarker`), or `unexpected_boundary` (the request has one, and the word holds a different boundary character, such as the synthetic `.`).
- A match cap never truncates silently: past `maxMatchesPerWord` the word is `incomplete` with `stop: "match_cap"` and keeps its first sorted matches; past 1,000,000 matches in one response the rest are `incomplete` with `stop: "total_match_cap"`. Either sets `complete:false` and `stopReason` (the first).
- Errors (the request never ran, with the extra fields shown): `pattern_parse` (`token`, `offset`, `expected`), `unknown_natural_class` and `unknown_feature` (`name`, `offset`, `near`), `unsupported_construct` (`construct`, `offset`), `boundary_needs_marked_input` (`offset`), `unknown_boundary_marker`, `too_many_words` (`words`, `maxWords`), `words_file_unreadable` (`path`), `bad_request` (including an unknown request field), `pattern_compile_failed`, `internal`. Pattern offsets are UTF-8 byte offsets into `pattern`.

## Op `effects`: which phonological rules can change a morpheme

`interfaces.serveOps.effects` in `pangloss --describe` declares the op: its request fields, edges, error codes, stop reasons, `mayApply` values, result and effect fields, limits and the `nonLocalCriteria`. A binary without that key lacks the op. It is additive and does not change the protocol version. It needs a project with authored FieldWorks identity (a snapshot or fwdata), else `authored_identity_unavailable`.

```json
{"id":"e1","op":"effects","morpheme":{"entryGuid":"..","msaGuid":"..","allomorphId":7},"edges":["left","right"],"rootsPerEnvironment":3,"maxWork":1000000,"wordTimeoutMs":10000}
```

- `morpheme.entryGuid` and `morpheme.msaGuid` address a stem or affix morpheme (the MSA class is found by the server; no class on the wire). `allomorphId` is the compiled allomorph id that `generate` reports; absent means every allomorph. `edges` is `left`, `right` or both (default both). `rootsPerEnvironment` is 1 to 50 (default 3). `maxWork` counts scan units (one allomorph against one rule subrule; default as `match`, at most the server's `--work-cap`, which refuses a larger value with `bad_request`) and `wordTimeoutMs` is the wall-clock deadline (at most 600000; default the server's `--word-timeout-ms`, else 10000). An unknown request field is `bad_request`.
- Result: `complete`, `stopReason` (`work_limit`, `deadline` or `null`), `partial` (true exactly when the scan stopped, then `effects` holds the units finished so far), `morpheme`, `allomorphs` (`allomorphId`, `form`, `segments`, `formPartial`), `effects` and `limits`. A form is the allomorph's own text (an affix keeps its boundary marker); `segments` excludes boundaries; `formPartial` is true when an affix process copies or modifies stem material, so the listed form holds only the segments the allomorph writes.
- Each effect names `rule` (`kind` `rewrite` or `metathesis`, `id`, `guid`, `name`, `stratum`, `ordinal` within the stratum, `status`), `subrule` (index; `null` for a metathesis rule), `allomorphId`, `edge` and `mayApply`. Every phonological rule of the grammar appears for every allomorph and requested edge. A rule that is not in the compiled grammar is listed too, with `status` `not_compiled` (its build failed) or `disabled` (a metathesis rule with an unset switch is not loaded, as HCLoader), `stratum` and `ordinal` `null`, `mayApply:"unknown"` and the loader's reason; those rows follow the compiled rules. A grammar loaded from XML records no skipped rules, so it lists none.
- **Conservative contract.** The op never claims a rule cannot change a form without proof. Anything not proven is `mayApply:"unknown"` with a `reason`; an unmodelable subrule is listed as unknown and does not make the response incomplete.
- **`mayApply` and `reason`.** Each rule's left-hand side (the target, not the environment) is matched against the allomorph's own form, segmented with the rule's stratum table, and also probed for a match that begins or ends beyond the form's edge, where the neighbouring morpheme is unknown. The values are `unknown` and `notOnForm`.
  - `notOnForm`, `reason:"no_lhs_match"`: the target has no match inside the form and none that reaches across either edge, and nothing before the rule can change the form (see `may_be_fed`). It is not "cannot change": the environment is not checked, and bleeding is not considered. An affix form is each run of segments the allomorph inserts, flanked by unknown stem material, so a match across a run's edge counts as straddling.
  - The target is tested with rule application's own segment test, not `match`'s identity test: a segment matches by its features, so `{i, u}` accepts the union of their feature values and `/i/` accepts any segment with `i`'s feature bundle. Effects therefore never says `notOnForm` for a segment the engine would rewrite; `match` uses the same test by default (`segmentTest`).
  - `unknown` with `reason:"environment_unchecked"` and `lhsSpans` (`[start, end)` segment spans of the form): the target matches inside the form; the environment is not yet checked.
  - `unknown` with `reason:"lhs_may_straddle_edge"`: no match inside, but one could begin or end beyond an edge. The probe over-approximates (the neighbour is unconstrained, a quantifier cut by the edge matches zero repetitions).
  - `unknown` with `reason:"may_be_fed"`: the target has no match, but the rule could be fed. Some earlier rule in application order (the morpheme's stratum onward, then ordinal) is not `notOnForm` on the same form, or some affix in the grammar modifies stem material before the phonology runs. Only a rule that nothing earlier can touch keeps `notOnForm`. A finer analysis (can an earlier output produce this target) may narrow it later.
  - `unknown` otherwise: `empty_lhs_epenthesis` (an empty target); `lhs_boundary` (the target holds a boundary or anchor, which lies on a morpheme edge where the form alone has no node, and does not match inside); `lhs_not_modelable` (the target cannot be compiled or cut soundly: alpha variables, a quantifier over several nodes, a pattern that can match no segment, more than 8 nodes); `underspecified_segments` (a segment of the form has no value for a feature of a grammar that has features; rule application treats an absent value as matching where this matcher does not); `stratum_precedes_morpheme` (the rule's stratum comes before the morpheme's); `lhs_match_budget` (the matcher stopped on its own work or time limit for the unit); `form_not_segmentable_in_stratum` (the form does not segment against the rule's stratum table); `form_partly_unmodelled` (the allomorph also writes material that the listed form does not hold: a modified part, an inserted context, a part copied twice as in reduplication, or no segment of its own at all). A rule that is not in the compiled grammar stays `unknown` with the loader's reason, never `notOnForm`. `--describe` lists every `reason` under `reasons`.
- **Rule text.** Each effect also carries its rule row as text `match` reads, so a host can hand the rule back to `match` and reason about it. The subrule is one alternative: a rule with several subrules has one row per subrule, each with its own `rhs` and `environment`.
  - `lhs`: the target as pattern text (empty for an insertion). `environment`: `left _ right`, as `match` takes after the `/`. `matchPattern`: `lhs / environment`, ready to send as `match`'s `pattern` (for a metathesis rule, the plain pattern text, with `environment` and `rhs` `null`). `rhs`: the subrule's replacement as pattern text (empty for a deletion). A class is written `[token]` with the `token` of the `inventory` op (a feature bundle `[+voice αback]` where no token reaches the class); a literal segment is `/x/`.
  - `boundaryMarker`: the boundary character a `+` stands for, or `null`; send it as `match`'s `boundaryMarker` whenever it is set.
  - Nothing is printed loosely. A part the text cannot say exactly is `null`, with an `unprintable` entry `{part, reason}` (`part` is `lhs`, `environment`, `matchPattern`, `rhs` or `rule`; `--describe` lists `unprintableReasons`): for example an alpha variable on a class that is not a signed feature bundle or that spans two features, a bounded quantifier other than optional and optional-repeated, a segment of another stratum's character table, or a rule that is not compiled. `matchPattern` is `null` when `lhs` or `environment` is.
  - The text is read against the last stratum's (surface) table; a rule of an earlier stratum with its own character table is `unprintable` (`stratum_table_differs`).
  - `match`'s default `segmentTest:"engine"` tests the target by features, as rule application does, and its environment sides by identity, as rule application does, so on the words rule application reads, `match` of `matchPattern` reports every segment position an equal-length rewrite changes (the span of an environment match; `match` also reports positions the rule would leave unchanged). This holds for a rule that is not `nonLocal` by `iterativeSpreading`; a spreading rule rewrites positions its first-pass environment does not show.
  - `nonLocal` is `true` when the rule meets any criterion of `nonLocalCriteria`, listed in `nonLocalReasons`: `unboundedQuantifier` (a `( )*` group in any part), `alphaAgreement` (an alpha variable), `iterativeSpreading` (an iterative rewrite one of whose environment nodes can match a segment some subrule writes, so an earlier rewrite feeds a later site). The criteria over-approximate: `nonLocal:true` means the rule may be non-local, `false` that none of the three applies. It is `null` (with empty reasons) for a rule that is not compiled.
- **Out of scope.** Test roots (`testRoots`) are not reported yet. Bleeding is not analysed, and feeding is only answered conservatively (`may_be_fed`): nothing says how rules interact beyond their stratum and ordinal.
- Errors (the request never ran): `bad_request`, `unknown_morpheme` (unknown entry, MSA not of the entry, or an `allomorphId` the morpheme does not own), `ambiguous_identity`, `authored_identity_unavailable`, `form_not_segmentable` (an allomorph form does not segment against its table; never reported as empty segments), `internal`. A work or deadline stop is a `result` with `complete:false`, never an error.

## Closing stdin

End of input on stdin exits 0 immediately, even in the middle of a parse: the in-flight and any queued requests are abandoned and get no response. Because the exit is immediate, an abrupt EOF can leave a partial last line on stdout; a host must discard an unterminated final line. Send `shutdown` for a graceful exit that answers `bye` after the current request.

## Versioning

`protocol` in the ready line and `interfaces.serveProtocol` in `--describe` change only for an incompatible wire change. Adding optional fields or error codes does not bump it.
