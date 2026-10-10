# 089 — Grammar health finishes when a stored-analysis comparison cannot

Kind: diagnostic (no parse divergence)
Status: implemented in Rust; no C# counterpart

## Sites

- C# site: none; grammar health and its stored-analysis comparison are Rust CLI facilities.
- Rust site: `pg-cli/src/stored_analysis_health.rs::check`, `check_with_options`,
  `compare_words_in_order` and `compare_word`; `pg-cli/src/grammar_health.rs::run_grammar_health`.

## Difference

No parse path changes. `pg-parse` and `pg-rules` are untouched; each wordform is parsed with the
same Morpher, step cap and work cap as before.

Before this change, the comparison of [086](086-stored-analyses-lost-to-authored-phonology.md)
returned an error as soon as one wordform reached the analysis cap, and `grammar-health` exited 1
with no report at all. The checks that do not parse had already run but were never written.

Now:

- A wordform whose parse reaches the cap, or whose confirmed analysis cannot be projected, yields
  one `grammar.stored-analysis.incomplete` warning per stored analysis, naming the wordform and the
  limit. Every other wordform is still compared. A comparison that cannot start at all, for
  example a worker thread that cannot be spawned, is one project-level `incomplete` finding.
- By default (`--stored-analyses bounded`) the comparison stops once the words before the next one
  have together spent `DEFAULT_WORK_BUDGET` (32,000,000) parse work units. The words not reached
  are named in one `grammar.stored-analysis.budget-reached` info finding: the first wordform not
  compared, and how many wordforms and stored analyses remain. `--stored-analyses all` removes the
  budget; `--stored-analyses off` skips the comparison.
- The budget is folded strictly in wordform order over each word's deterministic
  `ParseOutcome::work_steps`, whatever order the workers finish in. Word `i` is compared if and
  only if words `0..i` spent less than the budget, so the report is identical at every thread
  count. Workers stop claiming words at the cutoff; a word past it that a worker already parsed is
  discarded.
- Synthesis of a missing stored analysis is not counted against the budget. It stays bounded per
  analysis by its own work cap, as in 086.

## Why the comparison stays in the default run, bounded in total

A per-word bound alone does not keep the default run fast. With the abort removed and no total
budget, mbugwe compares 1,473 wordforms in 43 s wall time. Its 228 capped wordforms cost about
1 CPU-second each, and its 1,245 uncapped wordforms still cost 92 CPU-seconds, with single words
needing up to 7.5 million work units. A lower per-word cap therefore cannot bound the total.

Moving the comparison behind an opt-in flag would remove amharic's 201 and sena's 49
`no-longer-parses` findings from the default report. The total budget keeps those reports
byte-identical: amharic, the largest of the three, spends 20.7 million work units. It also bounds
aweti and mbugwe.

## Evidence

- `grammar_health_reports_a_capped_wordform_and_compares_the_rest` runs the command end to end on
  the staged `08-rule-context` project with a two-step cap. At that cap `xmupa` reaches the cap
  and `muma` and `xuma` do not. The report is written. `xmupa` has one `incomplete` warning naming
  the cap. The other words' findings equal those of a run without `xmupa`. On the parent commit
  `b51ba7a8`, with only the step cap made injectable, the test fails with
  `stored-analysis comparison for "xmupa" is incomplete: PanGloss hit its analysis work cap`.
- `stored_analysis_comparison_stops_at_its_work_budget_identically_at_every_thread_count` runs
  seven budgets at 1, 2 and 8 workers. It requires identical results, a cutoff that is the named
  wordform, compared findings equal to the full run's for the words before the cutoff, and at
  least one cutoff strictly inside the word list. Ignoring the budget makes it fail.
- `grammar_health_writes_the_report_when_the_work_budget_is_spent` drives a one-unit budget
  through the report validator and requires a written report with one project-scoped
  `budget-reached` finding. With an empty subject list the validator refuses the report, and the
  test fails with `no structured subject`.
- `stored_analysis_comparison_reports_every_capped_wordform_at_every_thread_count` (renamed from
  088's first-error test) requires one `incomplete` finding per wordform, in order, at 1, 2 and 8
  workers under a one-step cap.
- Local real-language samples, measured 2026-10-10 as the minimum of 3 interleaved runs on a
  20-CPU host under load average ~16-22. Times are wall seconds; parent `b51ba7a8` against this
  change:

  | sample | parent | `bounded` (default) | `off` | `all` |
  |---|---|---|---|---|
  | amharic | 1.0, 215 findings | 1.0, byte-identical | 0.3 | 0.9, byte-identical |
  | aweti | 0.8, no report | 1.6, 111 findings | 0.3 | 3.0, 223 findings |
  | indonesian | 0.2, 9 findings | 0.2, byte-identical | 0.1 | 0.2, byte-identical |
  | mbugwe | 1.3, no report | 2.9, 204 findings | 0.7 | 33.1, 1,745 findings |
  | sena | 1.0, 117 findings | 1.0, byte-identical | 1.0 | 1.1, byte-identical |

  "Byte-identical" covers the report file, stderr and the exit status. In every mode, every
  finding outside `grammar.stored-analysis.*` equals the `off` report. Three runs of each mode
  produced one distinct report per sample. The `off` times are FieldWorks import and grammar
  compilation; the checks that do not parse take milliseconds.
