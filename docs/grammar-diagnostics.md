# Grammar diagnostics

`pangloss grammar-health <grammar> [<out.json>] [--fw-project <project>] [--log-guids]` reports
problems in a grammar that its author should fix in FieldWorks. This document explains how each
diagnostic's level is chosen and what the report looks like. For every code, with its level and
fix, see the generated [grammar-diagnostics-reference.md](grammar-diagnostics-reference.md).

The diagnostics come from two sources:

- **Checks** (`hc-*` codes) inspect the compiled grammar. They are ported from HermitCrab's
  `GrammarHealthChecker` (sillsdev/machine PR 475). The one change is that C#'s single
  `hc-partial-morpheme` is split into one code per reason.
- **Import codes** (`fwdata.*`, `grammar.*`, `conversion.*`, and so on) are raised while a FieldWorks
  project is converted into a grammar.

Reporting never changes a parse. PanGloss keeps or drops exactly what HermitCrab's loader keeps or
drops; the level only decides how loudly that is reported.

## Levels

| Level | Meaning | Exit status |
|---|---|---|
| `error` | Known bad. The grammar is looser, ambiguous, or missing a morpheme. | Any error makes the command exit non-zero. |
| `warning` | FieldWorks should change, but the grammar still behaves as authored, or loses only one alternative. | Exit 0. |
| `info` | Something was left out on purpose. Nothing to fix. | Exit 0. |

## Why a diagnostic is an error

A diagnostic is an error when it falls into one of three classes. Each is worse than a warning
because the grammar no longer means what its author wrote, and nothing downstream can tell.

### 1. A restriction is dropped

The item still loads, but part of what limits where it applies is missing, so it applies in more
places than intended. The parser then explores more paths and returns extra or wrong analyses. This
is why partial morphemes are errors. A stem with no category, an inflectional affix with no template
slot, or an unclassified affix can attach almost anywhere. A single partial morpheme can make parsing
far slower for every word, not just the words that use it.

The same harm comes from:

- an environment that is missing or invalid. The allomorph loses that environment, and with none
  left it applies everywhere.
- an exception feature, inflection class, stem name or entry inflection type on an analysis or
  allomorph that does not resolve. That restriction is omitted.
- a phoneme feature value that does not resolve or is not supported. The phoneme loses that feature,
  so natural classes and rules match it wrongly.
- a natural-class feature constraint that does not resolve or is not supported. The class matches
  more segments than defined.
- a compound rule's side category or exception feature that does not resolve.
- a phonological rule's feature constraint or rule feature that does not resolve.
- a character inferred as a segment with empty features while feature rules exist.

### 2. Two items become indistinguishable

- **Representation collisions** (`grammar.phoneme.nfd-collision`, `grammar.boundary.nfd-collision`):
  two phonemes or boundary markers normalize to the same written form, so one is skipped or the two
  are confused.
- **Duplicate feature bundles** (`hc-duplicate-feature-bundle`): two segments have identical feature
  values. Lookup by features cannot tell them apart, and a rule that rewrites features can output
  either one, which gives wrong or duplicated analyses. The check only fires when the grammar has a
  phonological feature system; with none, every bundle is empty and nothing is reported.

### 3. A whole morpheme is lost

`fwdata.no-usable-allomorphs`, `grammar.msa.no-allomorphs`, `grammar.msa.no-rule-form-allomorphs` and
`grammar.circumfix.missing-half` mean an entry, analysis or circumfix has nothing left to load. Every
word that needs it fails to parse. This class loses parses rather than slowing parsing down, but it is
equally known bad.

## Why other diagnostics are not errors

- **One alternative is lost, others survive.** For example, one allomorph is unsegmentable but its
  siblings load. That is a warning: the morpheme still parses. It becomes an error once no allomorph
  survives (class 3).
- **An approximation.** For example, metathesis reduced to a swap, custom strata using the default
  layout, or a missing morpheme-boundary marker falling back to a null boundary. The behaviour is
  documented and deliberate: warning.
- **Unused or compacted.** An affix that is never reachable, an ad hoc rule that is never used, or a
  natural class nothing refers to: info.
- **Blocks compilation already.** Invalid source, duplicate or missing GUIDs, and an unresolved ad hoc
  prohibition on an active rule stop compilation under the default policy, so no level is needed.

Some codes stay warnings until they are split or verified:

- `grammar.stem-name.empty-regions` and `grammar.stem-name.build-failed` loosen an analysis only when
  an analysis refers to that stem name. The level is fixed per code, so promoting them would also flag
  unused stem names.
- `fwdata.missing-required-field` and `fwdata.unrecognized-enum-value` behave differently at different
  sites. An unknown adjacency value defaults to "anywhere" (a dropped restriction), but an unknown
  rule direction defaults to left-to-right (an approximation). Each needs splitting into two codes
  before either half can be an error.
- `hc-undeclared-segment` stays a warning until the parser's behaviour on an undeclared segment is
  verified.

## Report format

### JSON (schema version 3)

Written to `<out.json>`, or to stdout when no output path is given. Version 3 added the `error`
level; a reader must refuse any other `schema_version`.

```json
{
  "schema_version": 3,
  "fieldworks_project": { "name": "fixture", "source": "fwdata_path" },
  "summary": [
    { "code": "grammar.msa.no-allomorphs", "group_name": "No usable entry allomorphs",
      "level": "error", "count": 2 }
  ],
  "diagnostics": [
    {
      "level": "error",
      "code": "grammar.msa.no-allomorphs",
      "group_name": "No usable entry allomorphs",
      "origin": "import",
      "description": "Lexical entry 'kat' has no usable allomorphs.",
      "guidance": "In Lexicon > Lexicon Edit, add or correct an allomorph for the named lexical entry.",
      "subjects": [
        {
          "kind": "LexEntry",
          "title": "kat",
          "subtitle": null,
          "guid": "00000000-0000-0000-0000-000000000030",
          "internal_id": null,
          "fieldworks": {
            "status": "available",
            "guid": "00000000-0000-0000-0000-000000000030",
            "tool": "lexiconEdit",
            "url": "silfw://localhost/link?database%3Dfixture%26tool%3DlexiconEdit%26guid%3D00000000-0000-0000-0000-000000000030%26tag%3D"
          }
        }
      ]
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `fieldworks_project.name` | Project used to build FieldWorks links: `--fw-project`, or the stem of a `.fwdata` path. `source` is `argument` or `fwdata_path`, or null when neither applies. |
| `summary` | One row per code, with its count. |
| `level` | `error`, `warning` or `info`. |
| `code` | Stable identifier. Filter, suppress and test on this, never on `description`. |
| `group_name` | Short, stable human label for the code. |
| `origin` | `check` for `hc-*` codes, `import` for conversion codes. |
| `description` | Human-readable message naming the item. Wording may change. |
| `guidance` | Where in FieldWorks to fix it, or null. |
| `subjects[].kind` | FieldWorks class of the item. |
| `subjects[].title` / `subtitle` | The name a linguist sees. This is the only identity a report should display. |
| `subjects[].guid` | The item's own FieldWorks GUID, when known. |
| `subjects[].internal_id` | Compiled-grammar id, for tooling only. |
| `subjects[].fieldworks` | Either `{"status":"available","guid","tool","url"}`, a link that opens the item in FieldWorks, or `{"status":"unavailable", ...}` with a reason. |

### Log (stderr)

One line per diagnostic:

```text
error [No usable entry allomorphs] lexical entry: kat [silfw://localhost/link?...] - Lexical entry 'kat' has no usable allomorphs.
```

The fields are level, group name, subject kind, subject title with its FieldWorks link (or the
reason it is unavailable), then the description. `--log-guids` adds `[guid ...]` after each title.
The run ends with a count line, then a failure line when there are errors:

```text
grammar-health complete: 13 diagnostic(s) (3 error(s), 10 warning(s), 0 info)
pangloss grammar-health: 3 error(s); fix them before parsing
```

The report is always written in full before the command fails.

## Changing a level

- Import codes: `rust/crates/pg-snapshot/src/warning_metadata.rs`, using the `error(...)`,
  `warning(...)` or `info(...)` constructor. Every error must carry guidance.
- Check codes: `check_diagnostic_metadata` in `rust/crates/pg-grammar/src/grammar_health.rs`. This
  is the only place a check code's level is set.

Then run the `pg-grammar` tests (`rust/tools/pg.ps1 -Mode test -Package pg-grammar`). The
`diagnostics_reference_is_current` test rewrites
[grammar-diagnostics-reference.md](grammar-diagnostics-reference.md) and fails once, so the change
shows up in the diff. Commit the regenerated file with the change. Update this document when a new
class of error is introduced.

This report is separate from `pangloss fst-health`, which asks whether a compiled FST may be
published. That report blocks publication of an FST built from a grammar with partial morphemes
(`PGF0016`) independently of these levels.
