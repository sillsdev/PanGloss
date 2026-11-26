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
- `parse`: `word` is a non-empty string. `traceDetails` is optional (default `false`). `ruleTrace` is optional (default `false`) and only affects `traceDetails: false`: the JSON rule-trace tree is built and appended to `result` only when it is `true`; `traceDetails: true` ignores it.
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
| `false` (default) | `pangloss parse <project> <word> --trace --trace-format json` with `ruleTrace: true`; the default is that output without the tree | the command's whole stdout as one JSON string: the line `<word>\t<signature>\n`, then `guessed:\t<true\|false>\n` only when the server runs with `--guess` or `--guess=true` (never because the project guesses), then, only when the request has `ruleTrace: true`, the JSON trace tree (absent when no trace exists); without `ruleTrace` the string ends after those lines and the tree is not built | 1 when the parse was incomplete (capped, timed out, invalid shape), else 0; the string is still the full stdout |

For `traceDetails: true`, every field is identical to the cold document except the wall-clock measurements: `elapsedNs`, `timedNs`, `timingOverrunNs`, `unattributedNs` and every `*ElapsedNs` field. For `traceDetails: false` with `ruleTrace: true` the string is byte-identical; without `ruleTrace` it is that string up to the end of the signature (and `guessed:`) lines, with no trace tree.

Every `result` response, in both modes, also carries the word's search counts beside `result`, outside the cold stdout:

- `status`: `ok`, `CAP` when the work cap stopped the parse, `MEMCAP` when the per-word parser-state ceiling (100 MiB by default) or the affix-matching capture-state ceiling (256 MiB by default) stopped it, `TIMEOUT` when the
  time limit stopped it, or `SKIPPED` for an invalid shape. This optional field is absent from
  older servers.
- `workSteps`: the word's work (with `traceDetails: false` and no `ruleTrace` the parse is untraced, so the count equals batch's `work_steps` and a cold untraced `parse`; this is the reference count. A traced parse, `ruleTrace: true` or `traceDetails: true`, can count differently in either direction, so a word near `--work-cap` can differ in either direction: `niyidtan` in `prefixal-discontinuous-slot-dependency` at `--work-cap 300` is `CAP` plain and a full analysis with `ruleTrace: true`), meaning the total of work steps the parse reserved. This is the quantity `--work-cap` bounds.

This is the same count as the `work_steps` column that `pangloss batch --work-columns` writes (see `batch-tsv.md`) and, for the traced modes only, `traceDetails: true` and `ruleTrace: true`, as `search.workSteps` in the document and the traced count. They are deterministic for a given project, word, cap settings and PanGloss version, so compare parse cost with `workSteps`, not with wall-clock time. A parse stopped by `--word-timeout-ms` is the exception: its counts depend on this computer's speed. Treat the field as optional: older servers omit it.

Every `result` response also carries `guessed`: `true` when the analyses were guessed from a lexical pattern rather than found in the lexicon, whatever the `--guess` flag. A project that guesses by default can return guessed analyses without the `guessed:` line, so read this field, not the string. Treat it as optional: older servers omit it.

With `--timing`, a `result` response also carries `"timing":{"parseMs":N,"renderMs":N}`.

### Error codes

The process keeps running after every `error`.

| code | meaning |
|---|---|
| `malformed_json` | the line is not valid JSON, or not valid UTF-8; `id` is `null` |
| `bad_request` | not an object, or `id`, `op`, `word` (empty counts) or `traceDetails` missing or of the wrong type, or `ruleTrace` present and not a boolean; `id` is `null` when it could not be read |
| `unknown_op` | `op` is a string other than `parse`, `generate`, `inventory`, `match`, `effects`, `shadowedAllomorphs`, `health` or `shutdown` |
| `request_too_large` | the line exceeds 1,048,576 bytes (newline excluded); the line is discarded, `id` is `null` |
| `render_failed` | the trace document could not be built (the cold command would exit 1 with no output) |
| `internal` | the parser panicked on this word; the details are on stderr |

## The `generate` op

`generate` runs the `pangloss generate --request` v1 document inside the server and keeps one result per seed. It needs a project with authored FieldWorks identity (a snapshot or fwdata); an HC XML project gets `authored_identity_unavailable`. `--describe` declares it as `interfaces.serveOps.generate`, and a test checks that declaration against what the op emits. `pangloss generate <project> --requests <file.jsonl|->` runs the same op over a JSONL file with no server (see `docs/generation.md`, "Batch requests"); its rows equal the responses `serve` gives.

```json
{"id":"g1","op":"generate","request":{"schemaVersion":1,"root":{"entryGuid":"..","msaGuid":"..","msaClass":"MoStemMsa"},"morphemes":[{"entryGuid":"..","msaGuid":"..","msaClass":"MoInflAffMsa"}]},"trace":true,"maxTraceSteps":50000}
```

Optional fields: `trace` (default `false`; `true` adds each surface's `steps`), `maxSeeds` (default 10000), `maxTraceSteps` (default 50000; counts trace nodes kept across every seed), `maxWork` (lowers the 50,000,000-unit generation cap for this request) and `wordTimeoutMs` (a wall-clock deadline; without it the server's `--word-timeout-ms` applies). Unknown fields are `bad_request`. The parse `--work-cap` is never the generation cap.

The response is a `result` with `complete`, `stopReason`, `workUnits`, `seedChoices`, `traceSteps`, the effective `limits`, and `seeds`. A seed is one choice of allomorphs: `seedKey` (`r<root allomorph>/p<selection>`), `allomorphs`, `surfaces` and `traceTruncated`. `allomorphs` lists the root, then each requested morpheme in request order, each `{role, entryGuid, msaGuid, allomorphId, formGuid, shape, ruleName}`: `role` is `root`, `nonHead` or `rule`; an affix is a rule, so it has a `ruleName` (null when the grammar gives that rule no name, as for an unnamed affix process) and a null `allomorphId`, `formGuid` and `shape`, while an entry has `allomorphId`, `formGuid` and `shape` (the chosen allomorph's spelling) and a null `ruleName`. `entryGuid`, `msaGuid` and an entry's `formGuid` are null when the grammar carries no GUID for them. Seeds come in visit order, surfaces are sorted, and a surface reached by two seeds appears under both. Everything is deterministic except a stop on `deadline`.

With `trace: true` a surface is `{surface, derivations, steps, stepsUnresolved, alternatives, normalized, morphSpans}` (the last two are described under "Morpheme spans" below). `derivations` counts the words of this seed that reached the surface; it is 1 unless the seed branched (for example two allomorphs of one affix both matched). Each step is `{stratum, rule:{kind,id,name,guid}, input, output, subrule}` in application order (not sorted), with `+` marking morpheme boundaries. `rule.name` is null for a rule the grammar leaves unnamed. A step list is only given when it is proven: it starts at the seed's shape, every `input` is the previous `output`, and every step is a node on that word's own derivation in the trace tree, never one borrowed from a sibling word. So:

- one derivation: `steps` is that derivation, `alternatives` is `[]`;
- several distinct derivations reached the surface: `steps` is `[]` and `alternatives` holds each one (sorted, at most 16);
- a derivation could not be proven (the trace was truncated, several rule sequences fit equally, or the work budget ran out): `steps` is `[]`, `alternatives` is `[]` and `stepsUnresolved` is `true`. A step list is never a guess.

### Morpheme spans

With `trace: true` each surface also says which of its segments each requested morpheme occupies, read from the engine's own morph ranges on the final shape, not guessed. `morphSpans` is a list with one entry per distinct answer the surface's derivations gave, each `{derivations, segmentCount, morphemes, unattributed}`, where `derivations` counts the words that gave that answer (so the counts sum to the surface's `derivations`).

- `morphemes` has one entry per item of the seed's `allomorphs`, in that order: `{item, spans, reason, rule}`. `item` indexes `allomorphs`. `spans` is a list of runs, `{segments, offsets}`, because an infix or a circumfix splits one morpheme into several (an infix inside a root splits the root's spans in two); an empty list means the morpheme has no segment left on the surface (a null affix, or one a rule deleted). `reason` and `rule` are null whenever `spans` is a list.
- Units are those of `match`: `segments` is half-open `[start, end)` over the surface's segments (a multi-character representation is one segment, a boundary is none) and `offsets` is half-open UTF-8 byte offsets into the NFD form of the surface. `normalized` is that NFD form when the surface is not already NFD, else null. `offsets` is null for every run of a derivation whose segments' own NFD forms do not concatenate to the NFD surface (a combining mark that reorders across a segment edge); `segments` is still exact.
- A wrong span is worse than none. When the engine cannot place a morpheme soundly, `spans` is null and `reason` says why: `span_lost_by_rule` (a phonological rule moved the morpheme's segments across a morpheme edge by metathesis, or deleted a segment beside a neighbour's it changed, as in a merger, and `rule` names that rule), `nested_morph` (the engine records the morpheme as sharing its range with another, so no segment is its alone, as in an in-place mutation or a truncation), `compound_unsupported` (a compounding seed or word), `repeated_morpheme` (the seed names one morpheme twice, so its records cannot be told apart), `morph_not_recorded` (the engine kept no record of the morpheme) or `segments_not_aligned` (a segment has no single rendering, or `match`'s greedy segmenter would cut the surface somewhere else, so segment indexes would not match `match`'s), `span_unknown_truncation` (an affix that inserts nothing, a truncation or subtraction, leaves the engine only a positional mark on some segment, so no morpheme of that word can be placed; every morpheme of the word is null and its segments are listed as unattributed) or `span_ambiguous_copy` (a reduplication whose direction the grammar does not state, and which has no single whole-input copy, so which copy is the root is arbitrary; every morpheme of the word is null and its segments are listed as unattributed, since an affix inside the stem would otherwise keep a span on an arbitrary copy). `rule` is `{kind, id, name, guid}` like a step's rule, or null.
- Every segment is in exactly one place: one morpheme's `spans` or one `unattributed` run, so the spans and the unattributed runs together tile `[0, segmentCount)` with no overlap. `unattributed` lists runs `{segments, offsets, reason, rule}` that no requested morpheme claims: `inserted_by_rule` (a phonological rewrite inserted the segment, as an epenthesis does, `rule` names it; the segment is listed here rather than given to a neighbouring morpheme), `no_morph_owner` (no morph range holds it), `unrequested_morpheme` (the engine attributes it to a morpheme the request did not name), and any reason above when a morpheme's own segments are listed here because its `spans` are null (so a null never hides which segments are in doubt). Runs are maximal runs of one reason and rule.
- Copied material (a reduplication or a process affix that copies stem segments) belongs to the affix when the grammar states a prefix or suffix direction or the rule has one whole-input copy; there the engine's base/affix decision is sound by construction. With no such evidence the whole word is `span_ambiguous_copy` and nothing is picked.
- `span_lost_by_rule` covers one rule that deletes a segment and changes its neighbour, or that moves segments across an edge. A coalescence written as two rules (one changes a vowel, a later one deletes its neighbour) is not detected as a merger: the surviving vowel is the engine's attribution, not necessarily the linguist's.

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

- `pattern` (string, at most 4096 bytes) is HermitCrab pattern text. `words` (array of strings) or `wordsFile` (path, one word per line, blank lines skipped) gives the words; send exactly one. `boundaryMarker`, `boundaryMarkers`, `maxWords` (default 100000, at most 1000000), `maxMatchesPerWord` (default 1000, at most 100000), `maxWork` (matcher work units per word; default 50,000,000, at most the server's `--work-cap`) and `wordTimeoutMs` (per-word wall clock, at most 600000; default the server's `--word-timeout-ms`, else 10000) are optional. A field the op does not know is refused with `bad_request`, as for `generate`.
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

Without `boundaryMarker`, words carry no boundaries: a pattern with `+` is refused with `boundary_needs_marked_input`, and a word containing a boundary character is `unsegmentable` (`reason: boundary_in_unmarked_word`). With `"boundaryMarker":"+"` (which must be a boundary character definition of the surface table, else `unknown_boundary_marker`) the words are pre-cut, `+` matches the marker, and the marker is not a segment: it takes no segment index and no offsets. A marker with no `+` in the pattern is transparent, as boundaries are in synthesis. A word that mixes boundary kinds (a morpheme `+` and a clitic `=`) needs `boundaryMarkers`, an array of the other boundary characters the words hold, each a boundary character definition of the surface table (else `unknown_boundary_marker`). Every kind listed, and `boundaryMarker` too, is admitted in a word and is transparent to the pattern unless it names `+`; `+` still stands for `boundaryMarker` alone, so a pattern with `+` and only `boundaryMarkers` is `boundary_needs_marked_input`. A boundary character in no list is still `unsegmentable` (`unexpected_boundary`). The result echoes `boundaryMarkers` when sent. Existing requests are unchanged.

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
  - `lhs`: the target as pattern text (empty for an insertion). `environment`: `left _ right`, as `match` takes after the `/`. `matchPattern`: `lhs / environment`, ready to send as `match`'s `pattern` (for a metathesis rule, the plain pattern text, with `environment` and `rhs` `null`). `rhs`: the subrule's replacement as pattern text (empty for a deletion). An `rhs` literal `/x/` is a segment whose features are written over the target; an `rhs` class or bundle (`[+voice]`, `[token]`) is a feature change unified into the matched segment, not "any member of the class", so do not widen alternatives to every member. Alpha letters in `rhs` are the letters of `matchPattern` (one printer serves the whole row). A class is written `[token]` with the `token` of the `inventory` op (a feature bundle `[+voice αback]` where no token reaches the class); a literal segment is `/x/`.
  - `boundaryMarker`: the boundary character a `+` stands for, or `null`; send it as `match`'s `boundaryMarker` whenever it is set. A host whose words hold boundaries must also send one when it is `null`, and list every other boundary kind its words hold in `boundaryMarkers`: a word with a kind named in neither is `unsegmentable` (`boundary_in_unmarked_word` or `unexpected_boundary`), which is never "no match".
  - Nothing is printed loosely. A part the text cannot say exactly is `null`, with an `unprintable` entry `{part, reason}` (`part` is `lhs`, `environment`, `matchPattern`, `rhs`, `rule` or, in `outputUnprintable`, `outputMatchPattern`; `--describe` lists `unprintableReasons`): for example an alpha variable on a class that is not a signed feature bundle or that spans two features, a bounded quantifier other than optional and optional-repeated, a segment of another stratum's character table, or a rule that is not compiled. `matchPattern` is `null` when `lhs` or `environment` is, and also when `lhs` holds `#` (`anchor_position`), though `lhs` itself prints.
  - The text is read against the last stratum's (surface) table; a rule of an earlier stratum with its own character table is `unprintable` (`stratum_table_differs`).
  - `match`'s default `segmentTest:"engine"` tests the target by features, as rule application does, and its environment sides by identity, as rule application does, so on the words rule application reads, `match` of `matchPattern` reports every segment position an equal-length rewrite changes (the span of an environment match; `match` also reports positions the rule would leave unchanged). This is tested only for rules that are not `nonLocal` by `iterativeSpreading` or `iterativeLengthChange`, and metathesis is not in that test; an iterative rule feeds its own output to later sites and may rewrite positions `matchPattern` does not show.
  - `nonLocal` is `true` when the rule meets any criterion of `nonLocalCriteria`, listed in `nonLocalReasons`: `unboundedQuantifier` (a `( )*` group in any part), `alphaAgreement` (an alpha variable), `iterativeSpreading` (an iterative rewrite one of whose environment nodes can match a segment some subrule writes, so an earlier rewrite feeds a later site). `iterativeLengthChange` (an iterative rewrite one of whose subrules has a replacement of a different length than its left side: deletion, expansion or insertion, so later sites see shifted material). The criteria over-approximate: `nonLocal:true` means the rule may be non-local, `false` that none of the four applies. It is `null` (with empty reasons) for a rule that is not compiled.
  - `outputMatchPattern` is where the rule's output sits immediately after this rule applies: `matchPattern` with the target replaced by what the rule writes, ready to send as `match`'s `pattern` (with `boundaryMarker` when set). `matchPattern` finds the rule's input sites (`/t/ / [V] _` finds `kat`, not `kad`); `outputMatchPattern` finds its outputs (`/d/ / [V] _` finds `kad`), which is what a corpus holds. It is built from the engine's rewrite, never from text: for a same-length subrule each right-hand node's features overwrite the target node's (a bundle that adds to a target class merges both, `[+a +b]`; a literal segment `/x/` writes its own features), an alpha variable keeps the letter the environment binds, a deletion leaves only the environment (`/ left _ right`) and an insertion or length change writes the right-hand nodes alone. Where that is not one class, bundle or literal it is `null`, with `outputUnprintable` `[{part: "outputMatchPattern", reason}]` (`output_not_single_class`, `alpha_not_in_environment` for a variable only the target binds, `boundary_rewrite`, `metathesis`, or the `environment_not_printed`/`stratum_table_differs` of the rest of the row); it is never an approximation.
  - `outputMayBeAltered` says the pattern is the output right after this rule, not at the end of the derivation: later rules can rewrite those segments or split them. It is `true` when the rule is `nonLocal` (`nonLocal`), a rule that runs later, after it in its stratum or in a later stratum, has a target that can match a segment the pattern covers (`laterRewrite`, `rule` names it) or can insert between them (`laterInsertion`), or the pattern holds a word edge and a later stratum can add material beyond it (`wordEdgeLaterStratum`), or the rule's own written or rewritten segments can satisfy or overlap its own environment, or an insertion or length change has an environment side of two or more nodes, so one site's edit changes another site's (`selfInteraction`, `rule` names it, iterative or simultaneous), or a later stratum has a morphological rule that is not a plain prefix or suffix (a modified or split copy, an infix, compounding, a realizational rule) and can rewrite or split what this rule wrote (`laterMorphology`); `outputMayBeAlteredReasons` lists `{reason, rule}` and `--describe` lists `outputAlteredReasons`. The test is conservative: it asks only whether a later target can overlap a covered node, not whether the rule would apply. It is `true` in the common case; `false` means no later rule, later morphology or self interaction can touch the pattern's segments. It is `null` when there is no pattern.
- **Out of scope.** Test roots (`testRoots`) are not reported yet. Bleeding is not analysed, and feeding is only answered conservatively (`may_be_fed`): nothing says how rules interact beyond their stratum and ordinal.
- Errors (the request never ran): `bad_request`, `unknown_morpheme` (unknown entry, MSA not of the entry, or an `allomorphId` the morpheme does not own), `ambiguous_identity`, `authored_identity_unavailable`, `form_not_segmentable` (an allomorph form does not segment against its table; never reported as empty segments), `internal`. A work or deadline stop is a `result` with `complete:false`, never an error.

## Op `shadowedAllomorphs`: alternates that can never be selected

`interfaces.serveOps.shadowedAllomorphs` in `pangloss --describe` declares the op, its row fields, verdicts, reasons and contract. It is static and read-only: it analyses the loaded grammar and parses nothing, so parse results are unchanged.

```json
{"id":"s1","op":"shadowedAllomorphs","includeSelectable":false}
```

- Result: `complete`, `counts` (`shadowed`, `maybe_shadowed`, `selectable` over the morphemes with two or more alternates), `deadCounts` (`proven`, `maybe`, `live`), `alternates` and `deadContinuations`. `selectable` and `live` rows are listed only with `includeSelectable:true`.
- **Shadowing.** For each entry or affix rule, each alternate gets a `verdict`. `shadowed` is proven: an earlier alternate of the same morpheme is eligible wherever this one is and does not free-fluctuate with it, so no parse or generation selects this one; `blockers` names that earlier alternate. Example: an unconstrained passive `w` before `liw` shadows `liw`. `maybe_shadowed` lists the `reasons` that were not proven (`environment_subsumption_undecided`, `lhs_subsumption_undecided`, and so on, all declared in `--describe`). A proof uses only identical or empty conditions; a condition the engine does not model is never counted as covered.
- **Dead continuation.** A stem alternate is chosen by surface lookup, not by order, so it is reported separately: `continuation` is `proven` when no affix allomorph that could attach accepts the entry's rule features (inflection class) or stem name, and the allomorph cannot stand as a bare word; `maybe` when that last part is unproven (`reasons`); `live` otherwise. `blockedBy` names the requirement (`kind` `required_mpr`, `excluded_mpr` or `stem_name`, the `names`, and the affix `allomorphs` and `rules` that demand it), for example every following affix requires VC-B while the root is VC-A. Adding a null VC-A to VC-B converter makes the root live. A stem allomorph with an environment that needs a following segment (an alternate `rud` before `_ i`) also carries `neighbourRejections`, the requirements that turn away affixes that could supply that segment, even when other affixes accept the entry (the root is then `live`, listed without `includeSelectable`); when every such affix rejects it the row is `maybe` with reason `environment_needs_class_rejected_neighbour`. This check compares the affix's inserted text with the environment's text and ignores phonology, so it is never `proven`. `viaConverter` is true when some affix whose inserted shape has no real segment (a null affix, `+∅`, on either side) accepts the entry and changes its rule features, so the rejecting affixes may be reached after it; that affix's own input pattern and environments are not checked, so it may not apply to this allomorph. `neighbourRejections` is an upper bound (any affix that changes rule features, or whose shape is not modelled, counts as a possible supplier; the inserted text is compared with the environment's text, ignoring phonology) and lists only the five most frequent requirements.
- **A row is one HC allomorph**, not one FieldWorks allomorph: a FieldWorks affix allomorph with several environments becomes one HC allomorph per environment pass, and an entry or rule with several MSAs one per MSA. Every row carries `sourceAllomorphGuids` (the FieldWorks form guids the copy contains: one, or a prefix and a suffix for a circumfix copy) and `removableGuids`, `msaGuid` and `environments` (the authored text of the pass's environments; a suffix's left environment is compiled into its input pattern and is not listed). `shadowed` and `proven` are reported only when, for at least one guid of the row, every HC copy containing that guid has that verdict (`removableGuids` lists those guids); otherwise the row is `maybe_shadowed` or `maybe` with reason `source_allomorph_selectable_elsewhere` or `source_allomorph_live_elsewhere`, and `siblings` lists the copies that are not (`allomorphId`, `xmlKey`, `environments`). Act only on `removableGuids`. Never remove another guid from `sourceAllomorphGuids`. `proven` verdicts are not yet replayed against C# HC. Removing a FieldWorks allomorph listed in `removableGuids` is therefore safe for every pass and MSA; to drop one pass, remove that environment from the allomorph.
- Errors: `bad_request` (an unknown field or a non-boolean `includeSelectable`), `internal`.
- Not proven: a `maybe_shadowed` or `maybe` row says nothing either way, and no rule-level syntactic-feature or part-of-speech condition is modelled for dead continuations. The FST engine is not covered.

## Op `health`: the `grammar-health` report from the loaded grammar

`interfaces.serveOps.health` in `pangloss --describe` declares the op. One process that loaded a grammar (`--project` a `.fwdata`, `.fwbackup`, snapshot `.json` or grammar XML) answers `health` and `parse` requests in any order without loading again, which saves the cold start a separate `grammar-health` run pays. It is read-only and changes no parse.

```json
{"id":"h1","op":"health","shadowed":true,"includeSelectable":false,"storedAnalyses":"bounded"}
```

- Result: `{"id","type":"result","op":"health","report":{...}}`. `report` is the JSON document `pangloss grammar-health <project> [options]` prints for the same project and options, built by the same code (`diagnostics`, `stored_analysis_words`, and `shadowed_alternates` when asked), not a second report builder. Only ordering and wall-clock fields may differ.
- Request fields, all optional: `shadowed` (boolean, the command's `--shadowed`), `includeSelectable` (boolean, `--include-selectable`; requires `shadowed`), `storedAnalyses` (`"bounded"` default, `"all"` or `"off"`, the command's `--stored-analyses`).
- Not offered, because serve has no input for them or they write files: `--fst`, `--fw-project` (the project name comes from the `--project` path as the command derives it), `--snapshot-out`, `--facts-out` and `--facts-context`, `--log-guids` (the stderr log is not produced). They are listed under `notRun` in `--describe`.
- Stored analyses exist only for a `.fwdata` or `.fwbackup` project; for a snapshot or grammar XML the stored-analysis section is empty whatever `storedAnalyses` says.
- The report is computed from the grammar loaded at startup. A grammar edited on disk afterwards is not seen; start a new process.
- Unlike the command, which exits 1 on an error-level finding, an error-level finding is still a `result`; read the findings. The stored-analysis comparison parses wordforms on the request thread, so a `bounded` or `all` request can take as long as the command's comparison and blocks other requests meanwhile; `off` runs only the checks that do not parse.
- Errors: `bad_request` (an unknown field, a wrong type, an invalid `storedAnalyses`, or `includeSelectable` without `shadowed`), `internal`.

## Closing stdin

End of input on stdin exits 0 immediately, even in the middle of a parse: the in-flight and any queued requests are abandoned and get no response. Because the exit is immediate, an abrupt EOF can leave a partial last line on stdout; a host must discard an unterminated final line. Send `shutdown` for a graceful exit that answers `bye` after the current request.

## Versioning

`protocol` in the ready line and `interfaces.serveProtocol` in `--describe` change only for an incompatible wire change. Adding optional fields or error codes does not bump it.
