# 088 — Grammar health compared stored analyses serially and import repeated provenance work

Kind: efficiency
Status: optimized-in-rust

## Sites

- C# site: none; grammar health and the FieldWorks importer are Rust CLI facilities.
- Rust site: `pg-cli/src/stored_analysis_health.rs::check_with_options` and
  `compare_words_in_order`; `pg-fwdata/src/extract/mod.rs::extract_with`;
  `pg-fwdata/src/xml.rs::push_duplicate_guid_issues`, `class_allowed` and `RawGraph::census`;
  `pg-snapshot/src/conversion.rs::SelectionRecorder::has_load_decision` and
  `finish_with_load_decisions`.

## Difference

No parse path changes. `pg-parse` and `pg-rules` are untouched; the Morpher is called with the same
arguments and caps as before.

The stored-analysis comparison of [086](086-stored-analyses-lost-to-authored-phonology.md) parsed
each distinct wordform on one thread. It now runs up to `min(available CPUs, 8)` workers, `batch`'s
default cap. Workers claim words in wordform (`BTreeMap`) order from a shared cursor and the
per-word results are folded in that order. The diagnostics, and the first error returned when a
word reaches the analysis cap, are therefore those of the sequential walk. Each cap involved is a
deterministic step count: the Morpher's `word_timeout` stays `None`, so contention cannot change
an outcome. A worker stops claiming words past the lowest failing index. On that error path each
worker can still finish one word beyond the failure, so the error returns later than before by up
to one capped word's parse time.

Import provenance bookkeeping did the same work twice, or quadratically:

- `has_load_decision` scanned every recorded load decision; the finalizer calls it once per
  tracked record. It now range-queries `load_decision_ordinals`, which `record_load_decision`
  populates for every decision and nothing removes.
- `extract` cloned and finished its recorder before recording source decisions, then again after.
  Source decisions add load decisions only, so one finish after them yields the same inventory,
  issues and decisions. The plain `extract` path no longer clones the recorder at all, and
  `source_objects()` is built once.
- Duplicate-guid issues grouped every header guid. The parser marks each repeat occurrence, so only
  marked guids are grouped; the issue text and order are unchanged.
- `finish_with_load_decisions` computed `check_invariants` in release builds and discarded the
  result; it now runs only where the assertion can fire, under debug assertions.
- The census hashes the same header bytes without formatting a string per header, and
  `class_allowed` is a set lookup computed once per header.

## Evidence

- Output identity on all five local real-language samples (amharic, aweti, indonesian, mbugwe,
  sena): `grammar-health` report file, stdout report, stderr log and exit status, and the full
  `pangloss import` snapshot JSON including `conversionProvenance`, are byte-identical to the
  parent commit `c8f9505a`.
- `grammar_health::tests::stored_analysis_comparison_is_identical_at_every_thread_count` compares
  1, 2, 3 and 8 workers on 24 wordforms over the staged `08-rule-context` project.
  `stored_analysis_comparison_reports_the_first_capped_wordform_at_every_thread_count` requires
  the alphabetically first wordform's cap error at 1, 2 and 8 workers. Folding results in
  completion order instead of wordform order made both fail.
- `has_load_decision_matches_a_scan_of_every_recorded_decision` compares the index with a scan
  across kinds, stages and context keys. It fails if the query is an exact lookup of the empty
  context key, or if it omits the stage check.
- `duplicate_guid_issues_name_every_occurrence_in_first_occurrence_order` pins the issue text and
  order for a doubled and a tripled guid. Grouping only the marked repeat occurrences made it and
  two existing duplicate tests fail.
- Wall time measured 2026-10-10, parent `c8f9505a` against this change, minimum of 7 interleaved
  runs on a 20-CPU host under load average ~20: amharic 6.67 s to 1.43 s, sena 2.76 s to 1.77 s,
  indonesian 0.58 s to 0.26 s, aweti 1.48 s to 1.17 s. Mbugwe stops at the analysis cap on its
  second wordform and went from 1.97 s to 2.38 s, the error-path cost described above.
