# FST verification implementation plan

**Goal:** Implement the fst-140 lane brief's committed per-word analysis-set checker and measure all six supplied corpora.

**Architecture:** Reuse `pg_foma::differential::measure_word`, the shared stable identity projection, and the CLI grammar loader. Preserve complete occurrence rows, typed construction refusals and incomparable outcomes. Expose confirmation completeness at its owner rather than infer it from empty analyses.

**Spec:** `/home/johnm/work/fst-production-20261103/lanes/fst/fst-140-verify-brief.md` and its `common.md`.

**Constraints:** No new dependencies, no real-language data in git, no lexemes in the lane report, managed builds with four jobs, one final commit, preland on that exact SHA. Work inline without delegation as required by common.md.

## Tasks

- [ ] Share the existing Morpher word budget across restricted confirmation groups; expose incomplete confirmation diagnostics and teach the differential driver to honor them, including memory stops.
- [ ] Add feature-gated `fst-verify`, its declared surface, streamed TSV and JSON evidence with hashes, versions, options, per-word verdicts and exhaustive totals. Both-direction differences retain both identity lists; `missing` takes precedence when both occur.
- [ ] Test equal and known-missing conformance inputs, identity ordering/deduplication, both-direction differences, caps/timeouts/memory stops, typed refusal, bad options and output collisions. Run managed check and focused tests.
- [ ] Document the versioned output contract, README usage and changelog. Run all locally supplied reference inputs and preserve private artifacts outside git.
- [ ] Review contracts, completeness, bounds, scope and provenance; write the PR pitch and lane report. Commit once after check; rerun touched tests and preland on the committed SHA; require READY in the report before finishing.

## Review focus

Repeated words remain separate rows; partial results never become mismatch evidence; full identities distinguish root position/category and ignore duplicate discovery paths; a refused grammar produces no parity claim; output paths must not overwrite inputs.
