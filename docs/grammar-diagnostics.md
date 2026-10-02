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

Reporting never changes a parse. These levels describe the existing importer and compiler outcomes;
consult each code's explanation for whether a restriction, alternative, or whole item was omitted.

## Levels

| Level | Meaning | Exit status |
|---|---|---|
| `error` | Known bad. The grammar is looser, ambiguous, or missing a morpheme. | Any error makes the command exit non-zero. |
| `warning` | A source condition or parser limitation needs review; the effect depends on the code. | Exit 0. |
| `info` | An expected omission or compaction; no change is needed if the item is intentionally unused. | Exit 0. |

## Why a diagnostic is an error

A diagnostic is an error when it falls into one of three classes. Each is worse than a warning
because the grammar no longer means what its author wrote, and nothing downstream can tell.

### 1. A restriction is dropped

The item still loads, but part of what limits where it applies is missing, so it applies in more
places than intended. The parser then explores more paths and returns extra or wrong analyses. This
is why partial morphemes are errors. A stem with no category, an inflectional affix with no template
slot, or an unclassified affix has incomplete restrictions. The missing-slot affix can survive as a partial rule; it is not universally absent. A partial morpheme can make parsing
far slower for every word, not just the words that use it.

The same harm comes from:

- an invalid environment that is ignored as a restriction. A missing environment can instead stop import, and an environment-build failure can skip an alternative. Consult the per-code explanation for the actual outcome.
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
- **Unsupported or approximated.** Metathesis rules are skipped by the snapshot compiler, even when the importer reported an approximation. Custom strata use the default layout, and a missing morpheme-boundary marker can fall back to a null boundary. Each code explains its particular outcome.
- **Unused or compacted.** An affix that is never reachable, an ad hoc rule that is never used, or a
  natural class nothing refers to: info.
- **Blocks compilation already.** Invalid source, duplicate or missing GUIDs, and an unresolved ad hoc
  prohibition on an active rule stop compilation under the default policy. They retain warning level; promoting them is unnecessary.

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

### JSON (schema version 4)

Written to `<out.json>`, or to stdout when no output path is given. Version 4 adds PanGloss-owned advice, CommonMark help and structured subject status. Consumers must explicitly support the schema version; PanGloss's reader rejects other versions and inconsistent counts or navigation. The current producer writes `locale: en`. Readers preserve the producing tag's text and nonblank locale rather than comparing wording with their own catalog.

Illustrative excerpt (background text is abbreviated):

```json
{
  "schema_version": 4,
  "locale": "en",
  "fieldworks_project": { "name": "fixture", "source": "fwdata_path" },
  "summary": [
    { "code": "grammar.msa.no-allomorphs", "group_name": "No usable entry allomorphs",
      "level": "error", "count": 1 }
  ],
  "diagnostics": [
    {
      "level": "error",
      "code": "grammar.msa.no-allomorphs",
      "group_name": "No usable entry allomorphs",
      "title": "No usable entry allomorphs",
      "origin": "import",
      "scope": "object",
      "explanation": "The lexical entry has no allomorph available to this analysis.",
      "help_path": "docs/diagnostics/grammar.msa.no-allomorphs.md",
      "help_body": "Check the entry and its stem or affix forms in Lexicon > Lexicon Edit.",
      "fieldworks_places": [{ "tool": "lexiconEdit", "field": "Lexeme Form" },
                            { "tool": "lexiconEdit", "field": "Allomorphs" }],
      "description": "Lexical entry 'kat' has no usable allomorphs.",
      "guidance": "In Lexicon > Lexicon Edit, inspect Lexeme Form, Allomorphs and Morph Type. Resolve the specific allomorph loading findings before adding a new form.",
      "subjects": [
        {
          "kind": "LexEntry",
          "status": "object",
          "field": "Allomorphs",
          "source_class": "LexEntry",
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
| `locale` | Language of the producer-owned text; the current catalog is `en`, and readers preserve other locales. |
| `title` | Per-code title, also supplied as `group_name` for existing consumers. |
| `explanation` | PanGloss-owned account of the code's actual behavior; null only for an unregistered code. |
| `guidance` | PanGloss-owned next step naming a verified FieldWorks tool/field, or explicitly explaining why no verified correction exists. Null for an unregistered code. |
| `help_path` | Stable repository-relative per-code Markdown page; null for an unregistered code. Pin it to the PanGloss release tag used for this report. |
| `help_body` | Optional CommonMark background text, without HTML or front matter. Motif can render it directly. |
| `fieldworks_places` | Verified tool IDs and user-visible field labels. A blank field means only the tool is verified. An empty list means no fixed edit destination is established; it does not mean the finding was repaired. |
| `scope` | `object` when a live source is named, otherwise `unresolved_reference` or `project_settings`, derived from subjects. |
| `subjects[].kind` | Normalized FieldWorks class of the item. Schema 4 has a closed set of kinds; a new normalized kind requires a schema bump. Unrecognized XML classes use `Unknown` and retain `source_class`. |
| `subjects[].status` | `object`, `unresolved_reference`, or `project_settings`. An unresolved reference retains its GUID but has no live link. A project setting has no object GUID. |
| `subjects[].field` | Source field that contains the problematic reference or setting, when known. |
| `subjects[].source_class` | Original XML class when available, preserving subclasses and unrecognized classes. |
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

- Import codes: `rust/crates/pg-snapshot/src/warning_metadata.rs`, in the exhaustive `import_warning_level` match. Every error must carry guidance.
- Check codes: `check_diagnostic_metadata` in `rust/crates/pg-grammar/src/grammar_health.rs`. This
  is the only place a check code's level is set.

Then run the managed `pg-grammar` checks and unit tests (`rust/tools/pg.ps1 -Mode check -Package pg-grammar`, then `-Mode quick -Package pg-grammar`). The
`diagnostics_reference_is_current` test rewrites
[grammar-diagnostics-reference.md](grammar-diagnostics-reference.md) and fails once, so the change
shows up in the diff. The `every_registered_code_has_a_current_version_pinnable_page` test does the same for all per-code pages. Review and commit all regenerated files with the change. Update this document when a new
class of error is introduced.

This report is separate from `pangloss fst-health`, which asks whether a compiled FST may be
published. That report blocks publication of an FST built from a grammar with partial morphemes
(`PGF0016`) independently of these levels.

## Version-pinned help

Every registered code has a generated page at `docs/diagnostics/<code>.md`, headed by the code itself. Motif can build this URL from the parser's pinned tag and the finding's `help_path`:

```text
https://github.com/sillsdev/PanGloss/blob/<tag>/docs/diagnostics/<code>.md
```

The file path is the stable target for dotted codes, whose GitHub heading anchor drops punctuation. For example, `grammar.allomorph.not-a-rule-form.md` also renders the heading anchor `#grammarallomorphnot-a-rule-form`; consumers should use the file path. The reference index links every page. Both the index and pages are generated from the same Rust catalog as runtime advice, with tests that regenerate stale files and fail for review. No catalog JSON export, CLI subcommand or release asset is required.

Missing references are not live objects. For example, a snapshot warning can name the owning allomorph (`status: object`, `field: PhoneEnv`) and the absent environment (`status: unresolved_reference`, its referenced GUID, and `fieldworks.reason: unresolved_reference`). The owner remains navigable when its verified tool and project name are available. Project settings use a `Project` subject with `status: project_settings`, no GUID, and an explicit unavailable reason. Neither case implies an exact set of affected words.

English content is owned by PanGloss and keyed by stable code. Motif displays `description`, `explanation`, `guidance`, and optional `help_body`; it does not write substitute remedies. A future locale requires an explicit catalog and locale value, rather than inferring translated advice from messages. The current producer supplies null advice and an empty place list for unknown codes. Readers preserve advice supplied by a newer producer even when the code is unknown locally. Missing subjects, inconsistent status or navigation, conflicting stable metadata for one code, and incorrect summary counts are rejected. Runtime constructors normalize navigation from subject identity and the selected project; wire readers validate it without silently repairing it.
