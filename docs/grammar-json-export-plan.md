# LCM Grammar JSON export — plan for team sign-off

**Goal.** LibLCM gains a basic, deterministic JSON export of the parser-relevant subset of a
project (phonology, morphology, lexicon) — the format PanGloss already consumes as its snapshot.
Export only; nothing fancy. Import, merge/sync, texts, and custom fields are explicit non-goals
for v1. This is the artifact the team signs off on before deciding where the conformance suite
lives.

## Naming

- **Format name:** LCM Grammar JSON. ("Grammar" used in the broad sense — the spec's first line
  states the scope is *everything a morphological parser needs*, lexicon included.)
- **Envelope:** `{"format": "lcm-grammar", "version": 1, ...}` — renamed from PanGloss's current
  `"pangloss-project"`. Same content, neutral name, owned by LibLCM.
- **File extension:** `.grammar.json`.
- **C# surface:** `SIL.LCModel.DomainServices.GrammarJsonServices.ExportGrammar(LcmCache, TextWriter)`
  — one static class, sitting next to `M3ModelExportServices.cs` (the existing precedent: the M3
  parser-model export that XAmple generation is built on). No new project, no new NuGet package,
  no new dependency (Newtonsoft.Json 13 is already referenced; netstandard2.0-compatible).

## The one real spec change: ordering

Byte-identical output from two independent implementations (LibLCM and pg-fwdata) requires the
spec to define ordering *from the data*, not from implementation internals. The current spec says
"construction order preserved," which pg-fwdata realizes as fwdata file-encounter order — an
order LCM does not preserve (repositories enumerate unordered owning collections in undefined
order). Amend the spec:

- **LCM owning sequences** (senses, allomorph AlternateForms, template slots, rule RHS lists,
  possibility-list children...) keep their model-defined order — LCM and the fwdata file agree
  on these by construction.
- **Unordered collections** (lexical entries, environments, natural classes, ad-hoc rules...)
  are sorted by GUID (ordinal, lowercase-hyphenated form).

pg-fwdata adopts the same rule (a small, mechanical change — swap `lex_entry_order` for a GUID
sort). Everything else in `docs/snapshot-format.md` (omitted empties, pretty-printing, no
timestamps, WsForm arrays in project writing-system order) carries over verbatim.

## Optionality: the classification ladder and decisions D1–D5

The format already omits empty/absent fields, so optionality is a matter of **export filters**,
not schema redesign — one schema, no profiles, no format forks. Field decisions follow one
ladder, which the team ratifies once (after that, individual field questions are mechanical):

1. **Parser needs it** (`compile_project` consumes it) → required core. Objective test: strip it
   and the grammar no longer compiles/parses identically.
2. **Displaying a parse needs it** (glosses, POS names, citation forms) → optional, default-on.
3. **Only a dictionary needs it** → out of scope; LIFT/MiniLcm/Webonary territory. Definitions
   sit exactly on this line: kept in the format, filterable. Examples, semantic domains,
   pronunciations, etymologies, reversals: past the line, excluded.
4. **Size or sensitivity concern** → export-time filter, never a schema change.

Grounding data (Sena 3: 1,462 entries, 1,726 senses): fwdata 55.9 MB → grammar.json 2.45 MB
pretty-printed → 252 KB gzipped. Lexicon = 96% of the document, but senses = 27%, glosses = 9%,
definitions = **2%** — size does not justify a leaner default; filters exist for need and
data-minimization, not bytes.

The decisions:

- **D1 — ratify the core/optional boundary**: parser-input subset = required; everything else
  optional, stated field-by-field in the JSON Schema.
- **D2 — filter flags on both exporters** (`--exclude senses`, `--exclude definitions`) with
  identical semantics in LibLCM and pg-fwdata, so the byte-gate can also compare filtered
  exports. Default export includes everything.
- **D3 — writing-system filtering** (`--ws en`): the bigger payload/sensitivity lever than field
  dropping, for projects with many analysis languages.
- **D4 — mark filtered exports in the envelope**: `"omits": ["definitions"]`, present only when
  a filter was applied. Needed because omission already means "not authored"; without the marker
  a consumer cannot distinguish "no definition exists" from "definitions were stripped." Default
  exports carry no marker, so the byte-gate is untouched.
- **D5 — enrichment policy**: new content (examples, audio, semantic domains) enters only when a
  named deployment consumer needs it to display a parse, additively within the major version;
  otherwise the answer is "use LIFT/MiniLcm alongside."

Sensitivity note for the sign-off doc: the reason to strip definitions from a public website
export is usually data minimization (community preferences about semantic content on the open
web), not size — the filter flags make honoring that a one-flag decision.

## Steps

1. **Spec + schema.** Rename and move the spec: `snapshot-format.md` → `grammar-json.md` with the
   ordering amendment, plus a JSON Schema file and one small canonical example. Destined for the
   liblcm repo (`doc/` or alongside the source); PanGloss holds it until the LibLCM PR opens.
   Known future additive extensions get a one-paragraph "roadmap" section so reviewers see the
   trajectory without expanding v1: writing-system definitions and a built-in-GUID appendix
   (needed only when import arrives), senses/custom-field enrichment (needed only if non-parser
   consumers want them).
2. **pg-fwdata alignment PR (PanGloss).** Emit/accept `"lcm-grammar"` v1; switch unordered
   collections to GUID sort. Flag-day rename — snapshot files aren't long-lived artifacts yet.
   Existing equivalence gates re-baseline in the same PR.
3. **LibLCM PR.** `GrammarJsonServices` + NUnit tests: golden export of an in-memory test
   project, determinism (two exports byte-identical), and empty-project shape. Mark the API
   with whatever "experimental/subject to sign-off" convention the maintainers prefer.
4. **Cross-implementation byte-gate.** Export Sena 3 and Amharic via both implementations;
   assert byte-identical. Lives in PanGloss CI first (it has the Rust side and the .fwdata
   fixtures); LibLCM keeps its own golden-file tests. Divergences found here are spec bugs —
   fix the spec, then whichever implementation drifted.
5. **Sign-off package for the team:** the spec + schema, one real exported `.grammar.json`
   (Sena 3), the byte-gate evidence, the optionality ladder + decisions D1–D5 (section above),
   and a one-page cover note: what it's for (conformance fixtures, PanGloss verification input,
   field-deployment artifact), what it is not (an editing format, a sync format, a replacement
   for fwdata), and the open question they own — where the conformance suite lives.

## Sizing

Steps 1–2 are days, not weeks (mostly renames plus the sort change). Step 3 is the real work but
bounded: the extraction semantics are already specified per-field in the spec with "← LCM origin"
annotations, and LCM answers its own best-analysis/virtual-property questions natively — expect
it to be considerably smaller than pg-fwdata, which had to reimplement those conventions.
Step 4 is a test harness, not new logic.
