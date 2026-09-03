# FieldWorks Run Tests and a PanGloss backend work profiler

Date: 2026-08-11

## Conclusion

FieldWorks' current **Run Tests** feature is a corpus regression report, not a leaf-rule profiler. It parses the distinct words in the chosen text scope, measures wall-clock time for each whole word, stores result counts and parser opinions, and lets the user sort by parse time and inspect the word's live analyses. That workflow can lead a linguist to a troublesome affix, but the persisted report contains no per-affix or per-rule cost attribution.

An archived `SIL.Machine` parse-optimization branch contains the closer precedent: `batch --rule-stats=FILE`. It accumulates a named analysis/synthesis rule tree with inputs, successful calls, outputs, context buckets, and example words. However, only stratum nodes record elapsed ticks, its shared counters require sequential execution, and its flat text output has no stable source identity. A recorded Amharic run consequently attributed essentially all time to the coarse `Analysis > Morphology` node while child nodes displayed zero milliseconds.

PanGloss should combine the useful parts of both precedents: first-class per-word timings and outcome signatures, plus structured per-rule work/fan-out counters. It should add direct high-resolution timings around leaf work, retain normal memoization and merging, use per-word collectors that merge deterministically across threads, and emit a versioned backend JSON report with stable authored grammar identities.

## What FieldWorks Run Tests actually does

The Words-area configuration labels the menu **Run Tests** and offers current text, genre, or all texts (`../FieldWorks/DistFiles/Language Explorer/Configuration/Words/areaConfiguration.xml:20-24`, `289-295`). `ParserListener` obtains the distinct wordforms in that scope and schedules parser work with `checkParser: true`; once all results arrive it creates a report (`../FieldWorks/Src/LexText/ParserUI/ParserListener.cs:569-659`, `678-746`).

`ParserWorker` wraps `ParseWord` in a `Stopwatch` and assigns `ElapsedMilliseconds` to that word's result (`../FieldWorks/Src/LexText/ParserCore/ParserWorker.cs:147-157`). `ParserReport` persists corpus-level totals and a map of `ParseReport` records keyed by word. Each word record contains parse time, error, analysis-count and parser-opinion information, but no rule, affix, template, or trace data (`../FieldWorks/Src/LexText/ParserCore/ParserReport.cs:13-155`, `330-392`).

The report dialog exposes a sortable **Parse Time** column whose help text says it is the time to parse the word (`../FieldWorks/Src/LexText/ParserUI/ParserReportDialog.xaml:183-195`; `../FieldWorks/Src/LexText/ParserUI/ParserUIStrings.resx:253-257`). **Show Analyses** links to the live `IWfiWordform` analyses; it does not deserialize causal data from the report (`../FieldWorks/Src/LexText/ParserUI/ParserReportDialog.xaml.cs:72-99`).

Therefore Run Tests identifies expensive *inputs*. Inspecting their successful analyses may reveal frequently involved affixes, but failed branches are not represented and correlation is not cost attribution.

## The archived HC rule-stat mechanism

The local `machine` repository's archived `parse-optimization-archive` branch (tip `a9ef92379cd2558ba67b6590b883ca744935c1a7`) adds `batch --rule-stats=FILE`. `BatchCommand` rejects cross-word parallel mode, warns about within-word parallelism, enables corpus accumulation, checkpoints the report every 100 words, and emits both the analysis and synthesis trees (`src/SIL.Machine.Morphology.HermitCrab.Tool/BatchCommand.cs:43-49`, `73-90`, `112-147`, `219-224` at that revision).

Every `InstrumentedRule` stores a name, subrules, input count, success count, output count, elapsed ticks, and named context buckets with up to ten examples. A call is a success exactly when it returns at least one output (`src/SIL.Machine/Rules/InstrumentedRule.cs:40-105`). Morphological and phonological rule implementations add allomorph/subrule, category, stem-name, and bare-root buckets on successful outputs (for example `src/SIL.Machine.Morphology.HermitCrab/MorphologicalRules/AnalysisAffixProcessRule.cs:70-93`).

The formatter flattens the tree into grep-friendly paths and prints `inputs`, `successes`, `outputs`, and `elapsedMs` plus buckets (`src/SIL.Machine.Morphology.HermitCrab.Tool/RuleStatsReport.cs:8-55`). The timing is much coarser than the schema suggests: only `AnalysisStratumRule` and `SynthesisStratumRule` update `ElapsedTime`; leaf affix and phonological rules only update counts. The current PanGloss performance notes record the practical result: 63,421 of 64,037 ms landed on `Analysis > Morphology`, with child elapsed values rounded to zero (`rust/docs/o2-profile-findings.md:49-51`, `96-103`).

The branch is not current Machine behavior. Current `BatchCommand` explicitly notes that parse-optimization diagnostics were removed, with the retained implementation available only in archive documentation/history (`machine/src/SIL.Machine.Morphology.HermitCrab.Tool/BatchCommand.cs:14-15`; `machine/docs/archive/conformance-framework-implementation-notes.md:121-123`).

## Existing PanGloss signals and why they are insufficient

PanGloss already returns per-word `steps`, cap/timeout state, and candidate counts in `ParseOutcome`; batch parsing adds whole-word elapsed time and preserves input ordering across a longest-word-first Rayon schedule (`rust/crates/pg-parse/src/morpher.rs:94-121`; `rust/crates/pg-parse/src/batch.rs:55-117`). `HC_STEP_STATS` reports one aggregate attempt count per word. `HC_FST_PROFILE` and the dedup profiler expose process/thread-wide phase counters, not authored rule identities (`rust/crates/pg-cli/src/main.rs:1145-1184`; `rust/crates/pg-rules/src/morph.rs:2205-2251`).

`StepBudget` is also deliberately not a complete work measure: it ticks admitted morphological unapplication attempts, while synthesis counting is normally disabled and inner phonological/FST work can vary dramatically per tick (`rust/crates/pg-rules/src/stratum.rs:100-205`, `567-625`). Existing corpus work found both many-cheap-step and few-expensive-step pathologies, so attempt counts alone cannot rank cost (`reports/03-parse-latency-profile.md`).

The trace system is the wrong foundation for performance attribution. Tracing disables equivalent-analysis merging and the per-parse memo scope, changing the explored search space (`rust/crates/pg-parse/src/morpher.rs:344-354`). The tracing design explicitly separates diagnostic explanation from profiling and notes that existing counters lack rule identity (`rust/docs/p12-tracemanager-design.md:567-597`). The earlier tracing audit remains the source for explanation-tree requirements; a profiler is a separate backend concern (`docs/research/hc-tracing-fieldworks-audit.md`).

## Options

### 1. Port the archived HC tree literally

This is the fastest route to familiar counts and example buckets. It would also reproduce the archived design's weaknesses: shared mutable state, sequential-only runs, ambiguous inclusive/coarse timing, unstable names, and a text report that is difficult to compare automatically.

### 2. Add a typed work observer and corpus profiler (recommended)

Add a profiling-only observer alongside, not inside, `TraceSink`. The public backend operation should be corpus-shaped and deep: it accepts a `Morpher`, words, parse/profile options, and returns a complete `ProfileReport`. Internally each parse gets its own collector; corpus code merges those collectors by stable key after parallel work completes. No global counters or locks are required on the hot path.

Instrument the normal parse at these authored boundaries:

- analysis and synthesis strata;
- affix templates;
- morphological rules, including compounding and realizational rules;
- phonological rewrite/metathesis rules and their subrules;
- selected expensive engine phases such as FST traversal and dedup, attributed to the active authored rule where possible.

For each source record deterministic calls/inputs, successes, outputs, zero-output calls, fan-out, and capped/timed-out participation. Also record nanosecond-resolution total and maximum duration as advisory data. Keep direct work and amplification separate: a cheap rule that emits many candidates can be causally important even when its own body is fast. Retain a bounded set of worst/example words per source.

Use stable authored XML keys plus source kind and analysis/synthesis direction as report identity. Dense runtime IDs (`StratumId`, `MRuleId`, `PRuleId`, `TemplateId`) are appropriate collector indexes but not interchange keys. Most rules already retain authored identity through their definitions or morpheme registry; templates need their authored key retained during grammar loading rather than relying on an ordinal (`rust/crates/pg-grammar/src/model.rs`).

The report should be versioned JSON and include engine version, grammar fingerprint, corpus digest, parse limits/options, thread count, completion state, and per-word outcome signatures. A human table/TSV renderer belongs in the CLI or application layer, not the profiling core.

### 3. Rule-ablation reruns

PanGloss's existing selector path can rerun a corpus with a chosen stratum/template/morphological rule disabled. Comparing baseline and ablated runs estimates marginal effect, but it changes semantics, misses phonological-rule filtering today, interacts nonlinearly with other rules, and costs roughly one corpus run per suspect. It is valuable as a second-stage validation tool for the top profiler findings, not as the primary measurement system.

## Proposed backend seam

A concrete first design should expose one profiling operation from `pg-parse`, for example:

```text
profile_corpus(morpher, words, parse_options, profile_options) -> ProfileReport
```

`pg-rules` should own the low-level source keys/events because that crate knows exactly when rule work begins and what it emits. `pg-parse` should own the per-word lifecycle, grammar-identity resolution, outcome signatures, and deterministic cross-word merge. `pg-cli` should only parse flags and serialize/render the report. This preserves the existing rule that `pg-parse` is the shared backend used by CLI and FFI instead of creating a CLI-only profiler.

The observer must not set `TraceSink::is_tracing`, disable memoization, disable merging, change candidate ordering, or alter budgets. An invariant test should profile and parse the same corpus and assert identical semantic outcome signatures, cap/timeout flags, and step counts. Collector tests should also establish deterministic counts across thread counts; timing fields are explicitly non-deterministic and should be excluded from golden equality.

## Suggested delivery order

1. Retain stable authored keys for every profiled source, especially templates.
2. Add the internal observer and per-word collector for strata, templates, morphological rules, and phonological rules, with deterministic counts and advisory nanosecond timings.
3. Add `pg-parse` corpus aggregation and a versioned JSON schema, reusing normal batch parallelism.
4. Add backend/CLI tests for semantic transparency, deterministic aggregation, caps/timeouts, zero-output rules, and candidate explosion.
5. Correlate existing FST/dedup work counters with the active rule, then validate top suspects using selective ablation.

The most useful initial product is corpus-wide aggregation with drill-down to worst words. That matches Run Tests' real strength while adding the missing causal layer.
