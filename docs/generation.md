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

Use the positional form for HC XML:

```text
pangloss generate <grammar.xml> <root-morpheme-id> [other-morpheme-id ...]
```
