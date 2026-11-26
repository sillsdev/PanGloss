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

Version 1 selects an MSA, not a particular allomorph. If a `formGuid` is supplied, generation
returns `UnsupportedFormSelection` before synthesis; it never generates all forms while implying
the selected form constrained the result. A request is limited to 64 KiB and 64 selected
morphemes including the root. Generation uses the existing finite 50,000,000-unit Morpher
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
- It has the same limits as `serve` started with its defaults. There is no paradigm mode and no
  allomorph selection: `formGuid` is refused per row exactly as above.

Use the positional form for HC XML:

```text
pangloss generate <grammar.xml> <root-morpheme-id> [other-morpheme-id ...]
```
