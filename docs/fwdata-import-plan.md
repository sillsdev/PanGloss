# Direct FieldWorks project ingestion (`.fwdata` → PanGloss)

**Status: approved plan, implementation in progress (branch `worktree-fwdata-import`).**

## 1. Motivation

Today PanGloss ingests a HermitCrab grammar XML file exported from a FieldWorks project by
FieldWorks-side tooling (`Src/GenerateHCConfig`: `LcmCache` → `HCLoader.Load` →
`XmlLanguageWriter.Save`), plus sidecar files (`*-lexical.json` gloss data, `*-realize.toml`
feature maps). This plan replaces that input path: **PanGloss ingests the FieldWorks project
file (`.fwdata`) directly.**

Why:

1. **No changes to the FieldWorks repo.** The whole conversion moves into PanGloss; FieldWorks
   is just a data source.
2. **Engine plug-and-play.** The import produces an engine-agnostic snapshot of the project's
   lexicon/morphology/phonology. HermitCrab is one compiler backend over it; XAMPLE-type
   engines can be others, without HermitCrab as an intermediate.
3. **Robustness.** The C# export path is fragile: `GenerateHCConfig.exe` currently *crashes*
   on the Amharic sample project (stale `MoMorphAdhocProhib` → `KeyNotFoundException` in
   `XmlLanguageWriter.WriteMorphemeCoOccurrenceRule`). Verified 2026-07-14. Our importer must
   log-and-skip dangling references, never crash.

**We own the internal format** (decision 2026-07-14): the snapshot is a PanGloss-defined,
versioned **JSON** format. It is *not* a mirror of LCM/M3Dump naming; `.fwdata` is merely the
first source that maps into it. JSON over YAML because the artifact is machine-generated,
`serde` is already in the workspace (hc-wasm), and the browser demo can fetch it directly.
The existing HC-XML path (`hc_grammar::load`) stays intact — conformance fixtures and the
machine-submodule oracle still use it.

## 2. Architecture (three layers, two new crates + one new module)

```
.fwdata  ──(1) pg-fwdata──►  Snapshot (pg-snapshot JSON)  ──(2) hc_grammar::compile──►  Grammar
                                       │
                                       └──(future) XAMPLE-style backends, demo gloss data
```

1. **`rust/crates/pg-fwdata`** — parses `.fwdata` XML (flat `<rt class= guid= [ownerguid=]>`
   elements, `<objsur t="o|r">` links, `<AUni ws>`, `<AStr>/<Run>`, `<Str>/<Run>`,
   `<Field val=…/>`) into an in-memory object graph, then *extracts* the parser-relevant
   subset into a `Snapshot`. Uses `quick-xml` (already a workspace dep; Sena 3 is 54 MB —
   stream, don't DOM the whole file). Tolerant: dangling `objsur` targets, missing fields,
   and stale ad-hoc prohibitions produce warnings in an import report, never panics/errors.
2. **`rust/crates/pg-snapshot`** — the owned format: serde model + JSON (de)serialization +
   validation + format documentation. Versioned (`"format": "pangloss-project"`,
   `"version": 1`). Keyed by FieldWorks GUIDs (stable across sessions — unlike the `Hvo`
   integers in the legacy XML export, which drift per load).
3. **`hc_grammar::compile` module (new, in `rust/crates/hc-grammar`)** — compiles a
   `Snapshot` into the existing immutable `model::Grammar`, sibling to `load.rs` (reuses its
   internal construction machinery: interners, pattern builders, chardef/featsys/segment).
   Public entry: `hc_grammar::compile_project(&Snapshot) -> Result<Grammar, GrammarError>`.
   Semantically this is a Rust port of FieldWorks' `HCLoader.cs` (see §4).

CLI: `hc-rs import <project.fwdata> <out.json>` plus `parse`/`batch`/`fst-stats` accepting a
`.json` snapshot wherever they accept a grammar XML path (dispatch on extension).
wasm/demo integration and an XAMPLE backend are explicit follow-ups, not in this branch.

## 3. Snapshot format (v1) — content outline

Engine-agnostic, LCM-*informed* but PanGloss-*named*. Everything GUID-keyed. Only
parser-relevant data (no texts, wordform analyses, semantic domains, styles…). Glosses ride
along (replaces the `*-lexical.json` sidecar). Top-level sections:

- `project`: name, vernacular/analysis writing-system tags.
- `featureSystems`: `phonological` and `morphosyntactic` — closed features with value
  symbols, complex features (recursive types).
- `phonology`: phonemes (grapheme representations per WS + feature values), boundary
  markers, natural classes (segment-based and feature-based), environments (name + raw
  string representation, e.g. `/_[UnVDent]` — parsed by compilers, tolerantly), phonological
  rules (regular rewrite rules with structured contexts, metathesis rules), feature
  constraints.
- `morphology`: parts of speech (hierarchy, inflection classes + defaults, inflectable
  features, stem names with regions, affix slots, affix templates with prefix/suffix slot
  order), morph types (by well-known FieldWorks morph-type GUID → a PanGloss enum: stem,
  bound stem/root, prefix, suffix, infix, circumfix, proclitic, enclitic, clitic, particle,
  phrase…), compound rules (endo/exo, left/right head), ad-hoc co-occurrence prohibitions
  (allomorph and morpheme, with adjacency), lexEntryInflTypes (irregularly-inflected-form
  variant types with GlossPrepend/GlossAppend and slot lists), parser parameters (the
  `<ParserParameters><HC>` block: NotOnClitics, AcceptUnspecifiedGraphemes,
  NoDefaultCompounding, Strata, per-rule maxApps).
- `lexicon`: entries — lexeme form, citation form, morph type, allomorphs (form per WS,
  morph type, environments, stem name, `IsAbstract`, bound-ness), MSAs (tagged union: stem /
  inflectional / derivational / unclassified — POS ref, inflection class, exception
  "production restriction" features, feature structures, slot refs, from/to POS for deriv),
  senses (gloss, definition, MSA ref), variant/complex-form references (`LexEntryRef`s with
  component refs and inflType refs).

Field-by-field spec is authored as part of `pg-snapshot` (see `docs/snapshot-format.md`
written with that crate).

## 4. HC compilation semantics — port of `HCLoader.cs`

Reference sources (read-only, outside this repo/worktree):

- `C:\Users\johnm\Documents\repos\FieldWorks\Src\LexText\ParserCore\HCLoader.cs` (2837
  lines) — **the spec** for LCM→HermitCrab semantics. Key regions (line refs valid at
  FieldWorks HEAD 2026-07-14): `LoadLanguage()` driver 164–357; user strata `CreateStrata`
  359–473; rule-form validity 536–569; stems `LoadLexEntries`/`LoadLexEntry` 626–731;
  variants 733–807; root allomorphs 809–845; affix rules 847–1046; affix-process allomorphs
  incl. circumfix cross-product 1048–1332; reduplication 1446–1671; templates 1673–1735;
  null-affix synthesis for irregular slots 1771–1806; compounding 1808–2001; rewrite rules
  2003–2101; metathesis 2103–2161; ad-hoc rules 2163–2258; environment string → pattern
  tokenizer 2260–2457; feature structures 2500–2530; char-def table 2669–2743; natural
  classes 2788–2829.
- `C:\Users\johnm\Documents\repos\PanGloss\machine\src\SIL.Machine.Morphology.HermitCrab\`
  (main checkout; the submodule is NOT populated in this worktree) — the HC object model +
  `XmlLanguageLoader.cs` that `rust/crates/hc-grammar/src/load.rs` already ports 1:1.
- `C:\Users\johnm\Documents\repos\FieldWorks\Localizations\LCM\src\SIL.LCModel\MasterLCModel.xml`
  — authoritative LCM schema (field names, owning vs reference, cardinality). The raw
  `.fwdata` XML does not carry this metadata; `pg-fwdata` hardcodes what it needs for the
  classes it reads, guided by this file.

Compilation-order and semantics follow `LoadLanguage()`: MPR feature groups (inflection
classes, exception features, lexEntryInflTypes) → POS + head features → feature systems →
character-definition table from phonemes (+ boundary symbols `^0 * 0 &0 ∅`, morph boundary
`+`, space→`.` replacement, dotted-circle U+25CC stripping) → stem names → strata
(`Morphology`, `Clitics`, `Surface`, reorganized by the `Strata` parser param) → compound
rules (defaults if none) → lex entries (stem vs rule-form classification by morph-type GUID)
→ affix templates (slots with loaded affixes only; null-affix injection for irregular-form
slots) → phonological rules → ad-hoc co-occurrence rules.

Normalization subtleties to preserve (each has a dedicated HCLoader region above):
environment-string tokenization (`#`, `[NC]`, `(…)` optionals) with validity re-checking;
circumfix prefix×suffix allomorph cross-products; `[…]` lexical patterns → full-stem
reduplication and indexed-NC reduplication patterns; partial entries when an MSA lacks POS
(`IsPartial`, not an error); inflection-class defaulting up the POS ownership chain;
variant entries via `LexEntryRef` with gloss prepend/append; bound roots/stems.

**Phasing inside the compiler**: Phase A covers everything the Sena 3 and Amharic fixtures
exercise (features, phonemes, stems, environments, infl/deriv/unclassified affixes,
templates, compounding, rewrite rules, ad-hoc rules, strata, variants). Phase B covers the
rest (metathesis, reduplication, circumfix, clitic strata subtleties, user-defined strata
strings) — implemented if fixture-exercised, otherwise emitting
`GrammarError::Unsupported`-style warnings with a clear message, mirroring the existing
loader's managed-fallback lint philosophy.

## 5. Verification

Baseline: `cargo test --workspace` green at branch point (verified).

1. **Unit fixtures**: a small hand-written `.fwdata` fixture committed under
   `rust/crates/pg-fwdata/tests/data/` (synthesized, not copied from FieldWorks — a few
   entries, one template, one phon rule, one environment) driving parser + extractor +
   compiler unit tests.
2. **Oracle conformance (self-skipping, like the existing `sample_path()` tests)**: with the
   FieldWorks repo present (`PANGLOSS_FW_PROJECTS_DIR` env var, or the known sibling path
   `C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects`), import `Sena 3.fwdata`
   and `Amharic.fwdata` → snapshot → `compile_project` → `Morpher`, and independently
   `hc_grammar::load(samples/data/{sena,amharic}-hc.xml)` → `Morpher`. Run
   `samples/data/{sena,amharic}-words.txt` through both; **parse-result signatures (morpheme
   gloss sequences per analysis) must match**. Note: ids can't match (legacy export uses
   session-scoped `Hvo` ints; we use GUIDs) so comparison is behavioral, not structural.
   A freshly regenerated oracle (`GenerateHCConfig.exe`, FieldWorks `Output/Debug`) for
   Sena 3 differs from the committed sample only by Hvo drift — committed samples are valid
   oracles. Amharic's C# export crashes (§1), so the Amharic gate is: import succeeds with
   a warning about the stale ad-hoc rule, and parses match the committed sample.
3. **Determinism**: importing the same `.fwdata` twice produces byte-identical JSON
   (stable ordering — sort by GUID where LCM order is not meaningful; preserve LCM sequence
   order where it is, e.g. slots, rule order, allomorph order).

## 6. Implementation tasks (sonnet subagents, in dependency order)

- **T1 `pg-snapshot`**: crate + format spec doc + serde model + JSON IO + validation +
  round-trip tests. No dependencies on other tasks.
- **T2 `pg-fwdata`**: quick-xml `.fwdata` reader → object graph → extractor → `Snapshot` +
  import report (warnings). Synthetic fixture + self-skipping real-project tests
  (entry/phoneme/template counts against known values). Depends on T1.
- **T3 `hc_grammar::compile`**: snapshot → `Grammar` compiler (HCLoader semantics port,
  §4 phasing). Depends on T1 (parallel with T2).
- **T4 integration + conformance**: CLI `import` subcommand + `.json` dispatch in
  `parse`/`batch`; the §5.2 conformance test; README/docs updates. Depends on T2+T3.

## 7. Follow-ups (not this branch)

- hc-wasm: accept snapshot JSON (constructor overload), surface senses/gloss data to the
  demo (replaces `*-lexical.json`), and an add-to-dictionary path that appends to the
  snapshot and recompiles (replacing `augment_xml` byte-surgery on the XML path).
- Realize-map enrichment from snapshot glosses (feeds `hc-realize::infer`).
- XAMPLE-style backend over the same snapshot (reference: FieldWorks
  `M3ModelExportServices.cs` + `Src/Transforms/Application/FxtM3ParserToXAmple*.xsl`).
- Consider `.fwbackup` (zip) input support.
