# Generation requests

`pangloss generate` supports the existing HC XML positional selectors and an exact authored
FieldWorks request mode:

```text
pangloss generate <snapshot.json|project.fwdata|project.fwbackup> --request <request.json>
```

Request mode requires compiled FieldWorks entry and MSA identities. It never falls back to
headwords, XML identifiers, display text, or numeric runtime IDs. HC XML grammars continue to use
their positional `<MorphemeId>` values.

## Version 1 request

```json
{
  "schemaVersion": 1,
  "root": {
    "entryGuid": "00000000-0000-0000-0000-000000000030",
    "msaGuid": "00000000-0000-0000-0000-000000000032",
    "msaClass": "MoStemMsa"
  },
  "morphemes": [
    {
      "entryGuid": "00000000-0000-0000-0000-000000000050",
      "msaGuid": "00000000-0000-0000-0000-000000000052",
      "msaClass": "MoInflAffMsa"
    }
  ]
}
```

Each object has only `entryGuid`, `msaGuid`, `msaClass`, and optional `formGuid`. GUIDs must be
canonical lowercase hyphenated text. `msaClass` accepts `MoStemMsa`, `MoInflAffMsa`,
`MoDerivAffMsa`, and `MoUnclassifiedAffixMsa`; the root must use `MoStemMsa`. The `morphemes`
array order is the generation order and is preserved; it is not surface order. In one Swahili
analysis, surface `SM + stem + FV + RELSUF` lists the selected affixes as `SM, RELSUF, FV`. No
general rule is promised for other grammars: when a request returns no forms, try another order. The request's closed shape is documented in
[generate-request.schema.json](generate-request.schema.json).

Version 1 selects an MSA. Without `formGuid` every allomorph of the root and of each stem
(compounding non-head) morpheme is tried, as `Morpher.GenerateWords` does. A `formGuid` on the root
or on a stem morpheme forces that form: only the seeds that use an allomorph carrying that MoForm
GUID run. Those seeds are synthesized exactly as in the unforced run and keep their `seedKey`, so the
forced result is a subset of the unforced one and the results of all the forms together equal it. A
`formGuid` that is not a form of that entry gives `unknown_form`. A `formGuid` on an affix gives
`UnsupportedFormSelection`: an affix is a rule the engine applies by trying all its subrules, so
there is no allomorph to pick, and it never generates all forms while implying the selected form
constrained the result. `--request` honours `formGuid` the same way and prints the forced run's words. The seed-product budget still counts the unfiltered product, so a forced request can still stop with `seed_product_exceeds_budget`.
Generation uses the existing finite 50,000,000-unit Morpher
allowance for the exact root/non-head allomorph product and synthesis work. If the product is too
large or the shared work allowance stops generation, the CLI returns a typed resource refusal and
emits no surfaces. Successful output is a list of forms and does not carry an explicit completion
witness.

## Batch requests

```text
pangloss generate <snapshot.json|project.fwdata|project.fwbackup> --requests <requests.jsonl|->
```

`--requests` reads one request per line (`-` reads stdin) and writes one response per line to
stdout, in input order. A line is a `pangloss serve` `generate` request object, so the same
fields apply (`id`, `op: "generate"`, `request`, and the optional `trace`, `maxSeeds`,
`maxTraceSteps`, `maxWork`, `wordTimeoutMs`), and a response is the row `serve` returns for it,
produced by the same code, plus a batch-only `line` field: the 1-based input line number. Correlate rows to requests by `line`, or by `id` when it is present (an error row for an unreadable line has `id` null). Details are in
[serve-protocol.md](formats/serve-protocol.md#the-generate-op). The project loads once.

- A blank (whitespace-only) line is skipped and has no row; `line` still counts it.
- A line that cannot run (malformed JSON, no string `id`, an `op` other than `generate`, a line
  over 1 MiB, a refused request such as `unsupported_form_selection`) gets an error row with the
  serve error `code`, and the batch continues. A cap or deadline is an ordinary result with
  `complete: false`.
- Exit status: 0 once every line is answered, error rows included, with a count of refused lines
  on stderr. Exit 1 for a bad argument, an unreadable input, a grammar that fails to load or a
  failed write, as for the other commands that report per-item outcomes as data.
- It has the same limits as `serve` started with its defaults. There is no paradigm mode.
  `formGuid` behaves per row exactly as above.

## Marking surfaces attested

`generate <project> --requests <file.jsonl|-> --attested <words.txt>` reads a word list (one word
per line, blank lines skipped, at most 1,000,000 words) and adds `"attested": true|false` to every
surface of every row: true when the surface is in the list. Spellings are compared in NFD, so a
composed and a decomposed form of one word agree. A row may carry its own list in the `serve` fields
`attestedWords` (array of strings) or `attestedWordsFile` (a path), at most one of them, and that
replaces `--attested` for the row. Without a list there is no `attested` field, so output is
unchanged. Matching is exact after NFD: there is no case folding, and a UTF-8 byte order mark at the start of a list file is ignored. The list only marks what generation produced; it never adds or removes a surface.
`--attested` is for `--requests`; `--request` prints plain words.

Use the positional form for HC XML:

```text
pangloss generate <grammar.xml> <root-morpheme-id> [other-morpheme-id ...]
```
