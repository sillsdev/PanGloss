# PanGloss grammar facts format (v8)

**Format:** `pangloss-grammar-facts`. **SQLite application ID:** `1346848321` (`0x50474641`, `PGFA`).
**Schema version:** `8`. The executable DDL is [schema.sql](../rust/crates/pg-facts/src/schema.sql).

## Changes from v4

Schema v5 increments `artifact_meta.schema_version` and `PRAGMA user_version` from 4 to 5. It adds
per-occurrence source census rows in `source_object` and projects typed importer decisions into
`load_fact`. The `source_census` section is complete only when current provenance records every raw
header; synthetic and older hand-built Snapshots report it unavailable. `load_accounting` remains
partial while compiler decisions without owner reasons are explicitly recorded as unknown. The
artifact is rebuilt from source; there is no in-place schema migration.

This document defines the facts artifact emitted by `pangloss facts`. It describes authored facts
from the exact current `pg-snapshot` JSON input, typed conversion outcomes supplied by the importer
and compiler, and optionally one immutable projection of a validated parser stats run. It does not
contain Motif evidence, parse analyses, judgements, recommendations, or grammar edits.

## Changes from v5

Schema v6 increments `artifact_meta.schema_version` and `PRAGMA user_version` from 5 to 6. It adds
`compiled_mapping` and `compiled_allomorph_order`, populated from compiler-published final grammar
associations after compaction, and `parser_config`, which records normalized Snapshot parameters and
the compiled strata. The `compiled_mappings` section is complete after a successful compile;
`effective_grammar` remains partial because the full runtime grammar is not serialized. The
`parser_config` section is partial because Snapshot defaults do not retain whether every scalar was
explicitly present in the source. `load_accounting` is complete only when current imported provenance
and every importer/compiler inventory subject have non-unknown decisions. The artifact is rebuilt
from source; there is no in-place schema migration.

## Changes from v6

Schema v7 increments `artifact_meta.schema_version` and `PRAGMA user_version` from 6 to 7. It
adds the frozen-run stats projection and a collector support catalog. `pangloss facts` accepts
both a P7 batch cache and its manifest or accepts neither. A supplied pair is checked against the
exact Snapshot, production compile options, compiler build, closed-cache digest, sole run, options,
ordered words, and completion rows before any stats facts are published. Cache handles are remapped
to deterministic typed identities; source GUID bridges are created only from GUIDs present in the
exact Snapshot or its compiler-published source associations. The seven logical counter support
states come from `pg-rules`' collector API; self-time support is recorded by direction. The artifact
is rebuilt from source; there is no in-place schema migration.

## Changes from v7

Schema v8 increments `artifact_meta.schema_version` and `PRAGMA user_version` from 7 to 8. It
carries grammar objects in the source census and drops the per-occurrence rows for every other
class: `source_object` and the `sourceObject` rows of `load_fact` exist only where `inventory_kind`
is not null. `source_class_count` and the `source_census` total still count every occurrence, so a
text or wordform is visible as a count. The text-side data comes from the project itself, not from
these facts. No CLI option restores a full census. The Snapshot is compiled exactly as the parser
compiles it, so `grammar_hash` and compiled keys match the parser's. Every GUID in the stored rows is
lowercased when the rows are written, including GUIDs embedded in key and message text. The one
exemption is `source_object.raw_guid`, which keeps the importer's spelling. The artifact is rebuilt
from source; there is no in-place schema migration.

## Compiled output keys

Every compiled grammar object has one row in `compiled_output`, and every consumer names it by
`output_id`. The `key` text is the one spelling: `compiled_mapping` (source-to-output lineage),
`compiled_allomorph_order` (sibling order) and `stats_object` / `stats_allomorph` (stats) all join
to it. A stats row of kind `morph_rule`, `phon_rule` or `lex_entry`, and every allomorph row, carries the
`output_id` of the object whose key it shares; guesser, overlay and root-trie rows carry `NULL`.

Keys are built only in `rust/crates/pg-grammar/src/compile/lineage.rs`, one function per kind, and
GUIDs inside them are canonical lowercase from the start:

- Lexical entry: `lex_entry:{xml_key}#{entry_guid}[~{infl_type_guid}]@{bucket}`. The `#{entry_guid}`
  part appears when the morpheme has an owning entry; `[~...]` only for variant stems, one per
  inflection type.
- Affix or realizational rule: `morph_rule:{xml_key}#{entry_guid}@{bucket}`.
- Compound rule: `compound_rule:{endo|exo}#{rule_guid}@{bucket}`. The tag is `exo` for an exocentric
  rule and `endo` otherwise. An exocentric rule compiles to a left and a right output from one GUID, so
  its key adds the side: `compound_rule:exo#{rule_guid}#{left|right}@{bucket}`. A rule with no source
  GUID (a synthetic default, or a GUID-less XML fixture) uses its name in place of the GUID. Renaming an
  authored rule keeps its key.
- Natural class: `natural_class:{xml_id}`, with no bucket.
- Phonological rule: `phon_rule:{xml_id}@{bucket}`, or `phon_rule#{index}:{name}@{bucket}` when the
  rule has no `xml_id`.
- Template: `template:{guid}@{bucket}`, or `template#{index}:{name}@{bucket}` without a GUID.
- Allomorph: `{owner_key}#allo{index}`, where `owner_key` is the final key of the owning entry or rule.

`{bucket}` is the stratum name (`Morphology`, `Clitics`, ...). `stratum_key` is `stratum#{ordinal}`
and is `NULL` where no stratum lists the object. `stratum_key` carries no foreign key to `stratum`: a
refused compile publishes outputs whose strata rows were never written, so the constraint would reject
it.

**Collision rule.** Keys are claimed in compile order: lexical entries (each followed by its allomorphs),
then affix, realizational and compound rules in `mrules` order (each followed by its allomorphs), then
phonological rules, templates and natural classes. The first output to claim a key keeps it. Each later
output with the same key takes the suffix `!{n}`, with `n` = 2, 3, ... for that key, and its
`identity_quality` becomes `structural`. A suffixed key is a new key, so it is never reused. Two variant
entries sharing an inflection type or a repeated component lexeme therefore stay distinct. A by-design
multi-output, such as the exocentric halves, names its parts in the key instead and never reaches this rule.

A compound rule with no source GUID takes its name verbatim in the key; a name containing `#` or `@`
makes that key ambiguous, which no current fixture produces.

Columns of `compiled_output`:

| Column | Meaning |
|---|---|
| `output_id` | 1-based id, in compile order. |
| `kind` | `lex_entry`, `morph_rule`, `allomorph`, `template`, `phon_rule`, `compound_rule` or `natural_class`. |
| `key` | The canonical key above. Unique. |
| `owner_output_id` | Allomorph: its entry or rule. `NULL` otherwise. |
| `stratum_key`, `bucket` | Owning stratum (`NULL` and `''` where none applies). |
| `compiled_order` | Allomorph index within its owner. Non-`NULL` only for allomorphs. |
| `identity_quality` | `authored`, `structural` (a collision or an XML-derived key) or `synthetic`. |
| `realization_kind` | Allomorph shape: `root` (a root allomorph), `null` (empty RHS), `segments` (inserts with no copy between them), `process` (a `Modify` or context insert, or no insert), `circumfix` (inserts before and after the copies), `infix` (an insert between copies). Reduplication is not represented: the compiled RHS cannot show it, because `redup_hint` carries the morph type. |
| `has_phone_condition` | 1 when the allomorph has an environment, or a left-hand context beyond the bare boundary-and-any wrapper. |
| `has_morph_gate` | 1 when it has required or excluded MPRs, a required syntactic feature structure, or a stem name. |
| `gate_signature` | Canonical gate text, below. Equal text means equal gate. |
| `is_unconditioned` | 1 when neither `has_phone_condition` nor `has_morph_gate` is set. |

`gate_signature` is `mpr=<ids>;xmpr=<ids>;fs=<FS>;stem=<name>`, with these exact rules:

- `<ids>`: the `MprId`s of the required (`mpr`) or excluded (`xmpr`) set, ascending, comma-joined,
  with no spaces. Empty when the set is empty.
- `<FS>`: the required syntactic feature structure as `[feat=value,...]`, with features in ascending
  `FeatId` order. A symbolic value is `s:` and its symbol indices ascending and comma-joined. A complex value
  is `{...}` around its own `[...]`. The empty FS is `[]`.
- `<name>`: the stem name, with `%`, `;`, `=`, `|` and `,` written as `%XX` (two hex digits). Empty when
  the allomorph has no stem name.

Ids are grammar-local, so two signatures are comparable only within one compiled grammar.

`compiled_mapping(source_kind, source_guid, source_key, output_id, relation_role, source_ordinal,
identity_quality)`. `relation_role` is `entry` (an entry's own lexical entry), `variant` (an entry whose
allomorph forms a rule uses, other than the rule's owner), `msa`, `rule` (an infl-type or compound or
phonological or template rule), `form` (one allomorph form), `circumfix_prefix_half` and
`circumfix_suffix_half` (the two forms of a circumfix allomorph, in `source_ordinal` order), and
`null_affix`. Compound, phonological and template sources map to their outputs by `rule`.
A referenced natural class maps to its `natural_class` output with `relation_role` `rule`.

`compiled_allomorph_order(owner_output_id, source_entry_guid, source_msa_guid, source_allomorph_key,
source_allomorph_guid, output_id, compiled_order)`. Rows for a root whose entry the compiler did not
represent have `owner_output_id` and `output_id` `NULL`.

`template_slot.surface_ordinal` is the signed distance of a slot from the stem: suffix slot `i` is
`i + 1`, and prefix slot `k` is `-(k + 1)`. Both slot lists are innermost-to-outermost, as the snapshot
model documents (`rust/crates/pg-snapshot/src/morphology.rs`), so ordinal 0 is nearest the stem.

## Invocation and context

```text
pangloss facts <snapshot.json> --out <facts.sqlite> --context <facts-context.json> [--json]
```

To attach one frozen HermitCrab run, pass both `--stats <cache.sqlite>` and
`--stats-manifest <manifest.json>`. They must be supplied together. The stats cache must be the
closed cache named by the version 1 manifest produced by `pangloss batch --stats-manifest`; the
reader validates it against the exact Snapshot bytes, model fingerprint, compiler build, resolved
production compile options, cache digest, run metadata, options, ordered input list, and per-word
completion census. It refuses an unsupported, stale, changed, or mixed identity. A validated run may
contain capped, timed-out, invalid-shape, or missing words; their statuses remain explicit in
`stats_word`. See
the [batch stats manifest contract](../rust/docs/batch-stats-manifest.md) for manifest production.

```text
pangloss facts <snapshot.json> --out <facts.sqlite> --context <facts-context.json> \
  --stats <cache.sqlite> --stats-manifest <manifest.json> [--json]
```

Only `.json` Snapshot input is accepted. Use `pangloss import` first for a FieldWorks project.
`--out` must name a new path. The writer never replaces an existing file. `--json` writes one
result object to stdout; diagnostics go to stderr. A structured compiler refusal still publishes
authored and diagnostic facts and exits unsuccessfully with `compileStatus: "refused"`.

The context is a closed JSON object. Duplicate keys, unknown properties, unsupported versions,
and noncanonical floating-point values in the Baseline token are rejected.

```json
{
  "format": "pangloss-facts-context",
  "version": 1,
  "baselineToken": { "opaque": "caller-owned JSON value" },
  "inputKind": "baseline",
  "dryRunDigest": null,
  "expectedModelFingerprint": "sha256:<64 lowercase hex digits>"
}
```

For a Proposal Dry Run, set `inputKind` to `proposal-dry-run` and supply its digest:

```json
{
  "format": "pangloss-facts-context",
  "version": 1,
  "baselineToken": { "opaque": "the same frozen Baseline token" },
  "inputKind": "proposal-dry-run",
  "dryRunDigest": "sha256:<64 lowercase hex digits>"
}
```

`baselineToken` is opaque to PanGloss and is stored as RFC 8785 JSON Canonicalization Scheme (JCS)
JSON. `baselineKey` is SHA-256 over those canonical bytes. `inputKind` is `baseline` or
`proposal-dry-run`. A baseline requires `dryRunDigest: null`; a proposal dry run requires a
`sha256:` digest. `expectedModelFingerprint` is optional and, when present, must equal the
fingerprint computed for this source and compiler.

## Identity and publication

The `artifact_meta` singleton records:

| Column | Meaning |
|---|---|
| `application_id`, `schema_version`, `format` | File identity: `1346848321`, `7`, and `pangloss-grammar-facts`. These mirror `PRAGMA application_id` and `PRAGMA user_version`. |
| `writer_version`, `compiler_version`, `source_revision`, `build_identity` | Producer build identity. |
| `snapshot_format`, `snapshot_version` | The input envelope identity. |
| `provenance_schema_version`, `source_inventory_status` | The Snapshot's importer provenance version and status. |
| `source_sha256` | SHA-256 of the exact input bytes; formatting changes this value. |
| `grammar_hash` | `Snapshot::grammar_hash()`, excluding conversion provenance. |
| `model_fingerprint` | `pg_assess::model_fingerprint(SourceKind::Snapshot, source, compiler_version)`. It covers canonical source JSON, including conversion provenance, and compiler version. |
| `baseline_token_json`, `baseline_key`, `input_kind`, `dry_run_digest` | The caller-supplied evidence context and its identity. |
| `compile_options_json`, `compile_options_sha256` | The production default options, including the resolved substrate policy and `SemanticLossPolicy::Refuse`. |
| `compile_status`, `complete` | `completed` or `refused`; `complete=1` means every row promised by the v8 schema was written and verified. It does not mean every section is available. |
| `run_manifest_sha256` | Nullable SHA-256 of the exact accepted stats manifest bytes. NULL when no stats run was requested. |

Digests use the `sha256:<hex>` spelling except `grammar_hash`, which follows the existing
Snapshot API's bare lowercase hex spelling.

All tables are created from the checked-in DDL in one transaction. The writer uses UTF-8 encoding,
DELETE journal mode, a fixed page size and full synchronization, verifies `integrity_check` and
`foreign_key_check`, closes SQLite, syncs the temporary file, then publishes it with a
no-clobber atomic operation from the output directory. On Unix it also syncs the parent directory.
The response's `outputSha256` and `outputBytes` describe the published file after close; neither is
stored inside the file. Repeated builds with the same bytes, context, producer and SQLite build are
byte-identical. Cross-version SQLite byte identity is not promised.

The writer does not persist a timestamp or generated output path. Malformed input/context, model
mismatch, unsupported source/version, non-conversion compiler errors, and I/O failures before the
atomic publication leave no final artifact. A refused conversion is distinct: its complete
authored/diagnostic database is published, with `compile_status=refused` and `effective_grammar`
unavailable. If the file is published but its directory cannot be synced or its response digest
cannot be read, the command reports that the complete output already exists and names its path.

## Sections

`artifact_section` has exactly one row for each v8 section. `status` is `complete`, `partial`,
`unavailable`, or `not_requested`. Readers must check it before treating an absent table or empty
query result as an empty grammar.

| Section | V8 status | Scope |
|---|---|---|
| `project` | complete | Project name, declared writing-system order, exemplar characters. |
| `source_census` | complete for current imported provenance; unavailable for synthetic or legacy provenance | Grammar source objects with their retention/duplicate state, and class totals for every source header occurrence. Fatal import issues do not erase the source census. |
| `conversion_inventory` | complete | Importer and compiler inventory stage sets, counts, and structured issues. |
| `categories` | complete | Part-of-speech and inflection-class hierarchies, category feature links, and authored labels/default-class references. |
| `entries` | complete | Entry identity/order, senses, glosses and definitions. |
| `msas` | complete | MSA kind/entry plus category, slot, class, exception-feature and raw feature-structure references. |
| `adhoc_prohibitions` | complete | Flat allomorph and morpheme prohibitions, including disabled state and ordered conjunctive targets. |
| `adhoc_groups` | complete when `adhocProhibitionGroups` is present in the Snapshot; unavailable otherwise | Group identities, multilingual names/descriptions, and unordered member references. Current `.fwdata` imports publish the section, including when no groups exist. Older or hand-built Snapshots that omit the optional property do not claim an empty group inventory. |
| `load_accounting` | complete when current source census and all import/compiler inventory subjects have non-unknown decisions; partial otherwise | Typed decisions from the importer, compiler, and compaction. Missing owner decisions remain `unknown` with `decision_unrecorded`; they are not inferred from diagnostics. |
| `effective_grammar` | partial after compile; unavailable after refusal | V8 carries final mappings and contextual allomorph order, but does not serialize the complete runtime grammar. |
| `templates` | complete after compile; partial after refusal | Authored slots, templates, and authored side/order; `compiled_order` is compiler-published and nullable when a source slot is not represented. |
| `allomorphs` | complete | Every allomorph, every form in the supplied Snapshot, and every authored morphological gate with the compiler's `parser_effect`. |
| `environments` | complete after compile; partial after refusal (`compile_refused`) | Every Snapshot environment and allomorph phone/position edge, with parse outcomes for environments the compiler attempted; canonical sides only after compile. |
| `features` | complete | Feature-system definitions plus phoneme and feature-defined natural-class structures. |
| `phonology` | complete after compile; partial after refusal | Authored phoneme, boundary, class, and constraint facts; final effective class extensions and strata when compilation completes. |
| `patterns` | complete after compile; partial after refusal | Authored rewrite/metathesis trees and resolved trees for valid attempted environments. |
| `compound_rules` | complete | Authored compounding rules: kind, disabled state, head side, constituent and outcome categories and classes, exception features, and the parser's per-rule maximum applications when the Snapshot carries one. |
| `affix_processes` | complete | Authored affix-process input parts with their pattern roots, and ordered output steps. |
| `compiled_mappings` | complete after compile; unavailable after refusal | Compiler-published final source-to-output associations and allomorph order after compaction. |
| `parser_config` | partial | Normalized Snapshot parser parameters and compiled strata; source presence for defaulted scalar values is not retained. |
| `stats` | complete when one validated frozen run is attached; not_requested otherwise | One manifest-bound HermitCrab run and its exact ordered input completion census. A complete section can include incomplete or invalid-shape words; it means the supplied run and all its rows were validated and projected. `stats_counter_support` is populated in either case and describes collector capability, not run measurements. |

## Tables

The DDL is normative. Text uses SQLite binary collation unless a table says otherwise. GUIDs are
lowercase hyphenated `TEXT`. Ordinals are zero-based and nonnegative. Booleans are `0` or `1`.
Generated parent rows use foreign keys; source-reference targets intentionally do not, so dangling
category, class, slot, MSA, allomorph and prohibition references remain visible.

### Authored facts

| Table | Key and columns |
|---|---|
| `project` | `singleton`, `name`. |
| `writing_system` | `tag`. Contains tags used by project, citation-form, sense, allomorph-form, phoneme, and boundary-marker rows. |
| `project_writing_system` | `(role, ordinal)`, where role is `vernacular` or `analysis`; `writing_system_tag` references `writing_system`. |
| `project_exemplar_character` | `ordinal`, `character`. |
| `category` | `guid`, nullable source `parent_guid`, `sibling_ordinal`, `name`, `abbreviation`, nullable `default_inflection_class_guid`. The table contains POS objects only. |
| `category_feature` | `(category_guid, ordinal)`, `feature_guid`; the feature target is not a foreign key. |
| `inflection_class` | `guid`, owning `owner_category_guid`, nullable class `parent_guid`, `sibling_ordinal`, `name`, `abbreviation`. |
| `affix_slot` | `guid`, owning `category_guid`, `name`, `optional`. |
| `affix_template` | `guid`, owning `category_guid`, `name`, `disabled`, `is_final`. |
| `template_slot` | `(template_guid, side, ordinal)`, source `slot_guid`, nullable `compiled_order`; `side` is `prefix` or `suffix`. The source slot has no foreign key. Ordinals follow the authored Snapshot vectors. The compiler publishes effective order after dropping unresolved or rule-free slots; a missing order is not reconstructed by the writer. |
| `lex_entry` | `guid`, `source_ordinal`, `lexeme_morph_type`. |
| `allomorph` | `guid`, owning `entry_guid`, `ordinal`, `morph_type`, `form_class` (`stem`, `affix`, or `process`), `is_abstract`, nullable `stem_name_guid`. `form_class` is derived from the morph type and the process realization, because the Snapshot does not carry the source `MoForm` subclass. |
| `allomorph_gate` | `(allomorph_guid, gate_kind, ordinal)`, nullable `target_guid`, nullable `fs_id` (required features only), `parser_effect`, nullable `reason_code`. `gate_kind` is `inflection_class`, `required_features`, `required_category`, or `stem_name`. `parser_effect` is `applied`, `ignored`, `unresolved`, `owner_not_loaded`, or `not_attempted`. It is the compiler's recorded outcome. `owner_not_loaded` means no owner read the allomorph, so the compiler recorded nothing for it. `not_attempted` means the compiler has no path that reads this gate on this allomorph kind. `required_features` (`MsEnvFeatures`) is an affix-allomorph gate in FieldWorks data; a process or stem allomorph that carries it is published `not_attempted`. A circumfix's prefix-half classes are `applied`; its suffix-half classes are `ignored` with reason `circumfixSuffixClassesNotRead`, and its halves' features are `ignored` with reason `circumfixIgnoresAllomorphGates`. `required_category` (`MsEnvPartOfSpeech`) is always `ignored` with reason `msEnvPartOfSpeechNotRead`, because no compiler path reads it. An allomorph under several MSAs reports its strongest outcome. |
| `allomorph_form` | `(allomorph_guid, ordinal)`, `writing_system`, `form`; all forms are retained in source order. |
| `entry_citation_form` | `(entry_guid, ordinal)`, `writing_system`, `form`; the source order is preserved. |
| `msa` | `msa_guid`, owning `entry_guid`, and `kind` (`stem`, `inflectional`, `derivational`, `unclassified`). |
| `msa_category` | `(msa_guid, role, ordinal)`, `category_guid`; role is `pos`, `from_pos`, `to_pos`, or `clitic_from`. |
| `msa_slot` | `(msa_guid, role, ordinal)`, `slot_guid`; role is `slot` or `clitic_slot`. |
| `msa_inflection_class` | `(msa_guid, role)`, `class_guid`; role is `class`, `from_class`, or `to_class`. |
| `msa_stem_name` | `(msa_guid, role)`, `stem_name_guid`; role is `from_stem_name`. |
| `msa_exception_feature` | `(msa_guid, role, ordinal)`, `target_guid`; role is `required`, `from_required`, or `to_required`. |
| `exception_feature` | `guid`, `name`, `abbreviation` for each productivity-restriction (exception) feature, the targets of the exception-feature references elsewhere. |
| `stem_name` | `guid`, owning `category_guid` (the part of speech that lists it), `name`, nullable `abbreviation`, and `nonempty_region_count`. The count is the regions whose feature structure has at least one assignment; a stem name with zero is dropped by the compiler. |
| `stem_name_region` | `(stem_name_guid, ordinal)`, `fs_id`; each region is a `feature_structure` with owner `stemName`, role `region`, and path equal to the region ordinal. Empty regions are published too. |
| `lex_entry_infl_type` | `guid`, `name`, `abbreviation`, and nullable `fs_id` for the type's inflection features (owner `inflType`, role `features`). Gloss prepend and append are display-only and not published. |
| `lex_entry_infl_type_slot` | `(infl_type_guid, ordinal)`, `slot_guid`; the slots the irregular form fills, in source order. The slot has no foreign key. |
| `sense` | `sense_guid`, owning `entry_guid`, nullable source `msa_guid`. The MSA target is not a foreign key. |
| `entry_variant` | `(variant_entry_guid, ref_guid, ordinal)`, `component_guid`, and `component_kind` (`entry`, `sense`, or `unresolved`). Each row is one component lexeme of a variant link (`LexEntryRef` with variant types); a component that is neither a loaded entry nor a sense is `unresolved`. Complex-form links are not published. |
| `entry_variant_type` | `(ref_guid, ordinal)`, `type_guid`, and `is_infl_type` (1 when the type is a `lex_entry_infl_type`). This is the only table that holds variant types. |
| `sense_text` | `(sense_guid, kind, ordinal)`, `writing_system`, `text`; kind is `gloss` or `definition`. |
| `adhoc_prohibition` | `prohibition_guid`, `kind` (`allomorph` or `morpheme`), `disabled`, `adjacency`, `primary_guid`, `target_kind` (`allomorph` or `msa`). |
| `adhoc_other` | `(prohibition_guid, ordinal)`, `target_guid`, `target_kind`. The ordinal and conjunctive target order are preserved. |
| `adhoc_group` | `group_guid`; one row per authored group present in the Snapshot. |
| `adhoc_group_text` | `(group_guid, field, writing_system)`, where field is `name` or `description`; `text` stores that alternative. Writing-system alternatives have no artificial order. |
| `adhoc_group_member` | `(group_guid, member_guid)`; one row per member reference. The target is intentionally not a foreign key and there is no ordinal because LCM `Members` is a collection. A target may identify a prohibition, nested group, or unresolved object. |

### Phonology, environments, and patterns

| Table | Key and columns |
|---|---|
| `phoneme_set` | Singleton and nullable first phoneme-set GUID from the Snapshot. |
| `phoneme` | GUID, nullable feature-structure ID, name, and optional basic IPA symbol. |
| `phoneme_grapheme` | `(phoneme_guid, ordinal)`, writing-system tag, and grapheme; order follows Snapshot forms. |
| `boundary_marker`, `boundary_grapheme` | Boundary GUID/name and ordered writing-system spellings. |
| `feature` | GUID, feature system (`phonological` or `morphosyntactic`), kind (`closed` or `complex`), labels, and nullable feature-type GUID. |
| `feature_value` | GUID, owning feature, ordinal, name, and abbreviation for a closed value. |
| `feature_structure` | Integer ID plus feature system, typed source owner, role, and nested path. Owners include `msa` (roles `features`, `from_features`, `to_features`), `allomorph` (role `required_features`, the allomorph's `MsEnvFeatures`), `stemName` (role `region`), and `inflType` (role `features`). Feature structures have no source GUID. |
| `feature_assignment` | `(fs_id, ordinal)`, feature GUID, value kind, and either a closed value GUID or child feature-structure ID. Source feature/value references are retained without foreign keys. |
| `natural_class` | GUID, kind (`segments` or `features`), abbreviation/name, display name, and nullable feature-structure ID. |
| `natural_class_member` | `(natural_class_guid, ordinal)`, explicit source phoneme GUID; dangling member references are retained. |
| `natural_class_effective_member` | `(natural_class_guid, table_key, member_key)`, the compiler's final class extension for each character table. `member_key` is typed JSON identity (`object` with a source GUID or `synthetic` with a compiler key); `phoneme_guid` is set only for source phonemes. `match_kind` records segment-list or feature matching. `match_basis` is `listed` for a segment list, `specified` when a feature match came from a value the member has, and `underspecified` when at least one feature matched only because the member's lane defaulted to the full mask. |
| `feature_constraint` | GUID and feature GUID for alpha-variable constraints. |
| `allomorph_environment` | `(allomorph_guid, role, ordinal)`, with role `phone` or `position`; the environment target is deliberately not a foreign key so dangling references remain visible. |
| `environment` | GUID, name, raw representation, parse status (`valid`, `invalid`, `not_attempted`, or `unavailable`), and nullable parse error code/text. An invalid status means the compiler rejected the whole expression. |
| `environment_usage` | `(allomorph_guid, role, ordinal, compile_context_key)`, raw and resolved environment GUIDs, `compiled`, outcome (`represented`, `invalid`, `unresolved`, `owner_not_loaded`, or `not_attempted`), and optional exact compiler load-decision identity. `compiled=1` is reserved for a represented compiler decision. |
| `environment_natural_class` | `(environment_guid, compile_context_key, side, token_path)`, token text, context-side Unicode-scalar span, nullable winning natural-class GUID, and token result. Repeated class tokens remain separate rows. The winner and whole-expression result come from the compiler's shared resolution computation. |
| `environment_side` | `(environment_guid, side)` for valid environments only, with `canonical_key`, the lowercase SHA-256 of the side's resolved tokens, and `shape`. Each class token is replaced by its sorted member keys, and spacing and class names never reach the key. `shape` is `empty`, `word_boundary` (only `#`), `single_segment` (one segment or class, with at most one `#`), or `complex`. An optional `( ... )` group is `complex` wherever it appears. Interior `#` is dropped by the compiler, as HCLoader drops it, so `X#` and `X` key alike on a side where the `#` is not at the outer edge. |
| `environment_side_member` | Members of `single_segment` and `word_boundary` sides only: `member_kind` (`phoneme`, `boundary` or `synthetic`), `member_key` (the same identity as `natural_class_effective_member`, and `{"kind":"word_boundary"}` for a `#`), and nullable `phoneme_guid` and `boundary_guid`. A complex side has no member rows, so containment for it is reported as not checked. |
| `phonological_rule` | GUID, name, kind (`rewrite` or `metathesis`), direction, source order, and nullable effective stratum key. |
| `phonological_rule_variable`, `rewrite_rhs`, `rewrite_rhs_pos`, `rewrite_rhs_rule_feature` | Ordered rewrite variables, RHS branches, required POS values, and required/excluded rule-feature targets with target kind (`inflectionClass`, `exceptionFeature`, `featureValue`, or `unresolved`). Each `rewrite_rhs` row carries nullable `change_root_id`, `left_context_root_id` and `right_context_root_id`, each a `pattern_root` ID for that side when the RHS has it. |
| `affix_process_input` | `(allomorph_guid, part)`, 1-based part number as `CopyFromInput` and `ModifyFromInput` refer to it, `is_variable` (1 for a wildcard `Variable` part), and nullable `pattern_root_id` for a concrete context (`owner_kind='affixProcess'`, `role='process_input'`, ordinal `part - 1`). |
| `affix_process_output` | `(allomorph_guid, ordinal)`, `kind` (`copy`, `insert_segments`, `insert_class`, or `modify`), nullable input `part` (copy and modify), nullable `natural_class_guid` (insert_class and modify), and nullable literal `text` (insert_segments). |
| `compound_rule` | GUID, `kind` (`endocentric` or `exocentric`), name, `disabled`, nullable `head_last` (endocentric only; 1 means the right constituent is the head), and nullable `max_applications` from the Snapshot's per-rule parser parameter. `max_applications` is NULL when the Snapshot carries no value for the rule, so the compiler's default of one is not written here. |
| `compound_rule_side` | `(rule_guid, side)`, `side` is `left`, `right` or `outcome`, nullable `category_guid`, and nullable `inflection_class_guid` (outcome only). Constituent sides carry only a category. |
| `compound_rule_exception_feature` | `(rule_guid, side, ordinal)`, `side` is `left` or `right`, and `target_guid`. Exception features are constituent requirements, so the outcome has none. Linker and to-product restriction are not in the Snapshot and are not published. |
| `stratum`, `rule_stratum` | Compiler stratum key/order/name/table and each rewrite rule's order within it. Generated stratum keys are not GUIDs. |
| `pattern_root` | Integer ID, owner kind/GUID, role, ordinal, and source kind (`authored` or `resolved_environment`). |
| `pattern_node` | Integer ID, root and nullable parent node, sibling ordinal, pattern kind, quantifier bounds, and optional source phoneme/class/boundary or literal token. Compiled literal-segment nodes have ordered phoneme/boundary children showing the compiler's segmentation. A parent foreign key keeps each tree attached to its root. |
| `pattern_variable` | `(node_id, polarity, ordinal)`, feature-constraint GUID for plus/minus variables. |

Pattern roots use `environment_left` and `environment_right` for resolved environment sides,
`rewrite_lhs`, `rewrite_sc`, `rewrite_left_context`, and `rewrite_right_context` for rewrite rules,
`metathesis_pattern` for metathesis rules, and `process_input` for concrete affix-process input
parts (`owner_kind='affixProcess'`, owner GUID the allomorph GUID). Rewrite RHS roles use the RHS
ordinal, and `rewrite_rhs` links each side's root by ID; other roles use ordinal zero. Environment roots have `owner_kind='environment'` and
`source_kind='resolved_environment'`; rule roots have `owner_kind='phonologicalRule'` and
`source_kind='authored'`. Authored node kinds preserve sequence, iteration, phoneme, natural class,
boundary, word boundary, and variable contexts. Resolved environment nodes preserve anchors,
optional quantifiers, natural class references, and literal segments.

Environment parsing is lazy like the production compiler: unused definitions have
`parse_status='not_attempted'`. Phone and position edges stay distinct. A dangling allomorph
reference appears in `allomorph_environment` and `environment_usage` with a NULL resolved GUID.
Unknown natural-class tokens have a token row with `result='unresolved'`; earlier successfully
resolved tokens remain visible when a later token makes the entire environment invalid. Empty sides
have no pattern root. The environment token spans are zero-based Unicode-scalar offsets within the
trimmed left or right context side, not byte offsets into the whole expression.

### Provenance and load facts

| Table | Key and columns |
|---|---|
| `source_census` | Singleton `total_occurrences` and `ordered_header_sha256` from Snapshot provenance. |
| `source_class_count` | `class_name`, `occurrences`, `unhandled_occurrences`. |
| `source_object` | Grammar objects only (`inventory_kind` not null): `source_key`, 1-based `source_ordinal`, raw `class_name` and `raw_guid`, nullable `canonical_guid` and `inventory_kind`, plus `handled`, `retained`, and `duplicate` flags from the source reader. |
| `conversion_item` | `(pipeline_stage, inventory_stage, subject_kind, subject_key)`, nullable `subject_guid`. Pipeline stage is `import`, `snapshot`, `compile`, or `compact`; inventory stage is `authored`, `considered`, `selected`, `represented`, `rejected`, or `synthesized`. |
| `conversion_stage` | `(pipeline_stage, inventory_stage)`, `item_count`; includes zero-count rows for declared stage pairs. |
| `conversion_issue` | `issue_key`, `pipeline_stage`, stable code/class/fatal fields, optional source kind/GUID, and message. |
| `load_fact` | `(subject_kind, subject_key, pipeline_stage, context_key, decision_ordinal)`, optional GUID, disposition, nullable `loaded`, required `reason_code`, optional effective JSON and issue link. |
| `compiled_mapping` | `(source_kind, source_key, output_kind, output_key)`, optional source GUID, and output identity quality (`authored`, `structural`, or `synthetic`). Rows are emitted from final compiler associations after compaction; circumfix source halves can map to the same output, and one source can map to several expanded outputs. |
| `compiled_allomorph_order` | `(owner_key, source_allomorph_key, output_key)`, source entry/MSA and bucket, nullable source allomorph GUID, nullable final order, and `is_final_elsewhere_case`. Order follows the final compiler vector. A circumfix product can have one row per source half at the same order; group by `owner_key` and `output_key` to count compiler outputs. Empty `source_allomorph_key` or `output_key` denotes an absent optional identity. `is_final_elsewhere_case` is set only for the final sibling when its compiled environment list is empty; it identifies the possible unconditioned fallback in that ordered set. A NULL order marks an owner-published source candidate that has no compiled output in that context. Include `bucket` in queries because one source allomorph can be considered in both morphology and clitic buckets. |
| `compiled_form_segment` | `(output_id, ordinal)`, the segments of each compiled allomorph, with `segment_kind` `phoneme`, `boundary`, `synthetic`, `natural_class` (a class insert, with `natural_class_guid`) or `variable` (a copy or modify of the input, in process outputs only). A `boundary` row carries `boundary_guid` when the boundary is a source marker; the compiler's synthetic morpheme boundary, which is inserted at an affix's seam, has no GUID and is identified by `token_text` `+`. A root publishes its shape, and a bracket pattern publishes none. A concatenative affix publishes its inserted shape, including its boundary, but not its copy of the stem. `token_text` is the character definition's first representation; it is NULL for `variable` and `natural_class` rows. A root or affix whose shape has a node with no character definition publishes no rows for that allomorph. |
| `parser_config` | Setting key, normalized Snapshot value, effective compiled value, and source-presence status. `not_retained` means the Snapshot did not preserve the original XML presence/default distinction. |

`subject_key` is JCS serialization of the complete typed compiler `InventoryKey`; `subject_kind`
uses its camelCase enum value. `context_key` is empty for global decisions; allomorph form-bucket
decisions use `lexEntryForm:morphology` or `lexEntryForm:clitics`. Ordinals are
owner-local per subject, pipeline stage and context. A missing owner decision is represented as
`disposition='unknown'`, `loaded=NULL`, `reason_code='decision_unrecorded'`. Dispositions are
`represented`, `rejected`, `defaulted`, `synthesized`, `compacted`, `metadata_only`,
`not_considered`, and `unknown`. `loaded` is NULL for `not_considered` and `unknown`; `loaded=1`
means the owner reported representation or synthesis at that stage. It does not mean the construct
was used by a parse. A `compact` row supersedes earlier compile presence for final availability.
`source_object.source_key` is the same canonical typed serialization shape, using the
`sourceObject/sourceOccurrence` identity with its source ordinal, class, raw GUID, and optional
canonical GUID. Import decisions for malformed or repeated headers use this key so occurrences
with no GUID or the same GUID stay distinct. Ordinary object decisions use the tracked grammar
kind and authored GUID. Import reason codes include `represented`, `disabled`, `unreferenced`,
`additional_phoneme_set_not_selected`, `missing_guid`, `duplicate_header`, `unknown_class`,
`missing_reference`, and `wrong_kind_reference`; unsupported or uninstrumented owner decisions
remain explicit unknowns.

An identity tagged `synthetic` carries a stable compiler-owned key for an object with no authored
GUID (for example, the standalone dot boundary); its `subject_guid` is NULL. Synthetic keys remain
typed identities and must never be treated as FieldWorks GUIDs. `setting` identities remain
reserved for named source-wide settings.

`conversion_issue.issue_class` is one of `malformedSource`, `invalidSource`, `ambiguousSource`,
`unrepresentableForHc`, `substrateUnresolvable`, `migrationDifference`, or
`unreachableInGrammar`. `source_kind` uses the same camelCase `InventoryKind` spelling as
`conversion_item.subject_kind`. The checked-in DDL defines each SQLite type, nullability rule,
primary key, foreign key, index, and `CHECK` constraint.

### Frozen parser stats

Stats are optional and immutable in a facts file. Without both stats flags, all run-specific tables
are empty, `artifact_meta.run_manifest_sha256` is NULL, and `artifact_section.stats` is
`not_requested`. `stats_counter_support` is still populated because it describes what the current
collector can measure. With a valid cache/manifest pair, the stats section is `complete` even when
some requested words are incomplete; word-level completion is represented explicitly and must be
checked before using those words as completed parser evidence.

| Table | Key and columns |
|---|---|
| `stats_cache_identity` | `(singleton INTEGER PRIMARY KEY CHECK(singleton=1), cache_sha256 TEXT NOT NULL, cache_bytes INTEGER NOT NULL CHECK(cache_bytes>=0), schema_version INTEGER NOT NULL CHECK(schema_version>0), counter_semantics INTEGER NOT NULL CHECK(counter_semantics>0), engine TEXT NOT NULL CHECK(engine='hc'), grammar_hash TEXT NOT NULL)`. One row records the closed cache bytes. |
| `stats_run` | `run_id INTEGER PRIMARY KEY CHECK(run_id>0)`; `engine`, `grammar_hash`, `options_hash`, `options_json`, `created_utc`, nullable `step_cap`, `cache_word_count`, `requested_word_count`, and `batch_options_json`. Engine is `hc`; cap is NULL, `-1`, or positive; counts are nonnegative and requested count is at least cache count. One row records exact run metadata and manifest batch options. |
| `stats_morpheme`, `stats_stratum`, `stats_allomorph` | Each has a nonnegative integer primary-key ID, nullable `key TEXT`, `label TEXT`, and `identity_quality TEXT` checked against `authored`, `structural`, `synthetic`. ID zero has NULL key and quality; positive IDs require key, label, and quality. ID zero is the not-applicable dimension sentinel. Guessed-root dimensions use synthetic identities; strata and compiled allomorphs use structural identities. |
| `stats_object` | `object_id INTEGER PRIMARY KEY CHECK(object_id>0)`, nonnull `key`, `kind` checked against `morph_rule`, `phon_rule`, `lex_entry`, `root_index`, `guesser`, `overlay`, nonnull `label`, `identity_quality` checked against `authored`, `structural`, `synthetic`, and `morpheme_id` referencing `stats_morpheme`; `(kind,key)` is unique. |
| `stats_object_source` | `(object_id, source_kind, source_guid, role)` primary key, `object_id` references `stats_object`; `source_kind` is `entry`, `msa`, `phonologicalRule`, or `compoundRule`. Roles are `owner_entry`, `morpheme_msa`, `msa_owner_entry`, `source_form_owner_entry`, `phonological_rule`, or `compound_rule`. GUIDs have no foreign key. It is `WITHOUT ROWID`, with index `stats_object_source_guid(source_kind,source_guid)`. |
| `stats_allomorph_source` | `(allomorph_id, source_ordinal)` primary key; `allomorph_id` references `stats_allomorph`; ordinal is nonnegative; GUID is nullable; `omitted` is 0/1 and can be 1 only with NULL GUID. Circumfix products retain both source halves in order. Omitted forms use role `null_affix`; unresolved compiler source forms use `unresolved_source_form`; other roles are `stem`, `bound_stem`, `root`, `bound_root`, `prefix`, `suffix`, `infix`, `circumfix`, `proclitic`, `enclitic`, `clitic`, `particle`, `phrase`, `discontig_phrase`, `prefixing_interfix`, `infixing_interfix`, or `suffixing_interfix`. `WITHOUT ROWID`, with index `stats_allomorph_source_guid(source_allomorph_guid)`. |
| `stats_word` | Positive integer `word_id` primary key; `run_id` references `stats_run`; exact `form`; status checked against `complete`, `incomplete`, `invalid_shape`, `not_attempted`; nullable nonnegative `elapsed_ns`, `attempts`, `passes`; nullable 0/1 `capped`, `timed_out`, `invalid_shape`. `(run_id,form)` is unique. `not_attempted` rows have all measurements NULL; other statuses require all measurements and flags non-NULL. |
| `stats_fact` | `(word_id, object_id, stratum_id, allomorph_id, direction)` primary key; all four IDs reference their dimension/object tables; direction is `analysis` or `synthesis`; attempts, work, outputs, not-applied, no-root, surface-mismatch, uses, self-time nanoseconds are nonnegative `INTEGER NOT NULL`. `WITHOUT ROWID`, with indexes `stats_fact_object(object_id,direction)`, `stats_fact_stratum(stratum_id,direction)`, and `stats_fact_allomorph(allomorph_id,direction)`. |
| `stats_counter_support` | `(engine, counter_semantics, object_kind, counter, direction)` primary key; engine `hc`; object kinds as `stats_object.kind`; counter is attempts, work, outputs, not-applied, no-root, surface-mismatch, uses, or self-time; direction is `both`, `analysis`, or `synthesis`; support is `measured`, `not_applicable`, or `not_wired`. Logical counters use `both`; self-time rows require one concrete direction. `WITHOUT ROWID`. |

The checked-in DDL is normative for SQLite types, nullability, checks, keys, foreign keys, and
indexes. The cache's dense integer handles are not copied as identities. Object and dimension rows
are validated against `pg-grammar::stats_identity`, sorted by typed key, then assigned deterministic
local IDs; counter rows use those local IDs. The object identity key format belongs to that API.
Source bridges use a separate exact-GUID membership check against the Snapshot or compiler's
allomorph source records. Structural and synthetic identities are never presented as authored
GUIDs. No counter is inferred, backfilled, or fabricated for a collector path that does not publish
it.

The version 1 manifest is closed JSON. It carries `format`, `version`, and source, compiler, cache,
run, batch, input, and completion objects. Source identity includes source kind, exact source digest,
grammar hash, model fingerprint, and canonical production compile options. Cache identity includes
the closed-file digest, byte count, schema version, and counter-semantics version. Run identity
includes the single run ID, engine, grammar hash, options hash, and exact options JSON. Input carries
the ordered effective forms, count, and digest of compact JSON encoding. Completion carries aggregate
requested/complete/incomplete/invalid-shape/missing counts and an ordered row per input with status
and cap/timeout/shape flags. Unknown manifest properties are rejected. See
[`batch-stats-manifest.md`](../rust/docs/batch-stats-manifest.md) for the producer contract.

This query joins counters only to one-to-one tables and includes only completed words and measured
`work` support:

```sql
SELECT w.form, o.kind, o.key, f.direction,
       SUM(f.attempts) AS attempts, SUM(f.work) AS work, SUM(f.outputs) AS outputs
FROM stats_fact AS f
JOIN stats_word AS w USING (word_id)
JOIN stats_object AS o USING (object_id)
JOIN stats_counter_support AS s
  ON s.engine='hc' AND s.counter_semantics=(SELECT counter_semantics FROM stats_cache_identity)
 AND s.object_kind=o.kind AND s.counter='work' AND s.direction='both'
WHERE s.support='measured' AND w.status='complete'
GROUP BY w.form, o.kind, o.key, f.direction;
```

Do not join `stats_fact` directly to `stats_object_source` or `stats_allomorph_source` before
summing counters: a source can map to several compiled objects, and a circumfix output has two
source allomorphs. Use `EXISTS` when filtering counter rows by a source GUID, or aggregate counters
first and display source associations in a separate query. A source bridge says which authored
objects contributed to a compiled identity; it does not distribute a counter among those source
objects. A complete stats section also does not mean every word completed.

For example, filter to all objects associated with one source MSA without multiplying the work
when several source rows map to the same compiled object:

```sql
SELECT o.kind, o.key, SUM(f.work) AS work
FROM stats_fact AS f
JOIN stats_object AS o USING (object_id)
WHERE EXISTS (
  SELECT 1 FROM stats_object_source AS s
  WHERE s.object_id=o.object_id AND s.source_kind='msa' AND s.source_guid=?1
)
GROUP BY o.kind, o.key;
```

## Reader rules

- Check `PRAGMA application_id`, `PRAGMA user_version`, the matching metadata values, and
  `complete=1` before reading facts. Schema v8 has no migration or upgrade path; rebuild from the exact
  Snapshot and context when the schema or model fingerprint differs.
- Check `artifact_section` for every prerequisite. An unavailable ad hoc group section does not
  mean the source had no groups. A NULL `template_slot.compiled_order` means no final order was
  published for that authored occurrence; consult its template state and compiler `load_fact` rows.
- Preserve ordered target lists. One row in `adhoc_prohibition` plus several `adhoc_other` rows is
  one conjunctive rule, not several pairwise rules.
- Treat `loaded=NULL` and `disposition='unknown'` as unknown. Do not turn missing owner reasons or
  unsupported sections into zero counts.
- Never infer compiler outcomes by parsing `conversion_issue.message`; `code`, `issue_key`, and
  `load_fact` are the machine-readable fields.

### Ordered prohibition read

Read each prohibition and its ordered target rows before joining load decisions. A prohibition can
have several target rows and several load-decision rows; joining both directly multiplies rows.

```sql
SELECT p.prohibition_guid, p.kind, p.disabled, p.adjacency, p.primary_guid,
       o.ordinal, o.target_guid, o.target_kind
FROM adhoc_prohibition AS p
LEFT JOIN adhoc_other AS o USING (prohibition_guid)
ORDER BY p.prohibition_guid, o.ordinal;
```

For `P-adhoc-duplicate`, this fixed query returns the exact signature fields, the ordered target
list as a JSON array, and the compiler's published load decision. Its `kind` distinguishes an
allomorph primary from an MSA primary; `primary_guid` remains the authored target identity.

```sql
SELECT p.kind, p.primary_guid,
       (SELECT json_group_array(target_guid)
        FROM (SELECT target_guid FROM adhoc_other AS o
              WHERE o.prohibition_guid=p.prohibition_guid ORDER BY o.ordinal)) AS ordered_other_guids,
       p.adjacency, p.disabled=0 AS enabled, f.disposition AS loader_disposition,
       f.loaded AS loader_loaded, f.reason_code AS loader_reason
FROM adhoc_prohibition AS p
LEFT JOIN load_fact AS f
  ON f.subject_guid=p.prohibition_guid
 AND f.subject_kind=CASE p.kind
      WHEN 'allomorph' THEN 'allomorphCoOccurrence'
      ELSE 'morphemeCoOccurrence'
     END
 AND f.pipeline_stage=CASE
      WHEN EXISTS (SELECT 1 FROM load_fact AS c
                   WHERE c.subject_guid=p.prohibition_guid
                     AND c.subject_kind=f.subject_kind
                     AND c.pipeline_stage='compact') THEN 'compact'
      ELSE 'compile'
     END
ORDER BY p.prohibition_guid;
```

Read group rationale and membership independently from the signature query so a rule belonging to
several groups does not multiply its signature rows.

```sql
SELECT m.member_guid, g.group_guid, t.field, t.writing_system, t.text
FROM adhoc_group_member AS m
JOIN adhoc_group AS g USING (group_guid)
LEFT JOIN adhoc_group_text AS t USING (group_guid)
ORDER BY m.member_guid, g.group_guid, t.field, t.writing_system;
```

Read compiler disposition rows separately, then resolve final presence using `compact` when it
exists and `compile` otherwise. Disabled rows have `disposition='not_considered'` and
`loaded=NULL`; an enabled row with `loaded=1` means the compiler represented the prohibition, not
that a parse used it.

The `reason_code` strings include `represented`, `disabled`, `synthesized`, `metadataOnly`, `abstract`,
`emptyForm`, `morphTypeNotInBucket`, `unreferenced`, `noEligibleAllomorph`, `ownerNotLoaded`,
`ancestorDefaultInflectionClass`, `additional_phoneme_set_not_selected`, `missing_guid`,
`duplicate_header`, `unknown_class`, `missing_reference`, `wrong_kind_reference`, `decision_unrecorded`,
`source_inventory_unknown`, or a stable code from `pg_snapshot::ImportWarningCode::wire()`. Compiler-owned
conversion reasons include `affixProcessUnsupportedArity`; readers retain unrecognized reason strings
and do not derive new meanings from issue messages. Import inventory finalization may also emit
`sourceObjectNotResolved` for a referenced context or rule feature that no importer owner loaded.
Unknown and uninstrumented inventory keys carry `decision_unrecorded`; readers retain unrecognized
reason strings and do not derive new meanings from issue messages.
