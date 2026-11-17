# Stats cache feature gates

HC `pangloss batch --stats` records feature predicates during the existing search. No extra parse
or corpus pass is made. The regular batch TSV and existing counters keep their meanings.
Gate collection is automatic with HC `--stats`; without stats there is no collector or cache.
Foma caches record gate measurement as unmeasured.

## Versions and measurement

Full caches use schema **13**, compact caches **14**. Both replace the preceding 11/12 caches
when opened for writing. Counter semantics remain 8 for the existing counters. Inspect
`cache_identity.schema_version` before querying, and use `word.gates_measured`, rather than table
presence, as the measurement flag. A measured word can have no gate rows because its search never
reached a declared gate. That does not establish that an unvisited feature blocked nothing.

`word.capped`, `word.timed_out`, and `word.invalid_shape` remain independent completeness indicators.
Counts from an incomplete search describe only the observed prefix. A zero blocked count is not
corpus evidence when any requested word is missing, unmeasured, capped, timed out, or invalid.
The caller must check its requested input coverage as well as the stored flags.

## Tables and identities

```sql
feature(feature_id INTEGER PRIMARY KEY, key TEXT, kind TEXT, label TEXT, mode TEXT,
        UNIQUE(key, kind, mode))
gate_fact(word_id INTEGER, object_id INTEGER, feature_id INTEGER,
          passed INTEGER, blocked INTEGER,
          PRIMARY KEY(word_id, object_id, feature_id))
```

`gate_fact` uses `WITHOUT ROWID`; counters are nonnegative 64-bit counts of evaluated predicates,
aggregated across directions and local allomorphs. `object_id` joins `object.object_id`; the object
key and kind use the same compiled-output identities as the ordinary fact table. Allomorph
predicates are attributed to their owning morphological rule or lexical entry. A reparse replaces
that word's gate facts, together with its measurement state, in the ordinary flush transaction.
Compact mode retains these counters unchanged.

| feature kind | key | modes |
|---|---|---|
| `mpr` | authored MPR feature identity: XML `id`, or the original FieldWorks GUID for exception features, inflection classes, and entry inflection types | `required`, `excluded`, `compound-required` |
| `stem-name` | authored XML ID or original FieldWorks stem-name GUID | `required`, `excluded` |
| `syntactic` | canonical nested JSON object mapping feature identities to arrays of authored symbol IDs, or nested feature objects | `required`, `realizational-unification`, `realizational-subsumption` |
| `syntactic` | canonical JSON object with `required` and `output` members, each containing the nested feature encoding above | `inverse-unification` |
| `pos` | the same JSON encoding for the phonological subrule's required POS set | `required` |

`feature_id` is a cache-local SQLite integer, never a grammar bit number or a cross-cache ID.
Join Motif's exception-tag identity against `feature.key`, filtered to `kind='mpr'` and
`mode='excluded'`. Labels are display text and must not be used as identities. Syntactic/POS rows
record the collective feature-structure comparison, not independent counterfactual tests of each
JSON leaf. The structural keys `__pos__`, `__head__`, and `__foot__` identify compiler slots where
present; these keys are not FieldWorks GUIDs. POS values and nested authored feature/value
identities retain their source XML IDs or GUIDs. A stem-name predicate includes its regions as
defined in the source grammar.

`required` records the declared syntactic requirement: rule unification or affix-allomorph
subsumption at validation. `inverse-unification` is the analysis owner's
compatibility check between `priority_union(required, output)` and the word's syntactic features.
Its key retains both authored structures, including an empty required structure for output-only
rules. Output values override matching required paths, so a block of this predicate must not be
presented as a failure of the required structure alone. Analysis edge prechecks and rule bodies
are separate evaluations and each counts when reached.

Realizational analysis observes unification with the word's realizational features, before
shape matching. Synthesis observes subsumption against those features. Their modes distinguish
both comparisons from syntactic requirements and from one another; aggregation across directions
never merges these different predicates.

## Evaluated checks

Collection observes existing decisions without repeating their tests or changing their short
circuits:

- Affix-process and realizational allomorph required/excluded MPR checks; compounding subrule
  required/excluded MPR checks, and head/non-head productivity restrictions.
- Lexical lookup replay's affix MPR and required-stem-name feasibility checks, including checks
  that reject a candidate before synthesis. Required MPR feasibility uses the existing upper
  bound on available features; exclusion feasibility uses the existing lower bound. A passing
  feasibility check need not pass during synthesis; both checks count when both are reached.
- Morphological rules' required syntactic feature structures in analysis and synthesis, including
  the compound non-head candidate checks, and realizational-feature requirements.
- Affix required-stem-name identity checks, root stem-name region requirements and sibling
  exclusions, including guessed-pattern roots and disjunctive validation candidates.
- Final affix-allomorph required-syntactic-feature subsumption, including disjunctive candidates.
- Phonological rewrite subrules' required POS and required/excluded MPR checks in synthesis.
  Analysis has no such subrule gates.

For ungrouped and All-group MPR predicates, each evaluated member records its own membership
outcome. For Any-group predicates, all members of the evaluated bucket share its collective
outcome: required passes if any member is present; excluded blocks only if every member is
present. Such a blocked count demonstrates participation in a rejecting group, not that removing
that one tag would change acceptance. Later buckets and excluded sets skipped after a required
failure get no observations.
Compound productivity members share their existing collective productivity decision, distinguished
by the `compound-required` mode.

Word-level obligatory-feature presence checks validate the final word rather than a rule or affix's required/excluded predicate; they are not attributed to a guessed rule owner.

Segment-level feature environments and alpha-variable matches belong to FST pattern matching,
not the MPR/POS gate mechanism. Environment, co-occurrence, template, bound-root, and surface-match
rejections are not feature-gate rows. Existing counters and traces describe those mechanisms.

The parser uses compact integer keys and one expected-constant-time hash update per observed
feature. Identity strings are resolved after each parse. Gate observation consumes no parser-work
reservations, so parser work is comparable with stats off.

## Replay and packed storage

```text
pangloss stats grammar.xml --cache cache.sqlite --group gate
pangloss stats grammar.xml --cache cache.sqlite --group gate --format jsonl
```

Rows show object and feature keys, feature kind, polarity, passed, and blocked. They sort by
blocked descending, then identity. `--word`, `--object`, `--kind`, `--top`, and
`--exclude-censored` apply. The header reports measured, unmeasured, and censored word counts;
empty output is not converted into zero-valued feature rows. Direction, stratum, wide, sort, and
by-kind dimensions do not apply to gate replay.

`--cache-pack` archives `gate_fact` and `feature` along with `object` and the other fact dimensions.
The outer SQLite envelope retains `word`, including `gates_measured`, for Motif's existing direct
word reads. `pangloss stats` expands the archive transparently without modifying the source.
Direct gate readers first obtain a plain copy:

```text
pangloss stats-cache unpack --cache packed.sqlite --out plain.sqlite
```

## Exclusions observed but never blocking

After validating requested input coverage, bind `:exception_tag_ids_json` to a JSON array of
Motif's authored exception-tag identities. Other MPR identities can represent inflection classes
or entry types. This query refuses to return redundancy candidates if any stored word is
incomplete or unmeasured, and excludes features never checked:

```sql
SELECT f.key AS exception_tag_id,
       SUM(g.passed) AS passed, SUM(g.blocked) AS blocked
FROM gate_fact AS g
JOIN feature AS f USING (feature_id)
JOIN word AS w USING (word_id)
WHERE f.kind = 'mpr' AND f.mode = 'excluded'
  AND f.key IN (
    SELECT CAST(value AS TEXT) FROM json_each(:exception_tag_ids_json)
  )
  AND NOT EXISTS (
    SELECT 1 FROM word
    WHERE gates_measured <> 1 OR capped OR timed_out OR invalid_shape
  )
GROUP BY f.feature_id
HAVING SUM(g.passed) > 0 AND SUM(g.blocked) = 0
ORDER BY f.key;
```

An absent feature, an empty cache, or unmeasured data must be displayed as unmeasured, not zero.
These are corpus observations, not a proof that a restriction can safely be deleted.
