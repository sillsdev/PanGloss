## Why

`docs/research/spellcheck/PLAN.md` § D1 ("Orthographic units") states that the orthographic edit
unit — multigraphs and PUA already defined as single collation units, plus per-writing-system
combining classes — is "existing LibLCM data, not something to invent," then records a verified
`CAVEAT, verified 2026-07-24`: **that data exists in the FLEx project but PanGloss does not extract
it.** `pg-fwdata` pulls only the space-separated writing-system *tags* from `CurVernWss`/
`CurAnalysisWss` (`rust/crates/pg-fwdata/src/extract/project.rs:33-37`, `extract/mod.rs:38-40`) —
no collation tailoring, no multigraph or PUA definitions, no combining-class overrides. This is
re-verified directly in this change, one level deeper than the caveat's own evidence: a direct
`grep` of the real `Sena 3.fwdata` project file for any `LgWritingSystem`/`WritingSystem`-class
`<rt>` record returns **zero matches** — writing-system *definitions* are not merely unextracted,
they are structurally absent from `.fwdata` itself. They live in per-tag `.ldml` sidecar files in
the project's own folder (verified against the real sample project:
`C:\Users\johnm\Documents\repos\FieldWorks\DistFiles\Projects\Sena 3\WritingSystemStore\{seh,hbo,en,pt,grc}.ldml`,
sibling to `Sena 3.fwdata`). So this is confirmed to be a **new source to import**, not a field
`pg-fwdata` overlooked (`docs/research/spellcheck/00-synthesis.md` followup 18).

Three things currently have no data source because of this gap:
1. The **orthographic edit unit** — `docs/research/spellcheck/01-lexical-distance.md` calls
   byte/Unicode-scalar edit distance over multigraph/combining-mark orthographies a **correctness
   bug**, not a tuning knob, and names the writing system's own collation tailoring as the fix.
2. **D6 tokenization** (`PLAN.md` § D6, "required, undesigned") — the word-forming character set
   that must drive the word-breaker (`docs/research/spellcheck/sil-primary-sources.md` findings
   4-6).
3. The multilingual **script/character-set gate** — `openspec/changes/define-multilingual-spellcheck-runtime/design.md`
   D-LangID-1 step 2 names this gate and that change's **Open Question 1** is exactly this gap,
   phrased as "does the richer per-writing-system data exist anywhere in `pg_snapshot` today," with
   the answer left as "needs the user's call."

This change answers that open question with a design, and does not invent answers for the three
downstream consumers themselves — it supplies their input data, not their algorithms.

**Claim type note** (per `openspec/config.yaml`'s proposal rule): every factual claim in this change
is an **observation** — a direct read of a primary source (SIL/SLDR documentation, a real
`.ldml`/`.fwdata` file, or PanGloss's own code) or a stated design bet flagged as such. This change
makes no support, recall, or certification claims; it does not touch analysis correctness, FST
compilation, or any conformance gate.

## What Changes

- Extract per-writing-system orthographic data from a FieldWorks project's own `.ldml` sidecar
  files (`<ProjectDir>/WritingSystemStore/<tag>.ldml`), as a new, additive `pg-snapshot` section
  keyed by writing-system tag, read via a small targeted XML reader (reusing the `quick-xml`
  dependency `pg-fwdata` already has), not a general LDML/CLDR parsing crate — none exists in the
  Rust ecosystem beyond locale-*identifier* canonicalization crates (verified below).
- Define what is extracted: word-forming vs. non-word-forming character classification (from
  `exemplarCharacters`, generalized), the orthographic edit-unit inventory (multigraph/collation
  tailoring, custom/PUA characters), and per-writing-system combining-class overrides, wherever the
  project's own LDML carries them — with an honest "not specified" state per field when it does not,
  never a silently invented default.
- Take an explicit, reasoned position on **SLDR** (`github.com/silnrsi/sldr`), which the user
  specifically asked about: **not used** by this pipeline, as a fallback, seed, or validation
  reference, for reasons recorded in `design.md` D-SLDR-1 (coverage, authority direction, and
  architecture fit — this is a design bet, explicitly flagged, not a settled industry consensus).
- Define the pipeline-shape decision this forces: `pg_fwdata::import_file` currently takes a single
  `.fwdata` path (`rust/crates/pg-fwdata/src/lib.rs:61`); reading the sibling `WritingSystemStore/`
  folder needs either an implicit sibling-derivation convention or an explicit new entry point —
  left as an open question with the constraint (never hard-fail; always warn-and-continue, matching
  this crate's existing robustness posture) fixed here.
- Record every place this change could not settle a decision as an explicit **Open Questions**
  section, matching `define-multilingual-spellcheck-runtime`'s own habit, rather than inventing an
  answer.

## Capabilities

### New Capabilities

- `writing-system-data`: extraction, representation, and non-goals for per-writing-system
  orthographic data (word-forming character sets, orthographic edit units, combining-class
  overrides) sourced from a FieldWorks project's own LDML files.

## Impact

- Design-only. No `pg-*` crate is modified by this change; it defines the contract a later
  implementation change must satisfy, the same way `define-grammar-coverage-contract` and
  `define-fst-compilation-health` define contracts consumed by later implementation changes in the
  FST-coverage track (`openspec/changes/STAGING.md`).
- Directly answers `define-multilingual-spellcheck-runtime`'s **Open Question 1** (whether
  per-writing-system script/character-set data is extracted anywhere today) and its task 1.1. That
  change's D-LangID-1 step 2 (the script/character-set feasibility gate) depends on this change's
  output as its data source — put here as a stated **dependency**, not duplicated: this change owns
  the extraction/representation question, `define-multilingual-spellcheck-runtime` owns how the gate
  *uses* the data.
- Feeds, but does not design: the orthographic edit unit (`01-lexical-distance.md`'s correctness-bug
  fix) and D6 tokenization (`PLAN.md` § D6) — both remain undesigned; this change supplies their
  input, not their logic. See Non-goals in `design.md`.
- **Not added to `openspec/changes/STAGING.md`** — that file's scope is explicitly "the active
  grammar-coverage changes" (`STAGING.md:3`), the FST-compilation/coverage track. This change is
  upstream of the `pg-fwdata`/`pg-snapshot` import pipeline that track's changes do not touch
  (verified: neither `pg-fwdata` nor `pg-snapshot` appears anywhere in `STAGING.md`'s merge-hotspot
  or file-ownership lists), so it does not participate in that dependency graph, matching the
  precedent already set by `define-multilingual-spellcheck-runtime`.
- Depends on nothing landing first; it is itself a prerequisite other changes depend on.
