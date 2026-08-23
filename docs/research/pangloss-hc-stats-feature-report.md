# `--stats`: per-authored-object attempts, parses, hit rate, and time in HC mode

Date: 2026-08-22

Status: research synthesis. Not a design. The grill queue at the end is the next step.

Sources: `pangloss-stats-metrics-precedent-research.md` (online precedent),
`fieldworks-parser-report-storage-and-scoping.md` (FieldWorks/LibLCM/Machine source),
`pangloss-profiler-health-design-freeze.md` and its eleven decision records (existing product
decisions), plus direct reading of `pg-parse`/`pg-rules`/`pg-cli`.

## Conclusion

Three quarters of this feature is already decided, one quarter is genuinely new, and one column of
the requested report may not be constructible at all as stated.

**Already decided** (2026-08-11 freeze, item 5 of its implementation sequence, unstarted): the seam
is `profile_corpus(morpher, request) -> Result<ProfileReport, ProfileError>` in `pg-parse` with a
concrete `WorkCollector` in `pg-rules`; per-word collectors merged deterministically after parallel
work; normal parse semantics, never `TraceSink`; and `executions` / `successes` / `outputs` /
`memo_hits` / `memo_nogood_hits` / `replayed_outputs` kept as separate fields rather than one
"attempts" number.

**Genuinely new in this ask**: per-**lexical-entry** attribution, JSONL/CSV storage with
**on-demand** aggregation, **per-text** reports, the `--stats` CLI surface, and a word-selection
filter.

**The split that matters.** "Parses" is nearly free — `WordAnalysis` already carries `morpheme_ids`
and `root_morpheme_index` (`rust/crates/pg-parse/src/lib.rs:27-46`), so per-entry and per-affix
*successful-parse* counts are derivable from existing output with no hot-path instrumentation.
"Attempts" is the entire design problem: nothing today attributes work to an authored object, and
`StepBudget` is one scalar per word whose own doc says per-tick cost is not uniform
(`rust/crates/pg-rules/src/stratum.rs:100-205`).

**On the Prometheus framing**: the research is unambiguous that it is the wrong model here — 10^4-10^5
lexical-entry labels are Prometheus's own documented anti-pattern, and a batch CLI has no scrape
target. What transfers is one discipline (never store a ratio; store numerator and denominator and
divide at query time) and one negative result (per-call timers report noise). The right precedent is
wide structured events plus on-demand aggregation, which is what was asked for anyway. Soufflé's
Datalog profiler is the closest column-for-column precedent that exists.

## What each precedent actually contributes

| Precedent | Copy | Reject |
|---|---|---|
| **FLEx Run Tests** | Per-word wall time as a first-class stored field; report-to-report comparison by subtraction; keeping many dated reports per project | GUID filenames with no schema version; a single free-text scope label for a whole run; no export of any kind; all-or-nothing accumulation |
| **Archived Machine `--rule-stats`** | Context buckets with bounded example witnesses (allomorph, category, stem-name, bare-root) — genuinely useful grammar-facing evidence | Global mutable counters forcing sequential-only runs; flat string-concatenated node identity with no disambiguator; stratum "success" defined as `outputCount > 0` when the stratum seeds its own output with the unchanged input, making success ~always true; leaf rules never timing themselves |
| **Soufflé profiler** | The report shape: per-rule total/non-recursive/recursive time plus tuples produced, sorted descending, with drill-down | — |
| **Prometheus/OpenMetrics** | Store numerator and denominator, derive the ratio at query time | The label/series model; the scrape assumption; storing `%hit` as a series |
| **Honeycomb wide events** | One record per unit of work carrying every dimension; aggregate afterwards | Sampling (a linguist's run is not high-volume telemetry; dropping words drops evidence) |
| **Regex step counters** | Deterministic step counts as the primary cost signal precisely because wall time is not reproducible | — |

## Six problems the design has to answer

### 1. "Arc count" has no referent in the HC engine

There are no arcs — the term is borrowed from the FST side. Candidate meanings differ by orders of
magnitude and produce different `% hit` denominators: rule-body executions after the selector/budget
/depth gates (`stratum.rs:568-586`), `StepBudget` ticks, root-trie hits per candidate word, allomorph
match attempts, or feature unifications. A single "attempts" column across object kinds silently
compares a phonological rule's segment-position match against a lexical entry's trie hit.

### 2. Numerator and denominator are produced in different places

Attempts are observable at the attempt site. Parses are only knowable at the end of the word, after
`merge_equivalent`, `expand_alternatives`, and the validity/match gate
(`rust/crates/pg-parse/src/morpher.rs:397-472`). A rule that fired on a path that later died has
attempts but no parses — which is the *useful* signal — but attributing a *surviving* analysis back
to the objects that produced it is lineage, and lineage is not currently tracked. The cheap
substitute is "participated in a word that got at least one parse", which is a different and much
weaker claim.

### 3. Memoization erases attempts

Analysis and template memo hits replay cached subtrees without entering the leaf rule body
(`rust/crates/pg-rules/src/stratum.rs:664-683`, `789-803`; `rust/crates/pg-memo/src/lib.rs:92-118`).
An effective memo makes an important rule look inactive. The freeze already requires separate
`memo_hits` / `replayed_outputs` fields, but the *product* question — what number the linguist sees
in the "attempts" column — is unanswered.

### 4. `TraceSink` already enumerates the right boundaries and still cannot be used

`TraceSink` has exactly the seams wanted, including the negative events that form the denominator:
`phonological_rule_not_unapplied`, `morphological_rule_not_applied`, `blocked`, `lexical_lookup`
(`rust/crates/pg-rules/src/trace.rs:181-363`). But tracing disables equivalent-analysis merging and
the per-parse memo scope (`rust/crates/pg-parse/src/morpher.rs:344-354`), so it changes the search
space it is measuring. The seam list is reusable as a specification; the vehicle is not.

### 5. Per-text reports are not computable from a PanGloss run today

PanGloss's input is a flat `words.txt` with no text provenance, and `pg-fwdata` extracts only
lexicon, phonology, and morphology — nothing text-related. FLEx's own `ParserReport` has the same
gap for a different reason: it stores one free-text `SourceText` label for the whole run, not a
per-word text.

The mechanism *does* exist on the LibLCM side: `IWfiWordform.OccurrencesInTexts` is a real
project-wide backref to segments, and `ISegment.Owner.Owner as IStText` recovers the owning text
(`liblcm/src/SIL.LCModel/DomainImpl/OverridesLing_Wfi.cs:389`, `528-552`). Two cautions: the first
call per project is documented as "quite slow", and this is a FieldWorks-side capability, not a
PanGloss one.

### 6. Time is the column most likely to mislead

Three independent findings converge. Instrumentation-style profiling carries variable overhead
against ~1-2% for sampling, and a leaf rule application can be fast enough that timer overhead is
the same magnitude as the measured value — so a `%hit`-sorted report surfaces exactly the rules whose
timing is worst-measured. Profiling overhead consumes the same `--word-timeout-ms` deadline as
parsing (`rust/crates/pg-parse/src/morpher.rs:350-355`), so a timed-out run's stats are censored.
And the archived Machine mechanism is the cautionary case: 63,421 of 64,037 ms landed on one coarse
node while every child read zero (`rust/docs/o2-profile-findings.md:49-51`).

## The storage fork nobody has named yet

Two row grains, and the choice drives everything downstream:

- **Wide, one row per word.** Every dimension of that word plus a nested object→counters map. Row
  count equals word count (10^4-10^5). Matches the Honeycomb precedent directly. Nested maps are
  awkward for CSV and for naive `GROUP BY`.
- **Long, one row per (word, object).** Flat, trivially aggregable, CSV-native. Row count is words ×
  objects-touched-per-word — plausibly 10^2-10^3 objects per word, so 10^6-10^8 rows.

For calibration: the largest tracked corpus here is `sena-words.txt` at 7,121 words, so nothing in
the current test set exercises either shape. A real FieldWorks project is the 10^4-10^5 case, and
`CmdCheckParserOnAll` schedules every wordform in every text with no sampling or limit.

## Vocabulary collision

FLEx has no term for attempts and no term for a ratio anywhere in `ParserUIStrings.resx`. Its
existing columns are **Words Parsed**, **No Parses**, **Num Analyses**, **Failed Analyses**,
**Disapproved Analyses**, **Unknown Analyses**, **Num Changed Analyses**, **Error Messages**,
**Parse Time**. Every one is a raw count; there is no export and no ratio.

So "attempts / parses / % hit" is coined vocabulary. The hazard is that FLEx's corpus-level "Words
Parsed" is denominator-shaped and this feature's per-object "parses" is numerator-shaped — a linguist
looking at both reports sees the same word meaning two different things.

## Recommended shape

Not a survey — one recommendation, to be attacked in the grill:

1. Ship the free half first. A per-word JSONL of existing `ParseOutcome` + `WordAnalysis` data
   (`steps`, elapsed, `capped`, `timed_out`, `candidates_generated`, `morpheme_ids`,
   `root_morpheme_index`) gives per-entry and per-affix **parses**, per-word **time**, and corpus
   coverage with zero hot-path instrumentation and zero semantic risk. It also validates the storage
   grain, the aggregation engine, and the CSV report layout against real volume before anything
   touches the parse loop.
2. Then add attempts via the frozen `WorkCollector` seam, deterministic counters only, with memo
   events and dedup boundaries named separately.
3. Keep timing advisory throughout and excluded from golden equality, per the freeze.
4. Aggregate in a separate invocation over the stored records, not at the end of the run — that is
   what "computed on demand" requires, and it makes the stored file the contract rather than the
   report.

## Grill queue

### Units and vocabulary

1. **What is exactly one "attempt", per object kind?** Rule-body execution after the gates,
   `StepBudget` tick, root-trie hit, allomorph match, or something else — and if the answer differs
   per kind, is a single `% hit` column across kinds defensible or actively misleading?
2. **Is `% hit` comparable across object kinds at all?** If not, does the report need separate tables
   per kind rather than one sortable list?
3. **Do you accept coining "Attempts" / "Parses" / "% Hit"** against FLEx's existing "Words Parsed" /
   "No Parses" / "Parse Time" — and what happens when a linguist reads both reports in one sitting?
4. **Per-object "parses": participation or contribution?** Does a rule that fired on a path that died
   score zero, or does "was active during a word that parsed" count? The first needs lineage; the
   second is cheap and weaker.

### Attribution semantics

5. **Memo hits.** Does a memo hit count as an attempt for the rules inside the replayed subtree —
   yes (attribute replayed work), no (physical executions only), or a separate column? All three are
   defensible; the linguist-facing consequence differs sharply.
6. **Is fan-out a fourth column?** The prior health analysis argues that outputs/amplification, not
   attempts, is the actionable signal — a cheap rule emitting many candidates is causally important
   while its own body is fast. Is `attempts / parses / % hit / time` the right four, or the wrong
   four?
7. **Guessed and supplied roots.** Which lexical entry do their attempts and parses land on, given
   `morpheme_ids` carries `u32::MAX` for a fabricated root?
8. **Which dedup boundary is "parses"?** Raw rule outputs, template outputs, stratum outputs,
   pre-merge candidates, and final analyses are all different numbers. `candidates_generated` and
   `structured.len()` already differ by design.

### Scope, filtering, storage

9. **What is the word-selection filter language?** FLEx offers only current-text / genre / all, with
   no sampling, no limit, and no only-failing mode. Do you want more (only-failing, only-changed,
   top-N slowest, sample N, frequency ≥ k, regex) — and are you willing to fund the state each needs
   that PanGloss does not have?
10. **Where does text provenance come from?** Importer learns interlinear texts; the word list gains
    a text column; or the per-text join happens on the FieldWorks side using `OccurrencesInTexts`.
    Only the second preserves PanGloss's current input contract, and only the third avoids
    duplicating LibLCM.
11. **Row grain: wide-per-word or long-per-(word, object)?** 10^4-10^5 rows with nested maps, or
    10^6-10^8 flat rows.
12. **Is the per-word file a durable artifact or a scratch intermediate?** If durable, it needs a
    schema version, run identity, grammar provenance, and an unknown-field policy from day one —
    none of which FLEx's `ParserReport` has, and it is the first thing that document says not to
    copy.
13. **Do you want run-to-run comparison?** FLEx's diff is arithmetic subtraction between two whole
    reports. Doing that per-object forces object identity stable across grammar edits — precisely
    what killed the archived rule-stats naming scheme.

### Time

14. **Self time, inclusive time, or both?** Soufflé reports both. Does the HC rule-application model
    even have a clean self-versus-descendant boundary to attribute against?
15. **Does a per-object time column ship at all in v1**, given that the cheapest and most frequent
    rules — the ones a `% hit` sort surfaces first — are the ones whose timing is dominated by timer
    overhead?
16. **`--stats` with `--word-timeout-ms`.** Profiling overhead consumes the same deadline. Refuse the
    combination, or ship the report marked censored?
17. **`--stats` with `--threads N`.** Counters must be thread-count invariant; timing never is. Is a
    recorded thread count plus counter-only golden tests acceptable?

### Aggregation and delivery

18. **What aggregates?** DuckDB as a dependency, DuckDB shelled out to, a hand-rolled `GROUP BY` over
    the specific reports needed, or emit CSV and let the linguist use Excel. The repo's build
    hardening makes a new native dependency a real cost.
19. **One invocation or two?** "Computed on demand" implies `--stats` writes records and a separate
    subcommand produces CSVs. Confirm, because it decides whether the record file or the report is
    the stable contract.
20. **Does this need to be portable to Machine HC in C#?** If yes, the record schema is a wire
    contract from day one, not an internal format — and the archived C# mechanism's removal from the
    `conformance-framework` branch means there is no existing shape to align with.
21. **What does the linguist do with the answer?** The prior health work is firm that a construct's
    cost never licenses an automatic grammar edit and that remedies stay conditional on linguistic
    equivalence. Is this report a performance tool, a grammar-quality tool, or both — and what does
    it refuse to say?
