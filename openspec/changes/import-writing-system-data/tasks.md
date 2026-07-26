This is a design-only change. No task here writes implementation code, runs a benchmark, or
performs a spike. Tasks that require code belong to a later implementation change that consumes
this design.

## 1. Close the open prerequisite questions

- [ ] 1.1 Survey more real `.ldml` files (additional sample projects beyond Sena 3, or FieldWorks'
      shipped default LDML set) to confirm whether combining-class overrides and custom-PUA
      character definitions actually appear in real SIL `<special>` blocks, before finalizing
      D-Snapshot-1's schema. Only `seh.ldml` and `hbo.ldml` from one project were read for this
      design.
- [ ] 1.1a In the same survey, record which **UnicodeSet syntax constructs** appear in real
      `<exemplarCharacters>` values (D-Parse-1, Open Question 7). The two files read here use only
      literals, `\uXXXX` escapes, `a-z` ranges, and `{…}` multi-character strings; confirm whether
      set operations, `\p{…}` property classes, or nesting occur before the reader's supported
      subset is fixed.
- [ ] 1.2 Decide, with the user or a later implementation change, D-Source-2's exact entry-point
      shape: implicit `WritingSystemStore/` derivation from the existing `.fwdata` path vs. an
      explicit new project-directory-aware entry point in `pg-fwdata`. The required behavior (never
      hard-fail; always warn on a missing folder or per-tag file) is fixed by this change regardless
      of which shape is chosen.
- [ ] 1.3 Independently confirm (by reading FieldWorks' own release notes, help documentation, or
      source rather than a search-engine synthesis) the claim that FieldWorks accesses SLDR when
      creating a new writing system. This is corroborating, non-load-bearing evidence in D-SLDR-1 —
      D-SLDR-1's decision does not depend on it — but is worth confirming directly before citing it
      elsewhere.

## 2. Specify the concrete `pg-snapshot` schema (design artifact, not code)

- [ ] 2.1 Write the concrete field-by-field schema for the new writing-system-data snapshot section
      (word-forming character classification, orthographic edit-unit/collation-tailoring data,
      custom/PUA characters, combining-class overrides), including the exact "not specified"
      representation per field, as a follow-on design artifact once task 1.1 is resolved.
- [ ] 2.2 Write the concrete shape of the `pg-fwdata` LDML-reader module boundary (what it returns,
      how warnings compose with the existing `ImportReport`), consistent with task 1.2's
      entry-point decision.

## 3. Handoff

- [ ] 3.1 Once tasks 1-2 land, open a separate implementation-track OpenSpec change (or changes)
      that consumes this design's decisions: the `pg-fwdata` LDML reader, the `pg-snapshot` schema
      addition, and a fixture/real-project test analogous to `rust/crates/pg-fwdata/tests/real_projects.rs`.
- [ ] 3.2 Confirm with `define-multilingual-spellcheck-runtime` that this change's output satisfies
      its Open Question 1 and D-LangID-1 step 2's stated data need; update that change's Open
      Questions section to mark item 1 as answered once this change's schema (task 2.1) is written.
- [ ] 3.3 Run `openspec validate --strict` on this change.
