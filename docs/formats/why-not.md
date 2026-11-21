# why-not (`pangloss.whynot.v1`)

`pangloss why-not` answers one question: *why was this word not parsed along these morphs?* You
give it a word and one or more **candidates** (a candidate is the morph sequence you expected, in
surface order). It does not search every path. For each candidate it reports whether the word
parses along it, and if not, the **first failing constraint**, typed, in plain words, with the
grammar object the failure is about, so a tool can draft a fix.

> **Status: slice 2 (Motif #386).** The command answers `accepted`, `rejected` or `unknown` by
> reparsing the word with the expected morphs pinned. A rejection names the first failing
> constraint for these kinds: `no_morph_match`, `environment_unmet` and `adhoc_prohibition`. `no_morph_match` is answered only when the trace shows no other cause, or when the word does parse with the same morphemes in another order or with other allomorphs (the `plain` sentence says which). Any
> other failure, or a search that cannot finish, answers `unknown` with a `stop` reason, never a
> guess. The examples below show the shape of each answer; ids, GUIDs and wording in them are
> invented. Kinds beyond the three above are later slices. Slice 2 declares the capability (see Detecting why-not) and makes every id in a report a `grammar.xml` id or a typed absence (see Ids).

Schemas: [`why-not-v1/report-v1.schema.json`](why-not-v1/report-v1.schema.json) (the report) and
[`why-not-v1/expect-v1.schema.json`](why-not-v1/expect-v1.schema.json) (the `--expect-json` input).
The vocabulary is the one in [rejection diagnostics v1](rejection-trace-v1.md): `hc_reason` is a C#
HermitCrab `FailureReason` name, grammar objects carry `type`, `id` and FieldWorks `guid`, and
`entry_guid` plus `msa_guid` are the identities the v1 selector uses. Morphemes and stem names are
named by their `grammar.xml` id (the `id` attribute).

## Command

```
pangloss why-not <grammar> <word> (--expect <spec> | --expect-guid <spec> | --expect-json <file|->)...
                 [--allomorphs <spec>] [--json] [--work-cap N|unbounded] [--word-timeout-ms N]
```

* `--expect <spec>` is one candidate. Repeat the flag for several candidates; each is judged
  separately.
* `--expect-guid <spec>` is a candidate by FieldWorks identity. Repeatable, and it may be mixed with
  `--expect`.
* `--expect-json <file|->` reads a JSON array of candidates (the shape below), from a file or from
  standard input (`-`). It cannot be combined with `--expect` or `--expect-guid`. Motif is expected
  to use this form.
* `--allomorphs <spec>` pins allomorphs by `grammar.xml` allomorph id (see Candidate syntax).
* `--json` writes the report below to standard output; without it a short text summary is written.
* `--work-cap`, `--word-timeout-ms` bound the search for each concrete morph sequence (a hit
  produces `unknown` with `stop: "work_cap"` or `"deadline"`).
* `pangloss why-not <grammar> <queries.jsonl> <output.jsonl>` runs the shared conformance suite's
  why-not queries (see Suite runner).

### Candidate syntax

A candidate lists **positions** in surface order. A position lists one or more **choices**; each
choice is tried, and the report says which was used.

| Symbol | Meaning | Example |
|---|---|---|
| `,` | next position | `kib,ak,ek` is three positions |
| `\|` | another choice at the same position | `kib,ak\|ik,ek` offers `ak` or `ik` at position 1 |
| `:` | (`--expect-guid` only) joins `entryGuid` and `msaGuid` | `E:M,E2:M2` |
| `@` | pins the allomorph of a choice, by its `grammar.xml` allomorph id | `kib,ak@ak1,ek` |

A morpheme id is the `id` attribute of its lexical entry or morphological rule in `grammar.xml`
(`MorphemeInfo.xml_key` in the engine). Ids are XML NCNames, so `,` `|` `@` `:` cannot occur in one,
and GUIDs (hex digits and `-`) cannot clash with the separators either. Some morphemes share one id
(a main entry and its variants share an MSA): such an id is an input error that points at
`--expect-guid`, whose `entryGuid:msaGuid` choice that matches several morphemes becomes several
choices at that position. Dense numeric ids (`morphemeId` in an analysis) are not accepted.

`--expect-json` input is a JSON array of candidates; each candidate an array of positions; each
position a non-empty array of choices `{"morpheme": "id"?, "guid": {"entry_guid", "msa_guid"}?}`. A choice needs a `morpheme` id, a `guid`, or both.
A choice with only a `guid` (both `entry_guid` and `msa_guid`) is resolved exactly as an `--expect-guid` choice is: it becomes one choice per morpheme
sharing that entry and MSA, and an id candidate and a guid candidate may share one call. With both, the guid must belong to the
morpheme the id names, or the call is an error (the id does not win silently). `allomorph` is optional and pins the allomorph by its `grammar.xml` id; with a guid-only
choice it needs the guid to name a single morpheme. Unknown fields are refused on input. The example below shows an id choice; a guid-only choice is
`{"guid": {"entry_guid": "...", "msa_guid": "..."}}`:

```json
[
  [
    [
      {
        "morpheme": "kib"
      }
    ],
    [
      {
        "morpheme": "ak"
      },
      {
        "morpheme": "ik"
      }
    ],
    [
      {
        "morpheme": "ek"
      }
    ]
  ],
  [
    [
      {
        "morpheme": "kib"
      }
    ],
    [
      {
        "morpheme": "ak"
      }
    ]
  ]
]
```

### Input errors

These are errors, not failure kinds: nothing is written to standard output and the exit status is
1, with the reason on stderr. They are an empty word, no candidate, a candidate or position with
nothing in it, an unknown morpheme id, a morpheme id shared by several morphemes, an `--expect-guid` that
matches no morpheme, an empty id, malformed JSON, a bad flag value, an allomorph id the grammar
does not have, one shared by several allomorphs, or one that belongs to another morpheme, and a
supplied guid that is not the morpheme's, a JSON choice with neither a `morpheme` nor a complete `guid`, and a guid-only choice that pins an allomorph while naming several morphemes. A grammar that fails to load is also exit 1. Everything else, including a word that is
rejected or `unknown`, exits 0.

## Report

The report **always** has `results`, even for a single candidate, so one shape serves every call.
`results` is in input order. Each entry echoes its candidate as `expect`.

| Field | Meaning |
|---|---|
| `schema` | `pangloss.whynot.v1` |
| `word` | the word as given |
| `results[]` | one entry per candidate, in input order |
| `results[].expect` | the candidate as parsed: positions, each a list of `{morpheme, guid?}` choices (the same shape as `--expect-json`; `allomorph` is present when the caller pinned one) |
| `results[].result` | `accepted`, `rejected` or `unknown` |
| `results[].analysis` | when accepted: `{analysis, used}`. `analysis` is `{morphs, root_index, category, guessed}`: the derivation by authored ids (every morph in surface order as `{morpheme, allomorph, guid?}`, the root's index in `morphs`, the part of speech's `grammar.xml` id or null); it carries no engine ordinal. `used` has one `{morpheme, allomorph?}` per position, the choice that produced it. Otherwise null |
| `results[].failure` | when rejected: the first failure (below). Otherwise null |
| `results[].stop` | when unknown: `not_implemented`, `work_cap`, `deadline`, `failure_not_expressible` (a failure was found but cannot fill a required `failure` field; the C# tool emits this, so both engines share this schema), `no_root` (no position is a lexical entry), `unsupported_morpheme` (a position is neither a lexical entry nor an affix rule), `compounding_unsupported` (several are), `word_not_segmentable` (the word has no segmentation the parser can start from), `unclassified_failure` (HC stopped the derivation for a reason this release does not name: a category or feature clash, an empty required slot, a family block, a stem name, a `require` co-occurrence rule, a surface mismatch) or `choices_capped` (the candidate expands to more concrete sequences than the search tries). Otherwise null |
| `results[].stop_note` | when unknown: one sentence saying what was reached. Optional |
| `best` | index into `results` of the best candidate, or null |

**`best`** is the first accepted candidate. If none is accepted, it is the rejected candidate that
survived the most checks, counting checks in the C# order (analysis side, synthesis gates,
word validity, per-allomorph checks, surface match; the order of `failure.kind` below), and for the
same check the later `position`; a tie goes to the earliest candidate in input order. If every
candidate is `unknown`, `best` is null. A candidate that is `unknown` never wins over one that was
rejected.

### `failure` (field names follow the shared suite: snake_case)

| Field | Meaning |
|---|---|
| `kind` | one of the kinds below |
| `plain` | one English sentence. On a grammar built from a FieldWorks model it names a morph by its form text and gloss (`-ak 'PL'`, the form alone when there is no gloss) and never quotes a guid or a compiled `<formguid>#n` key; on a `grammar.xml` it names ids, as the table below says. Ids stay in the structured fields |
| `position` | 0-based position in the candidate of the morph whose check failed, or null when the failure is about the whole word. For `environment_unmet` and `adhoc_prohibition` it is the position of `morphs[0]`, the allomorph or morpheme that owns the environment or the rule; for a prohibition the partner is only in `morphs[1]` and `members`, so `position` does not say where it sits. Never a guess: null when the morph cannot be placed in the candidate |
| `hc_reason` | the engine's reason, the C# `FailureReason` name when HC reports it, or null when PanGloss inferred the failure |
| `provenance` | `hc_reported` (C# raises this failure) or `derived` (PanGloss inferred it; C# has no event) |
| `object` | the grammar object a fix would touch, for kinds without a dedicated field above: `{type, id, guid}`; `type` is one of `environment`, `allomorph`, `affix`, `slot`, `template`, `rule`, `class`, `stem_name`, `feature`. `id` is the `grammar.xml` id; `guid` is the FieldWorks identity, filled in only when the grammar came from a FieldWorks model (a `.fwdata` snapshot). `environment_unmet` fills it with the allomorph the environment belongs to (the same id as `environment.allomorph_id`; `guid` is that allomorph's `MoForm` guid); `adhoc_prohibition` fills it with the `rule` (`guid` is the `MoAdhocProhib` guid, `id` stays null); `stem_name` carries `stem_name` instead; null when no single object applies |
| `morphs` | the morph(s) involved, as `{morpheme, allomorph?}` (the shape of an expected-parse element; `allomorph` is omitted when unknown); for a position with several choices, the choice used. For `adhoc_prohibition` it is the pair, the rule's primary morph first and the offending partner second; for `stem_name` the root allomorph |
| `context` | `environment_unmet` only: `{left, right, provenance}`, the actual text around the allomorph in the form HC checked, which is the synthesized form and can differ from the word when phonological rules apply (`plain` then says so); otherwise null. HC does not report it, so an engine or adapter computes it and `provenance` is always `derived`, even when the failure itself is `hc_reported` |
| `environment` | `environment_unmet` only: `{allomorph_id, guid?, text?, index?}`. When one environment failed it is that environment: its 0-based `index` in the allomorph's list, the FieldWorks environment `guid` and the authored `text` (such as `/ _ #`), the last two only on a snapshot-route grammar. When several failed (an allomorph's environments are any-of, so all of them failed) it names only the allomorph and `environments` lists them. `index` is omitted when unknown, never a fake 0. Otherwise null |
| `environments` | `environment_unmet` only, when more than one environment failed: one `{allomorph_id, guid?, text?, index?}` per failed environment, in list order; omitted when one failed. `plain` lists the texts |
| `members` | `adhoc_prohibition` only: every morph the prohibition names, primary first, each `{morpheme, allomorph?, guid?, form_guid?}` (`morphs` keeps just the clashing pair); omitted otherwise. An allomorph prohibition lists the allomorphs, a morpheme prohibition the morphemes |
| `stem_name` | `stem_name` only: the stem name's `grammar.xml` id. Otherwise null |
| `features` | `feature_clash` only: an array with one `{path, required, actual}` per mismatching feature path (the word's feature state against one requirement); an empty array otherwise |
| `alternatives` | how many other derivations of the same concrete morph sequence also fail (any reason), or null when none; derivations of other sequences of a multi-choice candidate are not counted |

Kinds, in the order C# reaches them (`check` order, earliest first): `no_morph_match`, `slot_order`,
`required_slot_empty`, `feature_clash`, `other_gate`, `stem_name` and `inflection_class`,
`adhoc_prohibition`, `environment_unmet`, `surface_mismatch`. "First" is PanGloss's definition,
not C#'s search order (C# explores every path): among derivations that honour the candidate, take
the one that survives the most checks and report its first failing check.

Real `hc_reason` values are C# `FailureReason` names; `slot_order` has none, because C# has no
event for it (a wrong order simply never offers the morph), so it is always `derived`.
`inflection_class` has no HermitCrab concept of its own; it is reported through the stem name or
MPR feature that carries it.

### Provenance, engine-specific kinds and GUIDs

* `failure.provenance` is `hc_reported` when C# raises the failure and `derived` when PanGloss
  inferred it. Parts of an `hc_reported` failure that HC does not report (`context`) carry their own
  `derived` mark.
* The `failure` field names `kind`, `hc_reason`, `morphs`, `environment`, `stem_name` and `features`,
  and the `{morpheme, allomorph?}` element shape, follow the shared conformance suite's why-not rows
  (fieldworks-parser-conformance PR 51, PROTOCOL.md section 11). Ids are the text of `grammar.xml` ids,
  as in the suite. `allomorph` ids are `grammar.xml` allomorph ids. On a project or snapshot grammar they are opaque per-grammar keys: do not store them or map them to FieldWorks.
* Conformance fixtures assert only the kinds C# itself reports: `environment_unmet`,
  `feature_clash`, `adhoc_prohibition` and `stem_name`. The other kinds are **engine-specific**:
  `no_morph_match`, `slot_order`, `required_slot_empty`, `surface_mismatch`, `inflection_class` and
  `other_gate` (and the `unknown` result) are kept, documented as such, and are not compared with C#.
* The `grammar.xml` id is the required identity everywhere. `entry_guid` and `msa_guid` (in a choice's `guid`) and
  `object.guid` are optional and are filled in when the caller has the FieldWorks model, as Motif does.

### Ids

Every identifier in a report is a `grammar.xml` id (the `id` attribute) or a typed absence. No
engine ordinal (a morpheme, allomorph, rule or part-of-speech index) appears in a field or in
`plain`.

| Object | Where it appears | Id |
|---|---|---|
| morpheme (entry or affix rule) | `morphs[].morpheme`, `used[].morpheme`, `analysis.morphs[].morpheme` | `grammar.xml` id of the entry or rule; `plain` adds its `MorphemeId` text or gloss in parentheses |
| allomorph | `morphs[].allomorph`, `object.id`, `environment.allomorph_id` | `grammar.xml` allomorph (or subrule) id |
| entry and MSA | `morphs[].guid.{entry_guid, msa_guid}` | FieldWorks GUIDs when the grammar came from a FieldWorks model; the whole `guid` is omitted for a hand-written grammar.xml, which has neither |
| form | `morphs[].form_guid`, `members[].form_guid`, `object.guid` on `environment_unmet` | the `MoForm` guid of the allomorph (the first form of a circumfix); omitted for grammar.xml |
| part of speech | `analysis.category` | `grammar.xml` part-of-speech id |
| co-occurrence rule | `object` `{type: "rule", id: null, guid}` | `guid` is the `MoAdhocProhib` guid on a snapshot-route grammar and null for grammar.xml, which gives these rules no id. It is found by the primary and the full member set; when two prohibitions have the same primary and members the trace cannot say which fired, so `guid` is null. `morphs[0]` is the rule's owner |
| environment | `environment.index`, `environment.guid`, `environment.text` | `index` is the 0-based position in its allomorph's list; `guid` and `text` come from the FieldWorks model only, never from grammar.xml: its `<Environment>` has no id, and an `id` authored there is never reported as a guid |
| rule (phonological or morphological), template, slot | not reported | no failure kind that names them is answered yet (they are `unknown / unclassified_failure`) |

On a `grammar.xml`, `plain` names an allomorph by its id (`aSal`) and a morpheme by its id with its text
(`eSal ("SAL")`); on a FieldWorks snapshot it names a morph by form text and gloss instead (`ta- 'PA' forbids da 'run'`). The suite runner's morph lines name the morpheme by `MorphemeId` text as the suite
does, and carry `entry_guid` and `msa_guid` as flat fields when known; its `plain` shows both
spellings.

### Detecting why-not

A host reads `pangloss --describe` and requires, before launching why-not:

* a `why-not` row in `commands`, and
* `interfaces.whyNot` with `schema` equal to `pangloss.whynot.v1`, `ids` equal to `grammar-xml` (the
  promise above), `kinds` (the failure kinds answered today, taken from the engine; every other
  kind comes back `unknown`, so a host can say so up front) and, to use the queries form,
  `suiteRows` true, and `expectGuidChoices` true when `--expect-json` must accept a choice that carries only a `guid` (a
  build without the key needs a `morpheme` id on every choice, so refuse it).

An older PanGloss has no `interfaces.whyNot` (before this capability), or no `why-not` row at all, and
the host must refuse and say the installed PanGloss cannot answer why-not. There is no `capabilities`
subcommand and `--version` prints only the version, so `--describe` is the one place to look.
PanGloss ships no MCP server; Motif's own tool list is where a host exposes the answer.

### Compatibility

Consumers ignore fields they do not know: the report, `failure` and its parts may gain fields in
a later slice without a new schema version. A choice and its `guid` object may carry extra fields. Only the `--expect-json` input is strict.

### Ordering and what is promised

HC short-circuits, so the contract promises **only the first failure per derivation**: later
failures of the same derivation are not reported. A candidate with several choices has several
derivations; its `failure` is the first failure of the furthest-progress derivation, and
`alternatives` counts the others that also fail. Derivations, sequences and candidates are ranked by one
key: the index in morph order of the allomorph whose check failed (a failure about the whole word
ranks before any allomorph, the surface match after all), then the check's place in `Allomorph.IsWordValid`'s order. The order across allomorphs is PanGloss's morph order; C# iterates the root, then affixes in rule-application order. An allomorph failure that cannot be placed in the word makes the candidate `unknown`. A repeated affix (one rule applied twice) is one position per occurrence in the pinned sequence; a sequence that names it once does not match a derivation that applies it twice, so the answer is `no_morph_match` only if nothing else shows, else `unknown`. `results` itself stays in input order, and `best` points at
the furthest-progress candidate. When any sequence of a candidate is `unknown`, the candidate is `unknown`: it could hide an acceptance.

## Examples (illustrative)

Ids, GUIDs and wording are invented (synthetic) and use a made-up language. They validate against
the report schema.

### `unknown`

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kibakek",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "unknown",
      "analysis": null,
      "failure": null,
      "stop": "not_implemented"
    }
  ],
  "best": null
}
```

### `accepted`

The word parses along the candidate. Position 1 offered two choices; `used` says morpheme `ak` was the one that worked.

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kibakek",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          },
          {
            "morpheme": "ik"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "accepted",
      "analysis": {
        "analysis": {
          "morphs": [
            {
              "morpheme": "kib",
              "allomorph": "kib-1"
            },
            {
              "morpheme": "ak",
              "allomorph": "ak-1"
            },
            {
              "morpheme": "ek",
              "allomorph": "ek-1"
            }
          ],
          "root_index": 0,
          "category": "posN",
          "guessed": false
        },
        "used": [
          {
            "morpheme": "kib"
          },
          {
            "morpheme": "ak"
          },
          {
            "morpheme": "ek"
          }
        ]
      },
      "failure": null,
      "stop": null
    }
  ],
  "best": 0
}
```

### `rejected`, one example per `kind`

The candidate is `kib,ak,ek` unless the example says otherwise.

#### `no_morph_match`

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kibakek",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "rejected",
      "analysis": null,
      "failure": {
        "kind": "no_morph_match",
        "plain": "No morph matches at position 1: nothing in the word can be the expected suffix `ak`.",
        "position": 1,
        "hc_reason": null,
        "provenance": "derived",
        "object": {
          "type": "affix",
          "id": "ak",
          "guid": "00000000-0000-0000-0000-000000000015"
        },
        "morphs": [
          {
            "morpheme": "ak"
          }
        ],
        "context": null,
        "environment": null,
        "stem_name": null,
        "features": [],
        "alternatives": null
      },
      "stop": null
    }
  ],
  "best": 0
}
```

#### `environment_unmet`

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kibakek",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "rejected",
      "analysis": null,
      "failure": {
        "kind": "environment_unmet",
        "plain": "The allomorph `ek-a` of suffix `ek` needs a left context of a back vowel, but the word has `i` on the left and `k` on the right.",
        "position": 2,
        "hc_reason": "Environments",
        "provenance": "hc_reported",
        "object": null,
        "morphs": [
          {
            "morpheme": "ek"
          }
        ],
        "context": {
          "left": "kib",
          "right": "k",
          "provenance": "derived"
        },
        "environment": {
          "allomorph_id": "ek-a",
          "index": 0
        },
        "stem_name": null,
        "features": [],
        "alternatives": 1
      },
      "stop": null
    }
  ],
  "best": 0
}
```

#### `slot_order`

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kibakek",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "rejected",
      "analysis": null,
      "failure": {
        "kind": "slot_order",
        "plain": "Suffix `ek` is in slot 3 and suffix `ak` is in slot 2, so `ek` cannot come before `ak`.",
        "position": 2,
        "hc_reason": null,
        "provenance": "derived",
        "object": {
          "type": "slot",
          "id": "slot3",
          "guid": "00000000-0000-0000-0000-000000000003"
        },
        "morphs": [
          {
            "morpheme": "ek"
          },
          {
            "morpheme": "ak"
          }
        ],
        "context": null,
        "environment": null,
        "stem_name": null,
        "features": [],
        "alternatives": null
      },
      "stop": null
    }
  ],
  "best": 0
}
```

#### `required_slot_empty`

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kibakek",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "rejected",
      "analysis": null,
      "failure": {
        "kind": "required_slot_empty",
        "plain": "The template requires slot 2 to be filled, and no expected morph fills it.",
        "position": null,
        "hc_reason": "None",
        "provenance": "hc_reported",
        "object": {
          "type": "template",
          "id": "tmpl1",
          "guid": "00000000-0000-0000-0000-000000000004"
        },
        "morphs": [],
        "context": null,
        "environment": null,
        "stem_name": null,
        "features": [],
        "alternatives": null
      },
      "stop": null
    }
  ],
  "best": 0
}
```

#### `feature_clash`

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kibakek",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "rejected",
      "analysis": null,
      "failure": {
        "kind": "feature_clash",
        "plain": "Suffix `ak` requires tense=past but the word has tense=future.",
        "position": 1,
        "hc_reason": "RequiredSyntacticFeatureStruct",
        "provenance": "hc_reported",
        "object": {
          "type": "rule",
          "id": "ak",
          "guid": "00000000-0000-0000-0000-000000000015"
        },
        "morphs": [
          {
            "morpheme": "ak"
          }
        ],
        "context": null,
        "environment": null,
        "stem_name": null,
        "features": [
          {
            "path": "syn.tense",
            "required": "past",
            "actual": "future"
          }
        ],
        "alternatives": null
      },
      "stop": null
    }
  ],
  "best": 0
}
```

#### `adhoc_prohibition`

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kibakek",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "rejected",
      "analysis": null,
      "failure": {
        "kind": "adhoc_prohibition",
        "plain": "An ad hoc rule forbids suffix `ak` together with suffix `ek`.",
        "position": 2,
        "hc_reason": "MorphemeCoOccurrenceRules",
        "provenance": "hc_reported",
        "object": {
          "type": "rule",
          "id": null,
          "guid": null
        },
        "morphs": [
          {
            "morpheme": "ak"
          },
          {
            "morpheme": "ek"
          }
        ],
        "context": null,
        "environment": null,
        "stem_name": null,
        "features": [],
        "alternatives": null
      },
      "stop": null
    }
  ],
  "best": 0
}
```

#### `stem_name`

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kibakek",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "rejected",
      "analysis": null,
      "failure": {
        "kind": "stem_name",
        "plain": "The root allomorph `kib-a` requires stem name `sn-past`, which this word does not select.",
        "position": 0,
        "hc_reason": "RequiredStemName",
        "provenance": "hc_reported",
        "object": null,
        "morphs": [
          {
            "morpheme": "kib"
          }
        ],
        "context": null,
        "environment": null,
        "stem_name": "sn-past",
        "features": [],
        "alternatives": null
      },
      "stop": null
    }
  ],
  "best": 0
}
```

#### `inflection_class`

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kibakek",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "rejected",
      "analysis": null,
      "failure": {
        "kind": "inflection_class",
        "plain": "The suffix `ak` is excluded for the root's inflection class `ic2`.",
        "position": 1,
        "hc_reason": "ExcludedStemName",
        "provenance": "derived",
        "object": {
          "type": "class",
          "id": "ic2",
          "guid": "00000000-0000-0000-0000-000000000002"
        },
        "morphs": [
          {
            "morpheme": "kib"
          },
          {
            "morpheme": "ak"
          }
        ],
        "context": null,
        "environment": null,
        "stem_name": null,
        "features": [],
        "alternatives": null
      },
      "stop": null
    }
  ],
  "best": 0
}
```

#### `surface_mismatch`

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kipak",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "rejected",
      "analysis": null,
      "failure": {
        "kind": "surface_mismatch",
        "plain": "The expected morphs build `kibak`, not the word `kipak`.",
        "position": null,
        "hc_reason": "SurfaceFormMismatch",
        "provenance": "hc_reported",
        "object": null,
        "morphs": [
          {
            "morpheme": "kib"
          },
          {
            "morpheme": "ak"
          }
        ],
        "context": null,
        "environment": null,
        "stem_name": null,
        "features": [],
        "alternatives": null
      },
      "stop": null
    }
  ],
  "best": 0
}
```

#### `other_gate`

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kibakek",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "rejected",
      "analysis": null,
      "failure": {
        "kind": "other_gate",
        "plain": "A synthesis gate refused suffix `ak` and no more specific kind applies.",
        "position": 1,
        "hc_reason": "MaxApplicationCount",
        "provenance": "hc_reported",
        "object": {
          "type": "rule",
          "id": "gate13",
          "guid": "00000000-0000-0000-0000-00000000000d"
        },
        "morphs": [
          {
            "morpheme": "ak"
          }
        ],
        "context": null,
        "environment": null,
        "stem_name": null,
        "features": [],
        "alternatives": null
      },
      "stop": null
    }
  ],
  "best": 0
}
```

### Several candidates and several choices

Three candidates; the second offers two choices at position 1. Candidate 0 fails at an environment, candidate 1 fails earlier (no morph matches), candidate 2 is `unknown`. `best` is 0: nothing was accepted, and an unmet environment survives more checks than a missing morph.

```json
{
  "schema": "pangloss.whynot.v1",
  "word": "kib-ek",
  "results": [
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          }
        ],
        [
          {
            "morpheme": "ek"
          }
        ]
      ],
      "result": "rejected",
      "analysis": null,
      "failure": {
        "kind": "environment_unmet",
        "plain": "The allomorph `ek-a` of suffix `ek` needs a left context of a back vowel, but the word has `i` on the left and `k` on the right.",
        "position": 2,
        "hc_reason": "Environments",
        "provenance": "hc_reported",
        "object": null,
        "morphs": [
          {
            "morpheme": "ek"
          }
        ],
        "context": {
          "left": "kib",
          "right": "k",
          "provenance": "derived"
        },
        "environment": {
          "allomorph_id": "ek-a",
          "index": 0
        },
        "stem_name": null,
        "features": [],
        "alternatives": 1
      },
      "stop": null
    },
    {
      "expect": [
        [
          {
            "morpheme": "kib"
          }
        ],
        [
          {
            "morpheme": "ak"
          },
          {
            "morpheme": "ik"
          }
        ]
      ],
      "result": "rejected",
      "analysis": null,
      "failure": {
        "kind": "no_morph_match",
        "plain": "No morph matches at position 1: nothing in the word can be the expected suffix `ak`.",
        "position": 1,
        "hc_reason": null,
        "provenance": "derived",
        "object": {
          "type": "affix",
          "id": "ak",
          "guid": "00000000-0000-0000-0000-000000000015"
        },
        "morphs": [
          {
            "morpheme": "ak"
          }
        ],
        "context": null,
        "environment": null,
        "stem_name": null,
        "features": [],
        "alternatives": null
      },
      "stop": null
    },
    {
      "expect": [
        [
          {
            "morpheme": "lo"
          }
        ]
      ],
      "result": "unknown",
      "analysis": null,
      "failure": null,
      "stop": "work_cap"
    }
  ],
  "best": 0
}
```

## Suite runner

`pangloss why-not <grammar> <queries.jsonl> <output.jsonl>` reads one query per line,
`{"word", "expect": [{"morpheme", "allomorph"}...]}`, and writes one output object per query, in
order, in the shape of the shared suite's `schema/whynot-output.schema.json`: `word`, `expect`,
`result` and, when rejected, `failure`, or, when unknown, `stop {reason, note}`. It runs the same
engine and maps this report onto that line; the suite names a morpheme by its `<MorphemeId>` text,
so the runner translates to and from the engine's `id` ids. The suite compares `result` and the
asserted subset of `failure`, and never `index`. A malformed or unresolvable query answers `unknown`
with a stop reason of its own (`malformed_query`, `unknown_id`, `ambiguous_id`, `inconsistent_expect`
or `invalid_query`) and does not stop the run. The pinned conformance suite predates PROTOCOL section 11 and the driver does not know a `why-not` capability, so the conformance scripts do not declare one; `--describe` declares it for hosts. The scripts declare it when the pin moves to a suite that has section 11.

## Not yet decided (later slices)

* The accepted `analysis` is the engine's word analysis today; a later slice may add the `parse`
  projection (glosses, authored identities).
* Compounding and guessed roots are out of the first search slice.
