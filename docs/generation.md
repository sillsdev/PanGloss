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
array order is the generation order and is preserved.

Version 1 selects an MSA, not a particular allomorph. If a `formGuid` is supplied, generation
returns `UnsupportedFormSelection` before synthesis; it never generates all forms while implying
the selected form constrained the result. A request is limited to 64 KiB and 64 selected
morphemes including the root. Synthesis keeps the existing finite step cap and emits surface forms
on stdout. The output is a list of forms and does not carry a generation-completion witness.

Use the positional form for HC XML:

```text
pangloss generate <grammar.xml> <root-morpheme-id> [other-morpheme-id ...]
```
