# why-not (`pangloss.whynot.v1`)

`pangloss why-not` answers one question: *why was this word not parsed along these morphs?* You
give it a word and one or more **candidates** (a candidate is the morph sequence you expected, in
surface order). It does not search every path. For each candidate it reports whether the word
parses along it, and if not, the **first failing constraint**, typed, in plain words, with the
grammar object the failure is about, so a tool can draft a fix.

> **Status: slice 0, a contract and a stub (Motif #386).** The command validates its input and
> then answers `unknown` with `stop: "not_implemented"` for every candidate. `best` is therefore
> always `null` today. Every `accepted` and `rejected` example below is **illustrative**: it shows
> the shape the later slices will return, not output any current build produces. Code against the
> shape; do not treat an example's wording or ids as real. The `--work-cap` and `--word-timeout-ms`
> flags are validated but not applied yet, and the command says so on stderr when you pass them.

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
* `--allomorphs <spec>` is reserved and refused for now (see Input errors).
* `--json` writes the report below to standard output; without it a short text summary is written.
* `--work-cap`, `--word-timeout-ms` bound the search once it exists (they produce `unknown` with
  `stop: "work_cap"` or `"deadline"`).

### Candidate syntax

A candidate lists **positions** in surface order. A position lists one or more **choices**; each
choice is tried, and the report says which was used.

| Symbol | Meaning | Example |
|---|---|---|
| `,` | next position | `kib,ak,ek` is three positions |
| `\|` | another choice at the same position | `kib,ak\|ik,ek` offers `ak` or `ik` at position 1 |
| `:` | (`--expect-guid` only) joins `entryGuid` and `msaGuid` | `E:M,E2:M2` |

A morpheme id is the `id` attribute of its lexical entry or morphological rule in `grammar.xml`
(`MorphemeInfo.xml_key` in the engine). Ids are XML NCNames, so `,` `|` `@` `:` cannot occur in one,
and GUIDs (hex digits and `-`) cannot clash with the separators either. Some morphemes share one id
(a main entry and its variants share an MSA): such an id is an input error that points at
`--expect-guid`, whose `entryGuid:msaGuid` choice that matches several morphemes becomes several
choices at that position. Dense numeric ids (`morphemeId` in an analysis) are not accepted.

`--expect-json` input is a JSON array of candidates; each candidate an array of positions; each
position a non-empty array of choices `{"morpheme": "id", "guid": {"entry_guid", "msa_guid"}?}`. Unknown
fields are refused on input. `allomorph` is reserved like `--allomorphs` and refused for now:

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
matches no morpheme, an empty id, malformed JSON, a bad flag value, and any allomorph selection
(`id@allomorph`, `--allomorphs`, or `allomorph` in JSON): allomorph selection by id arrives with
allomorph XML ids, which the loader does not keep yet. A grammar that fails to load is also exit 1. Everything else, including a word that is
rejected or `unknown`, exits 0.

## Report

The report **always** has `results`, even for a single candidate, so one shape serves every call.
`results` is in input order. Each entry echoes its candidate as `expect`.

| Field | Meaning |
|---|---|
| `schema` | `pangloss.whynot.v1` |
| `word` | the word as given |
| `results[]` | one entry per candidate, in input order |
| `results[].expect` | the candidate as parsed: positions, each a list of `{morpheme, guid?}` choices (the same shape as `--expect-json`; `allomorph` is omitted) |
| `results[].result` | `accepted`, `rejected` or `unknown` |
| `results[].analysis` | when accepted: `{analysis, used}`. `analysis` is the ordinary word analysis (`morphemeIds`, `morphOccurrences`, ...); `used` has one `{morpheme, allomorph?}` per position, the choice that produced it. Otherwise null |
| `results[].failure` | when rejected: the first failure (below). Otherwise null |
| `results[].stop` | when unknown: `not_implemented` (slice 0), `work_cap`, `deadline`, or `failure_not_expressible` (a failure was found but cannot fill a required `failure` field; the C# tool emits this, so both engines share this schema). Otherwise null |
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
| `plain` | one English sentence |
| `position` | 0-based position in the candidate the failure is about, or null when it is about the whole word |
| `hc_reason` | the engine's reason, the C# `FailureReason` name when HC reports it, or null when PanGloss inferred the failure |
| `provenance` | `hc_reported` (C# raises this failure) or `derived` (PanGloss inferred it; C# has no event) |
| `object` | the grammar object a fix would touch, for kinds without a dedicated field above: `{type, id, guid}`; `type` is one of `environment`, `allomorph`, `affix`, `slot`, `template`, `rule`, `class`, `stem_name`, `feature`. `id` is the `grammar.xml` id; `guid` is the FieldWorks identity, filled in only when the caller has the FieldWorks model. Null for `environment_unmet` and `stem_name`, which carry `environment` and `stem_name` instead, and when no single object applies |
| `morphs` | the morph(s) involved, as `{morpheme, allomorph?}` (the shape of an expected-parse element; `allomorph` is omitted when unknown); for a position with several choices, the choice used. For `adhoc_prohibition` it is the pair, the rule's primary morph first and the offending partner second; for `stem_name` the root allomorph |
| `context` | `environment_unmet` only: `{left, right, provenance}`, the actual text around the allomorph; otherwise null. HC does not report it, so an engine or adapter computes it and `provenance` is always `derived`, even when the failure itself is `hc_reported` |
| `environment` | `environment_unmet` only: `{allomorph_id, index?}`, the allomorph and the 0-based index in its environment list (an environment has no id of its own); `index` is omitted when the engine does not know it, never a fake 0. Otherwise null |
| `stem_name` | `stem_name` only: the stem name's `grammar.xml` id. Otherwise null |
| `features` | `feature_clash` only: an array with one `{path, required, actual}` per mismatching feature path (the word's feature state against one requirement); an empty array otherwise |
| `alternatives` | how many other derivations of this candidate also fail, or null when not counted |

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
  as in the suite. `allomorph` ids are optional until the loader keeps allomorph XML ids.
* Conformance fixtures assert only the kinds C# itself reports: `environment_unmet`,
  `feature_clash`, `adhoc_prohibition` and `stem_name`. The other kinds are **engine-specific**:
  `no_morph_match`, `slot_order`, `required_slot_empty`, `surface_mismatch`, `inflection_class` and
  `other_gate` (and the `unknown` result) are kept, documented as such, and are not compared with C#.
* The `grammar.xml` id is the required identity everywhere. `entry_guid` and `msa_guid` (in a choice's `guid`) and
  `object.guid` are optional and are filled in when the caller has the FieldWorks model, as Motif does.

### Compatibility

Consumers ignore fields they do not know: the report, `failure` and its parts may gain fields in
a later slice without a new schema version. Only the `--expect-json` input is strict.

### Ordering and what is promised

HC short-circuits, so the contract promises **only the first failure per derivation**: later
failures of the same derivation are not reported. A candidate with several choices has several
derivations; its `failure` is the first failure of the furthest-progress derivation, and
`alternatives` counts the others that also fail. Derivations and candidates are ranked by first-failure rank
(the kind order above, then position); `results` itself stays in input order, and `best` points at
the furthest-progress candidate.

## Examples (illustrative)

Each example below is **illustrative**: slice 0 returns only `unknown`. Ids, GUIDs and wording are
invented (synthetic) and use a made-up language. They validate against the report schema.

### `unknown` (what slice 0 returns today)

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
          "morphemeIds": [
            7,
            21,
            30
          ],
          "morphOccurrences": [
            {
              "allomorphId": 11,
              "morphemeId": 7,
              "order": 0
            },
            {
              "allomorphId": 20,
              "morphemeId": 21,
              "order": 1
            },
            {
              "allomorphId": 41,
              "morphemeId": 30,
              "order": 2
            }
          ],
          "rootMorphemeIndex": 0,
          "posId": null,
          "synFs": "<FeatureStruct as pangloss serializes it>",
          "mpr": "<MprSet as pangloss serializes it>",
          "guessed": false,
          "guessedString": null,
          "provenance": {
            "kind": "grammar"
          },
          "suppliedRoot": null,
          "morphemeRoots": [
            null,
            null,
            null
          ]
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
  "word": "No morph matches at position 1: nothing in the word can be the expected suffix `ak`.",
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
  "word": "The allomorph `ek-a` of suffix `ek` needs a left context of a back vowel, but the word has `i` on the left and `k` on the right.",
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
  "word": "Suffix `ek` is in slot 3 and suffix `ak` is in slot 2, so `ek` cannot come before `ak`.",
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
  "word": "The template requires slot 2 to be filled, and no expected morph fills it.",
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
  "word": "Suffix `ak` requires tense=past but the word has tense=future.",
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
  "word": "An ad hoc rule forbids suffix `ak` together with suffix `ek`.",
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
          "id": "adhoc12",
          "guid": "00000000-0000-0000-0000-00000000000c"
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
  "word": "The root allomorph `kib-a` requires stem name `sn-past`, which this word does not select.",
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
  "word": "The suffix `ak` is excluded for the root's inflection class `ic2`.",
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
  "word": "The expected morphs build `kibak`, not the word `kipak`.",
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
  "word": "A synthesis gate refused suffix `ak` and no more specific kind applies.",
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

## Not yet decided (later slices)

* The accepted `analysis` is the engine's word analysis today; a later slice may add the `parse`
  projection (glosses, authored identities).
* `alternatives` and the multi-allomorph tie rules arrive with the search.
* Compounding and guessed roots are out of the first search slice.
