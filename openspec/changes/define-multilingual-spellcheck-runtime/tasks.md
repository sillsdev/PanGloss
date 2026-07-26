This is a design-only change. No task here writes implementation code, runs a benchmark, or performs
a spike. Tasks that require code belong to a later implementation change that consumes this design.

## 1. Close the open prerequisite questions

- [ ] 1.1 Determine whether per-writing-system script/character-set data (word-forming set,
      multigraphs, custom PUA, combining-class overrides) is extracted anywhere in the current
      `pg-fwdata`/`pg-snapshot`/`pg-grammar` pipeline beyond the writing-system tag strings found at
      `rust/crates/pg-fwdata/src/extract/project.rs:33-37`. If it is not, scope the extraction as an
      explicit prerequisite change rather than assuming D-LangID-1 step 2 has data to run on.
- [ ] 1.2 Determine whether `CONTEXT.md`'s absolute resource ceiling (`CONTEXT.md:254-256`) is scoped
      per loaded pack or aggregated across all packs resident in one process, and record the answer
      against D-Data-4's "roughly N×" cost model.
- [ ] 1.3 Decide, with the user, whether a persistently multi-language-tagged word (the tied-score,
      no-signal case) is an acceptable permanent terminal state or must eventually be forced to one
      language by some deterministic rule.

## 2. Validate the cross-language scoring bet before it becomes load-bearing

- [ ] 2.1 Design (not build) a calibration measurement plan for D-NGram-3's cross-language score
      normalization: what a "calibration set" means per language pair, what would count as the
      normalization working vs. failing, and what the fallback is if it fails.
- [ ] 2.2 Confirm with a later implementation change that D2 (the unified weighted composition) has
      landed its own design before D-NGram-3's normalization is wired into a live tie-break, per the
      sequencing risk noted in this change's design.

## 3. Specify the additive `.pgpack` and session-layer schemas (design artifacts, not code)

- [ ] 3.1 Write the concrete schema shape for the new additive `.pgpack` sections (inter-word
      class-n-gram table, intra-word morpheme n-gram table, phonological substitution-cost table,
      per-writing-system orthographic-unit data) as a follow-on design artifact, once task 1.1 is
      resolved.
- [ ] 3.2 Write the concrete shape of the session-level active-language-set and seen-word-cache API
      (request/response shapes, not code) as a follow-on design artifact.
- [ ] 3.3 Write the concrete shape of the "no loaded language could account for this word" typed
      outcome (Open Question 4), consistent with the existing atomic-word-analysis-result and
      batch-analysis-outcome contracts in `CONTEXT.md`.

## 4. Handoff

- [ ] 4.1 Once tasks 1-3 land, open a separate implementation-track OpenSpec change (or changes) that
      consumes this design's decisions and, separately, states its own dependency on the
      `openspec/changes/STAGING.md` FST-coverage track's completion rather than inserting itself into
      that file.
- [ ] 4.2 Run strict OpenSpec validation on this change when the CLI is available.
