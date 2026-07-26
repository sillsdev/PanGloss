## Context

### The pipeline this change extends

`pg_fwdata::import_file(path: &Path)` (`rust/crates/pg-fwdata/src/lib.rs:61-69`) is the sole entry
point of the FieldWorks project import pipeline: it streams a single `.fwdata` XML file into an
object graph (`src/xml.rs`/`src/node.rs`), then extracts a `pg_snapshot::Snapshot` one section at a
time (`src/extract/mod.rs`: `project`, `features`, `phonology`, `morphology`, `lexicon`). The
`project` section (`src/extract/project.rs:21-38`) is the only place writing-system information is
touched today, and it extracts exactly two `Vec<String>` tag lists —
`Project::vernacular_writing_systems`/`analysis_writing_systems`
(`rust/crates/pg-snapshot/src/project.rs:9-23`) — from `LangProject.CurVernWss`/`CurAnalysisWss`,
which are space-separated ICU writing-system tags, default first (`ws_list`,
`extract/project.rs:42-45`).

**Re-verified, one level deeper than the `PLAN.md` caveat's own evidence.** The caveat states
`pg-fwdata` doesn't extract writing-system definitions; this change confirms *why* more precisely.
A direct `grep` of the real `Sena 3.fwdata` file
(`C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects\Sena 3\Sena 3.fwdata`, ~54MB) for
any `<rt class="LgWritingSystem"...>`-style record, or any `class="...WritingSystem"` pattern at
all, returns **zero matches**. Writing-system definitions are not merely unextracted by `pg-fwdata`
— they are **not present in `.fwdata` at all** in this FieldWorks version. This is a stronger, more
specific finding than the caveat's own "no richer per-writing-system object found in `pg-snapshot`
or `pg-grammar` by grepping for writing-system-related identifiers"
(`define-multilingual-spellcheck-runtime/design.md` Open Question 1) — it locates *why* that grep
came back empty: there is nothing to find inside `.fwdata`.

**Where the data actually lives, verified against the real sample project.** FieldWorks project
folders (not `.fwdata` files) contain a `WritingSystemStore/` subfolder with one `.ldml` file per
writing-system tag, sibling to the `.fwdata` file itself:

```
Sena 3/
├── Sena 3.fwdata
└── WritingSystemStore/
    ├── en.ldml
    ├── grc.ldml
    ├── hbo.ldml
    ├── pt.ldml
    ├── seh.ldml                  ← the project's default vernacular WS
    └── seh-fonipa-x-etic.ldml
```

(Directly listed and two files read in full: `seh.ldml`, `hbo.ldml`.) **Correction to the research
docs' working assumption**: `00-synthesis.md` followup 18 and `sil-primary-sources.md` refer to
"the project's `WritingSystems/` folder" — the real folder name in this FieldWorks version is
**`WritingSystemStore`**. This does not change the substance of the finding, only the exact path a
future implementation must use; recorded here so the implementing change does not repeat a
repo-wide search for a folder name that does not exist.

### What a real project's `.ldml` actually contains (verified, not assumed)

`seh.ldml` (Sena's default vernacular WS, plain Latin orthography):

```xml
<ldml>
  <identity>
    <language type="seh" />
    <special xmlns:sil="urn://www.sil.org/ldml/0.1"><sil:identity windowsLCID="1033" /></special>
  </identity>
  <characters>
    <exemplarCharacters>['\-A-PR-Za-pr-z]</exemplarCharacters>
    <exemplarCharacters type="punctuation">[\ -"(-*,./\:;?\[-\]\{-\}\u00AB\u00BB\u2018\u2019\u201C\u201D\u2028]</exemplarCharacters>
  </characters>
  <collations>
    <defaultCollation>standard</defaultCollation>
    <collation type="standard" />
  </collations>
  <special xmlns:sil="urn://www.sil.org/ldml/0.1">
    <sil:external-resources><sil:font name="Doulos SIL" types="default" />...</sil:external-resources>
  </special>
  <layout><orientation><characterOrder>left-to-right</characterOrder></orientation></layout>
</ldml>
```

`hbo.ldml` (the project's Biblical Hebrew analysis WS) has the same skeleton but a *populated*
`<collation type="standard">...</collation>` block (non-self-closing, with real tailoring content)
and an `exemplarCharacters type="punctuation"` set that includes combining marks
(`\u05C6\u0307`) — i.e. this specific real project file does carry non-trivial collation tailoring,
not just the trivial `seh.ldml` case.

**Measured element by element, though, that framing understates the file and points at the wrong
element** (`[M]`, counted directly over the real 142 KB `hbo.ldml`, 2026-07-24):

| Element | Content in `hbo.ldml` |
|---|---|
| `<exemplarCharacters>` (main) | 142,133 chars, **6,523 UnicodeSet brace-strings** |
| `<exemplarCharacters type="punctuation">` | 41 chars, **1 brace-string** |
| `<collations>` | populated, but **153 chars total** — a single `<cr>` CDATA rule, `& u0589 < u0589u05c2 < u0589u05c1`, i.e. **3 units** |
| `<special xmlns:sil>` | `sil:identity` (Windows LCID) + `sil:external-resources` (font names) only |

**In UnicodeSet syntax `{…}` denotes a multi-character *string*, not a character range.** Those
6,523 brace entries in the *main exemplar set* are therefore precisely the multigraph/cluster
inventory — consonant-plus-cantillation-mark sequences declared as single orthographic units. The
collation block, by contrast, carries three. So for this writing system the orthographic edit-unit
inventory is **three orders of magnitude larger in `exemplarCharacters` than in `<collations>`**, and
a design that sourced edit units from collation tailoring alone would extract 3 units instead of
6,523. `seh.ldml`'s `['\-A-PR-Za-pr-z]` has no brace-strings at all, which is correct for a plain
Latin orthography.

Four things follow directly from reading and measuring these two real files, not from the PDF alone:

1. **`exemplarCharacters` (no `type` attribute) vs. `exemplarCharacters type="punctuation"` is the
   concrete mechanism `sil-primary-sources.md` finding 4-6's "word-forming vs. non-word-forming"
   split maps onto** — a character a linguist wants treated as word-forming (e.g. a promoted tone
   mark) belongs in the main exemplar set; a non-word-forming marker belongs in the punctuation set.
   This is `[M]`, read directly from real project files, not inferred from the PDF's prose alone.
2. **The main exemplar set is the *primary* source of orthographic edit units, not the collation
   block** — via UnicodeSet brace-strings, as measured above. The `<exemplarCharacters>` element
   therefore does **two** jobs, and the design must read it for both: word-forming *character*
   classification (finding 1) and multi-character *unit* declaration. Collation tailoring is a
   secondary, complementary source — it can declare ordering relations over units the exemplar set
   does not list (`hbo`'s three `u0589…` sequences are exactly that) — so both are extracted and
   neither is sufficient alone.
3. **Both sources are genuinely present-or-absent per writing system, not fixed schema slots that
   are always populated** — `seh.ldml` has an empty `<collation type="standard" />` *and* zero
   brace-strings; `hbo.ldml` has both populated, at wildly different scales. A design that assumes
   every writing system carries rich tailoring data would be wrong; "not specified" must be a real,
   distinguishable state, and it must be per-source rather than one flag for "edit units" (see
   D-Snapshot-1).
4. **The SIL `<special xmlns:sil="urn://www.sil.org/ldml/0.1">` block is real and present in both
   files**, but in this sample project it only carries `sil:identity` (a Windows LCID) and
   `sil:external-resources` (font names) — **not** a combining-class-override or custom-PUA-character
   element. `sil-primary-sources.md`'s Hebrew custom-combining-class example
   (`ICU_and_writing_systems.pdf`) is real and `[M]` from that PDF, but it was not independently
   re-confirmed against *this* project's own `hbo.ldml` — flagged as an evidence gap in Risks, not
   assumed to generalize or to be absent.

### The sibling change's open question this answers

`openspec/changes/define-multilingual-spellcheck-runtime/design.md` D-LangID-1 step 2 (the
script/character-set feasibility gate) states: "`sil-primary-sources.md` findings 1-2 establish that
FieldWorks writing systems already carry per-writing-system combining-class overrides, multigraph
collation tailoring, and custom PUA characters — the natural source for this gate." Its **Open
Question 1** then flags that only the plain WS-tag-string extraction was found, and asks "whether
per-writing-system script/character-set data is actually extracted into PanGloss's own data model
today," calling it "the user's call." This change is that call: no, it is not extracted today
(re-verified above), and this document designs how it should be.

### Relationship to `pg-grammar::CharDefTable` (a different, existing system — not touched)

`pg-grammar::CharDefTable` (`rust/crates/pg-grammar/src/chardef.rs`) is the grammar's own
HermitCrab **phonological** segment/boundary inventory, built from `.fwdata`'s phonology records
(`PhSegmentDefinition`/`PhBoundaryDefinition`), already carrying both `representations` and
NFD-normalized `representations_nfd` per segment (`chardef.rs:60-63,97-107`), which
`01-lexical-distance.md` identifies as the existing PanGloss data structure closest to an
"orthographic unit." **This is a distinct system from what this change adds.** `CharDefTable` is
about the grammar's phonology (what counts as one phonological segment for rule application);
the LDML data this change extracts is about the writing system's ICU/orthographic conventions
(exemplar/word-forming characters, collation tailoring, PUA) — related in spirit, not the same table,
and not necessarily reconcilable one-to-one (a writing system can define collation tailoring for
characters the phonology never mentions, e.g. punctuation or borrowed-word symbols). See D-Relation-1.

## Goals / Non-Goals

**Goals:** decide where per-writing-system orthographic data comes from, what is extracted, how it
is represented in `pg-snapshot`, how the `pg-fwdata` pipeline changes shape to read it, and settle
the SLDR question the user raised with a reasoned, falsifiable position.

**Non-goals:**
- **D2 (the error model) and the orthographic edit-unit *algorithm*.** `01-lexical-distance.md`
  motivates why this data is needed; this change supplies the data, not the edit-distance design
  that consumes it.
- **D6 (the tokenizer algorithm).** `PLAN.md` keeps D6 "required, undesigned." This change supplies
  D6's input (the word-forming character set); it does not design the word-breaker.
- **Language identification policy.** Owned by `define-multilingual-spellcheck-runtime`
  (D-LangID-1). This change supplies the data D-LangID-1 step 2 consumes; it does not redesign the
  cascade.
- **Semantic data of any kind.** Ruled out by `PLAN.md` § D1; nothing here reopens that boundary —
  LDML orthographic data is parse-adjacent infrastructure (word-forming sets, edit units), not
  authored lexical semantics.
- **Reconciling this data with `pg-grammar::CharDefTable`.** See D-Relation-1 — a real open question,
  explicitly deferred to whatever change designs the edit unit itself.
- **Any code, benchmark, or spike.** Every quantitative or structural claim below not sourced to a
  file read in this repo, a real `.ldml` file, or a cited primary source is marked as a design bet.

## Decisions

### D-SLDR-1 — SLDR is not a data source for this pipeline: not a fallback, not a seed, not a validation reference

The user asked directly: "Wouldn't we be using `github.com/silnrsi/sldr`?" This decision answers
with a position and its reasons, not a default assumption.

**What SLDR is, verified:**
- The **SIL Locale Data Repository** (`github.com/silnrsi/sldr`), MIT-licensed (`[M]`, read directly
  from `github.com/silnrsi/sldr/blob/master/LICENSE`: "The MIT License (MIT), Copyright (c)
  2014-2023 SIL International") — fully compatible with PanGloss's own MIT license
  (`rust/Cargo.toml:29`), so licensing is **not** the reason this change declines to use it.
- Its own README states its purpose is twofold: to gather data for publication on ScriptSource, and
  "to gather information for submission to the Common Locale Data Repository" (CLDR) — i.e. SLDR is
  positioned as a **feeder into CLDR**, not a downstream consumer of CLDR (`[M]`, read directly).
  The manual separately documents an "import a new version of CLDR data into the SLDR" process
  (`[M]`) — data flows CLDR → SLDR for baseline locale data, and SLDR → CLDR for SIL-contributed
  "seed locales," a two-directional feeder relationship, not a one-way authoritative source PanGloss
  would consume from.
- Content, per its own README (`[M]`): language names, script names, character exemplars (main,
  auxiliary, index, punctuation), writing orientation, plural rules, default scripts/regions — the
  same shape of data a project's own `.ldml` carries, generically rather than project-tailored.
- **Canonical programmatic consumer**: `SIL.WritingSystems.Sldr` in `libpalaso`
  (`github.com/sillsdev/libpalaso/blob/master/SIL.WritingSystems/Sldr.cs`) — the shared .NET library
  FieldWorks itself is built on. Read directly (`[M]`): its core method `GetLdmlFile()` performs an
  HTTP fetch against the SLDR web service and caches the result locally, with an explicit
  network-failure fallback to that local cache (`SldrStatus.FromCache`). There is also Python tooling
  (`silnrsi/sldrtools`, successor to a `palaso.sldr` module) — a real, if narrower, ecosystem, not
  vaporware `[M]`.
- `langtags.json` (`https://ldml.api.sil.org/langtags.json`, `github.com/silnrsi/langtags`) is
  SLDR's companion language-tag-equivalence dataset — orthography-level tag equivalence sets, not
  per-writing-system orthographic content itself; a different, narrower question than what this
  change needs.

**The FLEx-authoritative-vs-SLDR-generic distinction, resolved with direct evidence, not
inference.** Read directly from FieldWorks' own "Using the Writing System Properties dialog box"
documentation (`[M]`): *"The Share writing system data with SLDR check box only appears in the
Vernacular Writing Systems Properties dialog box. It is selected by default."* This is a **push**
control — a project's own (tailored) writing-system data flows *to* SLDR by default, opt-out, for
vernacular writing systems. Separately, general FieldWorks documentation states writing-system
definitions "are now accessed from an SIL online repository when creating a writing system in
FieldWorks" (`[A]`, search-engine synthesis of FieldWorks release documentation, not independently
read in full — the specific PDF this claim traces to could not be fetched cleanly, see Risks) — i.e.
SLDR is consulted, if at all, only at the moment a **brand-new** writing system is created inside
FLEx, before the linguist has tailored anything.

**Decision, with reasons in order of force:**

1. **Coverage.** PanGloss's target languages are disproportionately previously-unwritten or
   minority languages — exactly the population least likely to have a curated SLDR entry (SLDR's
   own README describes an editorial contribution/publication pipeline, not a universal catalog).
   A data source this change treats as load-bearing must be guaranteed present the way `PLAN.md` §
   D1 requires of every other factor in this problem ("data the parser needs is guaranteed present"
   — the same criterion, applied here to "data the tokenizer/edit-unit/gate need"). The project's
   own `.ldml` is guaranteed to exist whenever the project itself exists (it is the file FLEx wrote
   when the linguist configured the writing system); SLDR coverage for that specific language is
   not guaranteed at all.
2. **Authority direction.** The verified push-by-default behavior (above) means that by the time a
   project reaches PanGloss for import, its own `.ldml` already reflects whatever SLDR contributed
   at creation time *plus* everything the linguist tailored afterward — collation tailoring,
   custom characters, and combining-class overrides that exist precisely *because* the generic
   default was wrong for this language (the Hebrew combining-class case in
   `sil-primary-sources.md` is a textbook example: the whole point was overriding the generic
   Unicode/ICU default). Re-consulting SLDR at import time could at best duplicate the project's
   own data and at worst reintroduce a generic default over a deliberate local override — exactly
   the failure `sil-primary-sources.md` finding 1 warns against ("normalization is per-writing-system
   tailored data... NOT a universal safe preprocessing step").
3. **Architecture fit.** `pg-fwdata`'s entire import model is a single local file read with **no
   network I/O anywhere in the crate** (`rust/crates/pg-fwdata/Cargo.toml` has no HTTP client
   dependency; `import_file` takes a `&Path`). SLDR's canonical consumer (`Sldr.cs`) is an
   HTTP-fetch-with-local-cache client — a live network dependency structurally foreign to this
   pipeline's shape and to the Native build deployment model's offline posture
   (`CONTEXT.md` "Deployment domains": PanGloss "imports grammar sources, compiles... diagnoses,"
   with no stated network dependency anywhere in that model).

**Position:** SLDR is **not used** by this change, in any of the three roles the user asked about
(fallback, seed, validation reference). This is a design bet, stated as such: it is plausible that
a future change finds real value in an *offline-vendored* SLDR/`langtags.json` snapshot as a
gap-filler specifically for writing systems whose local `.ldml` is nearly empty (`seh.ldml`'s
skeleton is close to this already) — that possibility is recorded in Open Questions, not decided
here, and explicitly not designed as part of this change's scope.

### D-Source-1 — extract from the project's `.ldml` files, located at `<ProjectDir>/WritingSystemStore/<tag>.ldml`

Per the Context section: `.fwdata` carries zero writing-system definition records (verified by
direct grep of the real file); the definitions live entirely in per-tag `.ldml` files in a
`WritingSystemStore/` folder sibling to the `.fwdata` file. This is the extraction source, full
stop — no other location was found and none is expected (this matches the general FieldWorks
project-folder layout documented across the SIL primary sources already read for the sibling
research, not a PanGloss-specific convention).

### D-Source-2 — the pipeline needs a project-directory-aware entry point, not just a `.fwdata` path; exact shape left open

`pg_fwdata::import_file(path: &Path)` (`rust/crates/pg-fwdata/src/lib.rs:61`) is the only entry
point today and takes exactly one file. Reading `WritingSystemStore/` requires the caller to supply
(or the crate to derive) the *project directory*, not just the `.fwdata` file. Two shapes were
considered, and this design fixes the required *behavior* without picking between them — that
choice is left to the implementing change:

- **(a) Implicit derivation** — compute `path.parent().join("WritingSystemStore")` from the
  `.fwdata` path already supplied. Minimal API change, matches the convention observed in every
  real sample project (`Sena 3/Sena 3.fwdata` + `Sena 3/WritingSystemStore/`) and in the existing
  test helper's own path construction (`rust/crates/pg-fwdata/tests/real_projects.rs:13-23`,
  `project_fwdata`). Fragile if a caller ever passes a `.fwdata` file relocated away from its
  project folder (e.g. a copied single file for a bug report) — WS data would silently go missing
  rather than erroring, unless the "always warn" rule below is followed.
- **(b) Explicit new entry point** — a function taking the project directory (or an explicit
  `WritingSystemStore` path) that returns WS-data extraction results (including per-tag warnings)
  independently of `import_file`, so a caller supplying only a bare `.fwdata` file gets a
  (warned) empty WS-data section rather than the extraction silently not running at all.

**Fixed regardless of which shape is chosen:** this must never hard-fail the overall import. It
follows the same posture `pg-fwdata` already states for itself
(`rust/crates/pg-fwdata/src/lib.rs:16-23`, "Robustness" — dangling references and missing expected
fields become `ImportReport` warnings, never a panic or hard `ImportError`) extended one level up:
"the whole sibling folder is missing" or "this specific tag has no `.ldml` file" are warnings, not
failures, exactly like a missing field inside one record is today.

### D-Snapshot-1 — new `pg-snapshot` section, keyed by writing-system tag, with an honest "not specified" state per field

A new snapshot section (name left to the implementing change, e.g. `writing_systems: HashMap<String,
WritingSystemData>` or a `Vec` parallel to `Project`'s tag lists) carrying, per writing-system tag:

- **Word-forming character classification** — derived from `<exemplarCharacters>` (no `type`
  attribute = word-forming/main) vs. `<exemplarCharacters type="punctuation">` (non-word-forming),
  verified `[M]` directly against `seh.ldml`/`hbo.ldml` as the real mechanism FieldWorks uses for
  this distinction. This is the direct input `sil-primary-sources.md` finding 4 says D6's
  word-breaker needs (Unicode LETTER/COMBINING/MODIFIER-LETTER-class word-forming behavior, with
  per-project overrides like the apostrophe or a promoted tone mark) — this field is that override
  data, not a replacement for the generic Unicode-property baseline a tokenizer would still need as
  its own starting point.
- **Orthographic edit-unit data — from two sources, exemplar-set brace-strings *first*.**
  1. **UnicodeSet multi-character strings (`{…}`) in `<exemplarCharacters>`**, main and
     punctuation. This is the primary multigraph/cluster inventory: 6,523 entries in the real
     `hbo.ldml` versus 3 in its collation block (`[M]`, measured — see Context). Extracting the
     main exemplar set for word-forming classification *and* for these unit declarations is one
     read of one element serving two fields, not two passes.
  2. **Collation tailoring** — the content of `<collations><collation type="...">…</collation>
     </collations>` when non-empty (`hbo.ldml`: one `<cr>` CDATA rule; `seh.ldml`: self-closing,
     empty — both real, valid states). Complementary, not redundant: it can declare ordering over
     sequences the exemplar set does not list.

  Together these are the data `01-lexical-distance.md` and `sil-primary-sources.md` finding 2 name
  as the source of multigraph-as-single-unit and PUA-as-unit definitions. **Neither alone is
  sufficient**, and sourcing edit units from collation tailoring alone — the natural reading of the
  research docs, and this design's own first draft — would have extracted 3 units instead of 6,523
  for the one real non-Latin writing system available to check.
- **Custom/PUA characters and combining-class overrides** — carried, when present, in the SIL
  `<special xmlns:sil="urn://www.sil.org/ldml/0.1">` extension block. Verified present as a real
  namespace/block in both sample files read (carrying `sil:identity`, `sil:external-resources` in
  this project); a combining-class-override or custom-PUA-character sub-element was **not** observed
  in either file read for this design — the schema must accommodate them (per
  `sil-primary-sources.md`'s Hebrew example, read from a different primary source, `[M]`) without
  this design asserting they were re-confirmed present in a specific real project file here (Risks).

**"Not specified" is a first-class, distinguishable value, not a silently invented default —** for
a writing-system tag with no `.ldml` file at all, and for any individual field an existing `.ldml`
file doesn't populate (`seh.ldml`'s empty `<collation type="standard" />` is a real example of the
latter). Every downstream consumer (the tokenizer, the edit-unit design, the script/character-set
gate) must have its own documented fallback for "not specified" — designing those fallbacks is
explicitly out of scope here (Non-goals), but this change's contract is that "not specified" is
reported, never papered over with a generic substitute inside the extraction step itself.

### D-Parse-1 — a targeted `quick-xml` reader for the LDML elements actually needed; no general LDML/CLDR crate

Verified (`[M]`, `crates.io`/`docs.rs` search): no maintained Rust crate parses general LDML XML
documents. `icu_locale`/`icu`, `unic-locale`, and `cldr` all operate on locale *identifiers*
(BCP-47 canonicalization, likely-subtag maximization) or ship raw CLDR JSON data — none parses an
arbitrary `.ldml` file's `<characters>`/`<collations>`/`<special xmlns:sil>` elements. Building or
adopting a general UTS #35 LDML implementation is not warranted for the handful of elements this
change actually needs.

**But XML parsing is not the whole job.** `<exemplarCharacters>`'s *content* is
**UnicodeSet syntax** (UTS #35 / ICU), not plain text: `['\-A-PR-Za-pr-z]` is a set of ranges with
escapes, and `{…}` entries are multi-character strings. Since the edit-unit inventory is now
sourced primarily from those brace-strings (D-Snapshot-1), the extraction needs a **UnicodeSet
reader** in addition to the XML reader. Scope, honestly: PanGloss needs only the subset actually
observed in real files — literal characters, `\uXXXX` escapes, `a-z` ranges, and `{…}` strings —
not the full UnicodeSet grammar (no set operations, no `\p{…}` property classes, no nesting). The
implementing change should parse that subset explicitly and **reject-with-warning** anything
outside it rather than silently mis-reading a set it does not fully understand, consistent with the
warn-and-continue posture below. Whether real projects in the wild use constructs outside that
subset is an Open Question — two sample files cannot settle it.

**Recommendation: reuse the existing `quick-xml` dependency** (`rust/crates/pg-fwdata/Cargo.toml:9`)
with a small, targeted reader analogous to `pg-fwdata`'s own existing `.fwdata` reader
(`src/xml.rs`/`src/node.rs`) — which is itself already a hand-written streaming reader for a
different SIL XML dialect, not a general-purpose XML object model. This is the "boring tool,
targeted read" option, and it is the right one: it matches the repo's own precedent in the same
crate, avoids a new heavyweight dependency, and the element set needed (`exemplarCharacters`,
`collation`, the `sil:` special block) is small and stable (LDML/UTS #35 is a mature, slow-moving
spec). This is not a case where the repo's "port what's missing" build philosophy
(`00-synthesis.md` "Build philosophy") calls for building a general LDML engine — nothing else in
the repo needs one, and the targeted-read approach is already proven at `pg-fwdata`'s own `.fwdata`
layer.

### D-Relation-1 — this is a new, `CharDefTable`-adjacent data source; it does not merge with `CharDefTable`

Per Context: `pg-grammar::CharDefTable` is the grammar's phonological segment/boundary inventory;
the LDML data this change adds is the writing system's ICU/orthographic-convention data. They are
related (both are candidate contributors to an eventual "orthographic unit" concept for the
edit-distance correctness fix `01-lexical-distance.md` names) but are not the same table, are
sourced from different files (`.fwdata` phonology records vs. `.ldml` sidecar files), and are not
guaranteed to agree in coverage (a writing system's collation tailoring can cover characters — e.g.
punctuation, borrowed-word letters — the grammar's own phonology never mentions). This change does
not attempt to unify them. Whether and how to combine the two into one edit-unit inventory is left
to whatever change designs the edit-unit/error-model itself (Open Questions).

## Dependencies and Ownership

- **Feeds `define-multilingual-spellcheck-runtime`'s D-LangID-1 step 2** (the script/character-set
  feasibility gate) directly — that change's Open Question 1 is this change's entire subject.
  This change owns the extraction/representation question; `define-multilingual-spellcheck-runtime`
  owns how the gate consumes the data, unchanged by this change.
- **Feeds D6** (`PLAN.md` § D6, tokenization) and the orthographic edit-unit design implied by
  `01-lexical-distance.md` — both remain undesigned; this change supplies their required input data,
  not their algorithms.
- **Does not depend on, and is not sequenced against, `openspec/changes/STAGING.md`'s FST-coverage
  track** — verified neither `pg-fwdata` nor `pg-snapshot` appears anywhere in that file's
  merge-hotspot or file-ownership lists. Same reasoning `define-multilingual-spellcheck-runtime`
  already used for its own non-insertion into that file.
- A later implementation change owns: the exact `pg-snapshot` schema (D-Snapshot-1), the
  `pg-fwdata` LDML-reader module and its entry-point signature (D-Source-2), and surveying more real
  `.ldml` files (beyond the two read here) before finalizing which SIL `<special>` sub-elements to
  target.

## Risks

- **Single-project evidence base.** The two `.ldml` files read directly for this design (`seh.ldml`,
  `hbo.ldml`) are both from one sample project (Sena 3). Combining-class overrides and custom-PUA
  character definitions were not observed in either — `sil-primary-sources.md`'s Hebrew
  combining-class example is real and `[M]` from a different primary source
  (`ICU_and_writing_systems.pdf`), not re-confirmed against a real project file in this pass. A
  wider survey (more sample projects, or FieldWorks' own shipped default LDML set) is needed before
  the exact schema is finalized — flagged in Open Questions, not treated as settled.
- **D-Source-2's entry-point shape is undecided.** If the implementing change picks the fragile
  implicit-derivation option without also implementing the "always warn, never silently skip" rule
  this design fixes, a `.fwdata` file relocated away from its project folder would silently lose WS
  data rather than reporting the gap.
- **"Not specified" pushes real design work downstream.** D-Snapshot-1's honesty requirement means
  D6, the edit-unit design, and the sibling change's script gate each need their own documented
  fallback for absent data. If those changes instead assume rich data is always present, this
  change's honesty becomes their blocker rather than their foundation — a sequencing risk to flag,
  not a contradiction in this design.
- **One claim in D-SLDR-1 is search-engine-synthesized, not independently read in full**: that
  "writing system definitions are now accessed from an SIL online repository when creating a writing
  system in FieldWorks" traces to FieldWorks release/help documentation that could not be fetched
  and read directly as primary text in this pass (the specific PDF URLs found for this topic
  resolved to an unrelated SIL Encoding Converters document on inspection). The push-direction
  evidence (the "Share writing system data with SLDR" checkbox, default-on) *was* read directly from
  primary FieldWorks documentation and is `[M]`; the pull-at-creation-time claim is `[A]` and is
  treated as corroborating, not load-bearing, in D-SLDR-1's reasoning — reason 1 (coverage) and
  reason 3 (architecture fit) do not depend on it at all.

## Open Questions

These are gaps this change could not settle from the required reading or repo inspection. They are
recorded rather than papered over with an invented answer.

1. **D-Source-2's exact entry-point shape** (implicit `WritingSystemStore` derivation from the
   `.fwdata` path vs. an explicit new project-directory-aware entry point) is not decided here —
   left to the implementing change, with the required behavior (never hard-fail; always warn on a
   missing folder or missing per-tag file) fixed.
2. **Whether combining-class overrides and custom PUA character definitions actually appear in real
   SIL `<special>` LDML blocks** beyond the `sil:identity`/`sil:external-resources` elements observed
   in the two sample files read here needs a wider survey (more real projects, or FieldWorks' shipped
   default LDML set) before D-Snapshot-1's schema is finalized.
3. **Whether a future, offline-vendored SLDR/`langtags.json` snapshot is worth adding later** as a
   gap-filler specifically for writing systems whose local `.ldml` is nearly empty (D-SLDR-1) is
   explicitly deferred, not decided. If pursued, it would need its own design pass on how "vendored,
   build-time-only, never a runtime network fetch" is enforced.
4. **What the D6 tokenizer's and the sibling change's script/character-set gate's documented
   fallback behavior should be** for a writing system with "not specified" word-forming data
   (D-Snapshot-1) is not designed here — it belongs to those changes, but this change's "absent is a
   first-class reported state" contract requires them to have *some* documented answer, which does
   not exist yet in either `PLAN.md` § D6 or `define-multilingual-spellcheck-runtime`.
5. **Whether this change's orthographic edit-unit inventory and `pg-grammar::CharDefTable`'s
   phonological segment inventory should ever be unified into one "orthographic unit" concept**
   (D-Relation-1) is left to the edit-unit design `01-lexical-distance.md` implies but does not
   itself design.
6. **The exact claim that FieldWorks "accesses writing system definitions from an SIL online
   repository when creating a writing system"** (used only as corroborating, non-load-bearing
   evidence in D-SLDR-1) traces to a source this pass could not fetch and read directly as primary
   text — worth an independent confirmation pass (e.g. reading FieldWorks' own release notes or
   source, not a search-engine synthesis) before treating it as more than corroboration.
7. **Which subset of UnicodeSet syntax real `.ldml` exemplar sets actually use** (D-Parse-1). The
   two files read here use only literals, `\uXXXX` escapes, `a-z` ranges, and `{…}` strings — but
   two files cannot establish that set operations (`&`, `-` between sets), `\p{…}` property classes,
   or nesting never appear in the wild. The design's position is to parse the observed subset and
   warn-and-reject the rest; a wider survey (the same one task 1.1 needs) should confirm the subset
   before the reader is written.
