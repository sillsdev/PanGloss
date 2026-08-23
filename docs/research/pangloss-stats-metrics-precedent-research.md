# Precedent research: per-grammar-object cost/success stats

Date: 2026-08-22

## Conclusion

The requested feature — per-lexical-entry/per-rule attempts, parses, %hit, and total time, for
10^4–10^5 authored objects, aggregated on demand into CSV — is a **wide-event problem, not a
Prometheus problem**. Prometheus's data model is built around the opposite assumption: a small,
bounded set of label values per metric, with ratios computed at query time via `rate()`/`increase()`
over monotonic counters, never stored directly. Its own documentation calls unbounded per-entity
labels (user IDs, and by direct analogy, lexical-entry IDs) the textbook anti-pattern, and the
ecosystem's answers (recording rules, cardinality limiters, remote-storage sharding) all exist to
protect a always-on TSDB server from exactly the shape being proposed here — a concern that does
not even apply, because PanGloss is a batch CLI with no scrape loop and no persistent server.

The correct precedent is Honeycomb/Charity Majors's "wide structured events" model: one record per
unit of work (per word, here) carrying every dimension that might matter, with metrics/ratios/top-N
derived afterward by a query engine over the raw records — which is exactly the JSONL-per-word +
on-demand-aggregation shape already proposed. DuckDB is the practitioner-standard local query engine
for this: it reads JSONL/CSV/Parquet directly with no server and aggregates hundreds of millions of
rows in seconds; Parquet is 3–30x smaller/faster than JSONL at scale but JSONL remains the right
*write* format for an append-only per-word log. The closest existing tool-shape precedent for "which
rule cost me the most" is Soufflé's Datalog profiler (self/total/recursive time per rule, sorted
descending) — worth imitating directly. The one place Prometheus's discipline *is* directly
transferable is the timing-honesty rule: fine-grained per-call wall-clock timers are noisy at the
scale involved and must be kept advisory and excluded from deterministic/golden comparisons, exactly
as PanGloss's existing profiler design (`docs/research/fieldworks-run-tests-backend-profiler.md`)
already concludes independently.

## 1. Prometheus / OpenTelemetry data model

Prometheus's model is four metric types over a `{name, labels} -> timestamped float` series:
**counter** (monotonic, only up or reset-to-zero on restart), **gauge** (arbitrary up/down),
**histogram** (client-side bucketed counts + sum, each bucket its own counter series), and
**summary** (client-side quantile estimation, not aggregatable across instances)
([Prometheus metric types](https://prometheus.io/docs/concepts/metric_types/);
[Histograms and summaries](https://prometheus.io/docs/practices/histograms/)). Every value is a
"simple timestamped floating point value," and a histogram's buckets or a summary's quantiles are
each *separate time series* — cardinality inside one logical metric is already a cost even before
labels are considered ([Better Stack guide](https://betterstack.com/community/guides/monitoring/prometheus-metrics-explained/)).

**Ratios are a query-time construct, never a stored one.** The canonical rule: counters are
consumed through `rate()` (per-second, for graphs/alerts) or `increase()` (absolute count, for
totals/billing); recording rules pre-compute expensive aggregations under a
`level:metric:operations` naming convention, stripping `_total` and naming ratio rules with
`_per_` (e.g. `job:errors_per_requests:ratio`); and — the mathematically important rule — **a ratio
is aggregated by summing numerator and denominator separately and dividing at the end**, never by
averaging pre-computed ratios ([Recording rules](https://prometheus.io/docs/practices/rules/)).
This is the direct precedent against storing a `%hit` field as a first-class stored value across
merged runs — it should be computed from separately-stored `attempts`/`parses` counts every time,
including at every level of aggregation (per-word, per-text, per-corpus).

**OpenMetrics** formalized the wire format Prometheus popularized: the `_total` suffix is now
*required* to mark a counter type in text exposition ([OpenMetrics 1.0
spec](https://prometheus.io/docs/specs/om/open_metrics_spec/); [GitHub
spec](https://github.com/prometheus/OpenMetrics/blob/main/specification/OpenMetrics.md)), though
OpenMetrics 2.0 relaxed the requirement to improve OpenTelemetry compatibility
([OTel/Prometheus compatibility](https://opentelemetry.io/docs/specs/otel/compatibility/prometheus_and_openmetrics/)).
**Exemplars** — a metric sample carrying an attached trace/span ID as a single example data point —
are the mechanism the ecosystem built specifically so that an aggregate metric can still point at
one concrete instance without putting per-request identity in a label
([Prometheus exemplars via OTel](https://google-cloud-opentelemetry.readthedocs.io/en/latest/examples/prometheus_exemplars/README.html);
["put per-request detail in trace attributes, not metric labels"](https://oneuptime.com/blog/post/2025-09-22-connecting-metrics-to-traces-with-exemplars/view)).
This is itself evidence that the ecosystem does not believe labels should carry per-entity
identity — exemplars exist *because* they don't.

## 2. Cardinality: the crux

Prometheus's own naming-practices page states the anti-pattern directly: **"Do not use labels to
store dimensions with high cardinality (many different label values), such as user IDs, email
addresses, or other unbounded sets of values"**
([Metric and label naming](https://prometheus.io/docs/practices/naming/)) — fetched directly and
confirmed to give **no numeric threshold**; the guidance is categorical, not a "N is fine, N+1 is
not" line. The mechanism: "every unique combination of key-value label pairs represents a new time
series" (same page), so `method × status_code × endpoint × customer_id` with even 10,000 customers
turns one counter into 12 million series
([Last9](https://last9.io/blog/how-to-manage-high-cardinality-metrics-in-prometheus/)). A PanGloss
project's 10^4–10^5 lexical entries, crossed with even 4 metrics (attempts/parses/time/hit-rate) and
any second dimension (per-text breakdown), lands in exactly that multiplicative-explosion shape by
direct analogy — the entry ID plays the same role as the forbidden `user_id`/`customer_id` label.

The ecosystem's answers are all **server-protection** mechanisms, which do not transfer to a batch
CLI with no long-lived TSDB: drop/rewrite offending labels via `metric_relabel_configs`, pre-aggregate
away the high-cardinality dimension with recording rules before it ever hits long-term storage, cap
ingestion with `sample_limit`, or bucket unbounded values
([Last9](https://last9.io/blog/how-to-manage-high-cardinality-metrics-in-prometheus/)). At the
storage-tier scale, Thanos/Mimir/VictoriaMetrics benchmarks around 5.5 million active series show
p99 query latencies of 20–47 seconds even with all this mitigation in place
([sanj.dev comparison](https://sanj.dev/post/prometheus-scaling-thanos-mimir-victoriametrics/);
[procedure.tech](https://procedure.tech/blogs/prometheus-monitoring-at-scale/)) — real infrastructure
built and tuned specifically to survive cardinality that PanGloss's own report would produce *by
design* for a single project, every run. This is the strongest single argument against forcing the
feature into the Prometheus data model: the entire remote-storage ecosystem's engineering effort has
gone into avoiding, at great cost, the exact thing this feature wants to do on purpose.

## 3. The alternative shape: wide structured events

Charity Majors (Honeycomb co-founder/CTO) frames "Observability 2.0" as one source of truth — wide,
structured events, one per unit of work — from which metrics, traces, and logs are all *derived*
after the fact, replacing the "three pillars, three sources of truth" model
([Honeycomb: one key difference](https://www.honeycomb.io/blog/one-key-difference-observability1dot0-2dot0);
[charity.wtf tag](https://charity.wtf/tag/observability-2-0/)). Concretely: "the pattern is simple:
emit one structured record per unit of work with all the important fields already attached,"
and mature Honeycomb datasets run "200–500 dimensions wide"
([Boris Tane, wide events 101](https://boristane.com/blog/observability-wide-events-101/)). The
value proposition stated directly: with everything captured per-event, "you can derive any metric
across any (number of) dimensions" after the fact —
["compute the P90 latency of a particular endpoint, for a particular client version, in a particular
country"](https://www.honeycomb.io/blog/structured-events-basis-observability) without having
pre-declared that cross-section as a metric+label combination in advance. This is precisely the
PanGloss ask: attempts/parses/time sliced by lexical entry, by rule, by template, by text, decided
*after* the run rather than baked into what gets counted during it.

**This is strictly better than metrics here specifically because**: (a) the dimension of interest
(lexical entry / rule / template identity) is exactly the kind Prometheus forbids as a label; (b) the
slicing needed is not known in advance — a linguist may want "top rules by %hit for text X" one day
and "top entries with zero parses" the next, and a wide per-word record supports both without
re-instrumenting; (c) no long-lived aggregation server exists to protect from cardinality, so the
usual cost side of "just store everything" — server memory pressure, remote-write cost, index
bloat — mostly does not apply to a local batch run's disk usage.

**Costs, and the honest counterargument.** Sampling is the datadog/Grafana-world's standard cost
control for wide-event volume, and the standard critique of sampling is that "it hides the rare
events you actually care about" — incidents and edge cases are statistically the first to be dropped
([semistructured.substack.com](https://semistructured.substack.com/p/datadogs-moat-is-the-human-at-the)).
That critique argues *for* PanGloss keeping one record per word with **no sampling** (a project's
worst-hitting entries are exactly the rare tail a sampled record would drop), but it also means the
volume cost is real and unmitigated: a 100k-word corpus times dozens of touched-object IDs per word
is a genuinely large per-run artifact, which is why an explicit query engine (§4) rather than
"eyeball the file" is required at this scale.

## 4. On-demand aggregation over JSONL/CSV: what practitioners actually use

**DuckDB is the practitioner default** for exactly this "no server, aggregate a pile of files"
shape. It queries CSV, JSON(L), and Parquet directly with SQL, with no import/ETL step, including
glob patterns across many files as one virtual table
([MotherDuck: No-ETL](https://motherduck.com/learn/no-etl-query-raw-files/);
[DuckDB file formats guide](https://duckdb.org/docs/lts/guides/performance/file_formats)). It has
been demonstrated aggregating 120 million rows in two seconds and querying 1.7 billion rows across
175 remote Parquet files without importing them
([DuckDB performance chapter](https://motherduck.com/duckdb-book-summary-chapter10/)). This is a
direct match for "compute a corpus-total CSV and a per-text CSV on demand from a per-word JSONL log"
— it is one `duckdb -c "SELECT ... FROM read_ndjson('run.jsonl') GROUP BY ... "` invocation, no
server, no daemon, embeddable as a single binary.

**Format tradeoffs at this row scale.** Parquet is columnar, compresses far better, and is reported
"3–10x smaller and 10–30x faster to read" than CSV for analytical (filter/aggregate) workloads,
with one comparison citing "600x" over CSV and "1200x" over row-oriented JSON for a full-table-scan
benchmark ([DriveDataScience](https://www.drivedatascience.com/parquet-csv-json-file-format-comparison/);
[MotherDuck chapter 10](https://motherduck.com/duckdb-book-summary-chapter10/)). But that comparison
is for *reading for analysis*, not *writing incrementally during a run*: JSONL is explicitly
recommended as "the sweet spot for machine-generated, machine-consumed... data that needs to be
appended to incrementally"
([format comparison piece](https://medium.com/featurepreneur/from-json-to-csv-to-parquet-the-rise-of-apache-parquet-as-the-ultimate-data-storage-solution-196c28375847)),
which matches a batch parser appending one line per word as it completes, versus Parquet's
row-group/compression model which wants to be written in one batched pass. **Realistic breakpoint
for this feature:** at 10^4–10^5 words with single/double-digit dimensions per word, a JSONL file is
low-single-digit-to-tens of MB — nowhere near where JSONL "stops being fine" (that threshold, per
the columnar-performance literature above, is closer to the 100MB–GB range where per-row JSON
parsing overhead and lack of column pruning start to dominate query time). JSONL-as-write-format +
DuckDB-as-read/aggregate-engine is the correct combination for the stated scale; Parquet becomes
worth it only if runs get chained/archived at a much larger multi-run, multi-project scale than
described.

**Simpler CLI tools** (`jq`, [Miller](https://github.com/johnkerl/miller),
[qsv](https://github.com/dathere/qsv)) exist for lighter-weight JSONL/CSV manipulation — Miller is
described as "awk for name-indexed data" and handles JSONL/CSV/TSV natively with aggregation verbs;
qsv is a fork of `xsv` with native `jsonl`/`tojsonl` conversion and can even run Polars SQL over CSV
([x-cmd Miller page](https://www.x-cmd.com/pkg/miller/); [qsv
GitHub](https://github.com/dathere/qsv)). These are viable as a lighter dependency than embedding
DuckDB, but none match DuckDB's combination of SQL expressiveness (GROUP BY, window functions for
top-N) and read-arbitrary-glob convenience for the "aggregate then also drill into worst words"
requirement.

## 5. Pull vs push, and in-process metrics in Rust

Surveyed Rust metrics crates, if PanGloss went the Prometheus-shaped route:

- **`metrics` + `metrics-exporter-prometheus`** — the `metrics` facade crate is deliberately generic
  (analogous to `log`/`tracing`), with `metrics-exporter-prometheus` as one possible sink; the
  exporter runs background "upkeep tasks" to drain histogram buckets that otherwise grow unbounded
  in memory ([docs.rs](https://docs.rs/metrics-exporter-prometheus/latest/metrics_exporter_prometheus/)).
  That upkeep task is itself evidence the crate assumes a *long-lived* process — it needs a
  background loop to run periodically, which a one-shot CLI invocation never gets to run more than
  once.
- **`prometheus-client`** — the reference-quality Open Metrics implementation for Rust, emphasizing
  type safety and performance ([crates.io](https://crates.io/crates/prometheus-client)).
- **`opentelemetry`/`opentelemetry-prometheus`** — OpenTelemetry-Rust is the actively maintained
  cross-vendor SDK; notably, the OTel project itself now steers *away* from the Prometheus exporter
  path and recommends OTLP as the more actively maintained integration, since "Prometheus natively
  supports OTLP" going forward
  ([opentelemetry-rust](https://github.com/open-telemetry/opentelemetry-rust); [OTel spec status
  doc](https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/metrics/sdk_exporters/prometheus.md)).
- **`hdrhistogram`** — a straight Rust port of Gil Tene's HdrHistogram, purpose-built for
  latency-shaped data: fixed memory footprint regardless of sample count, no allocation on the
  record path, and recording costs cited around 3–6ns per value on 2014-era hardware
  ([HdrHistogram_rust](https://github.com/HdrHistogram/HdrHistogram_rust);
  [hdrhistogram.org](https://hdrhistogram.org/)). This is the right tool *if* PanGloss wanted a
  latency-distribution view of "time per word/rule," decoupled entirely from the labels-and-series
  cardinality problem — a single histogram per rule-ID key, not a time series per lexical entry.

**Why the pull model doesn't fit at all.** Prometheus's default mode is a server periodically
scraping a live `/metrics` HTTP endpoint. PanGloss is a batch CLI that starts, runs, and exits — there
is no interval over which to be scraped. The ecosystem's own workaround for exactly this shape is
the **Pushgateway**, explicitly documented as being for "capturing the outcome of a service-level
batch job" that would otherwise terminate before a scrape could happen
([Pushing metrics](https://prometheus.io/docs/instrumenting/pushing/);
[SigNoz](https://signoz.io/guides/prometheus-pushgateway/)) — i.e., even Prometheus's own answer to
"batch job" is to bolt on an intermediary long-lived service, which is infrastructure PanGloss's
target users (a linguist running a local CLI) do not have and should not need to stand up.

## 6. Timing measurement honesty

The standard critique of fine-grained instrumentation applies directly to a "time per rule" metric:
**instrumentation overhead is large and highly variable** — measured at "16% average, 0–53% range"
for instrumentation-style profilers versus "1.06% average, 0.3–2% range" for sampling/LBR-based
profiling, a roughly 15x overhead gap
(cited from a hardware-counted PGO study, via search synthesis — see profiler comparison
literature, e.g. [Visual Studio profiler docs on collection
methods](https://learn.microsoft.com/en-us/visualstudio/profiling/understanding-performance-collection-methods-perf-profiler?view=vs-2022)).
Timer-call overhead itself is non-negligible at the scale of a single rule application: even a
"just call `Instant::now()` twice" measurement carries roughly tens of nanoseconds of call/latency
overhead on modern hardware (Windows QPC access is cited around 20ns with 100ns hardware-clock
resolution; a monotonic-clock read plus its own dispatch overhead is "roughly 40ns" on typical
systems) ([Acquiring high-resolution time stamps,
Microsoft](https://learn.microsoft.com/en-us/windows/win32/sysinfo/acquiring-high-resolution-time-stamps)).
When the work being timed (one leaf rule application) can itself take single-digit-to-low-double-digit
nanoseconds, the timer's own overhead is the same order of magnitude as the signal — the textbook
case for "this per-leaf-call number is not trustworthy at face value."

What profilers do instead: **sampling** (periodic stack snapshots, statistical rather than exact,
much lower overhead) versus **instrumentation** (exact call counts, exact per-call timing, high and
variable overhead) — with sampling profilers explicitly trading "less accurate" for "much lower
disruption," and instrumentation profilers valued specifically because they *do* give "exact call
counts," which is a different, and cheaper, kind of exactness than exact per-call timing
([gamedev.center summary](https://gamedev.center/sampling-vs-instrumentation-profilers-in-unity-when-to-use-each-for-better-performance/)).
This maps directly onto the design choice already reached independently in
`docs/research/fieldworks-run-tests-backend-profiler.md` (§ "Add a typed work observer..."): treat
**call/success/output counts as deterministic and load-bearing** (cheap to collect exactly, safe to
put in golden/regression tests), and treat **wall-clock duration as advisory only** — recorded
because users find it friendly, explicitly excluded from any test that asserts equality, and never
the sole signal a "most expensive rule" ranking relies on.

## 7. Precedents in parsers/compilers/rule engines for "which rule cost me the most"

- **Clang `-ftime-trace`** emits a Chrome Trace Event JSON per translation unit with nested timing
  events, letting a separate visualizer (e.g. ClangBuildAnalyzer) reconstruct hot spots
  ([Clang docs](https://clang.llvm.org/docs/analyzer/developer-docs/PerformanceInvestigation.html);
  [ClangBuildAnalyzer](https://github.com/aras-p/ClangBuildAnalyzer)) — the report shape is
  fine-grained nested spans, aggregated and ranked by a downstream tool, not by the compiler itself.
- **Rustc.** `-Z time-passes` was rustc's older flat per-pass timer; it was deliberately superseded
  by `-Z self-profile`, described as intended to have "at least as much granularity as `-Z
  time-passes` had pre-queries"
  ([rust-lang/rust#53631](https://github.com/rust-lang/rust/issues/53631)) — i.e., the project's own
  history is a move from coarse pass timing toward a structured, queryable self-profile format,
  echoing this repo's move away from the archived HC rule-stat tree's coarse-stratum-only timing.
- **Soufflé (Datalog).** The most directly analogous precedent: its profiler's per-rule/relation
  table reports **TOT_T** (total runtime), **NREC_T** (non-recursive portion), **REC_T** (recursive
  portion), **COPY_T** (merge/copy overhead), and **TUPLES** (facts produced) per rule, and can list
  rules "in descending order of total time consumed"
  ([Soufflé profiler docs](https://souffle-lang.github.io/profiler)). This is essentially the exact
  report shape requested here (attempts↔tuples-considered, parses↔tuples-produced, time, sorted
  top-N) applied to logic-program rules instead of morphological rules — strong direct precedent for
  the report's column layout and sort order.
- **Rule engines (Drools/CLIPS-family).** The pattern found is *ad hoc*, not built-in: practitioners
  attach an `AgendaEventListener` (or equivalent activation/firing hook) themselves and tally which
  rules fired against which facts, since maintaining "statistics on which rules are fired for each
  fact" is presented as something you build on top of the engine's listener API, not a shipped report
  ([DZone, Learn Drools part 6](https://dzone.com/articles/learn-drools-part-vi-rules-and-statistics)).
  This is a negative precedent worth noting: mainstream production rule engines apparently do not
  ship a built-in per-rule cost/hit-rate report either, reinforcing that this is worth building
  deliberately rather than assuming an off-the-shelf shape exists to copy wholesale.
- **Regex engines.** Debugging tools for catastrophic backtracking count **engine steps** as the
  cost proxy instead of wall time specifically because step count is deterministic and
  reproducible where wall time is not — one cited example: 65,000 steps to reject a 14-character
  adversarial string versus 54 steps for a well-behaved one
  ([regular-expressions.info](https://www.regular-expressions.info/catastrophic.html)). This is
  independent, cross-domain support for PanGloss's existing preference for deterministic
  attempt/step counters over timing as the primary cost signal.
- **Foma/xfst.** No evidence found of a built-in per-rule cost/statistics report in foma or xfst
  documentation or tutorials searched ([foma morphology
  tutorial](https://fomafst.github.io/morphtut.html); [Apertium foma
  wiki](https://wiki.apertium.org/wiki/Foma)) — `apply up`/`apply down` are described purely as
  transduction operations with no profiling output surfaced in the material found. Treat this as an
  **absence of precedent**, not a confirmed negative — foma's own C source or a dedicated forum
  search would be needed to rule out an internal counter/report flag, which was out of scope for
  this web-only pass.

## Comparing the candidate storage/aggregation shapes

| Dimension | Prometheus-style pre-aggregated metrics | Wide per-word events + on-demand aggregation | Hybrid (per-word events + coarse live counters) |
|---|---|---|---|
| **Cardinality tolerance** | Poor — 10^4–10^5 lexical-entry labels is the textbook anti-pattern ([naming docs](https://prometheus.io/docs/practices/naming/)) | Good — cardinality only matters at query time, and DuckDB/columnar engines are built for exactly this ([MotherDuck](https://motherduck.com/learn/no-etl-query-raw-files/)) | Good for the event half; coarse live counters (e.g. a running "words processed" gauge for a progress bar) stay genuinely low-cardinality |
| **Ratio correctness** | Correct only if disciplined (sum-then-divide, never average pre-computed ratios — [recording rules](https://prometheus.io/docs/practices/rules/)); easy to get wrong by storing %hit directly | Correct by construction — %hit is always derived from stored attempts/parses at query time, at any aggregation level | Same as wide-events for the per-object ratios; live counters need the same sum-then-divide discipline if ever combined |
| **Time attribution honesty** | Histograms/summaries handle distributions well, but per-label timing multiplies the cardinality problem further | Naturally separates deterministic counts (safe) from advisory per-word wall time (excluded from golden tests), matching `docs/research/fieldworks-run-tests-backend-profiler.md`'s conclusion | Same separation available |
| **Determinism / testability** | Weak — wall-clock-derived series are inherently non-deterministic across runs; hard to golden-test | Strong — deterministic fields (attempts, parses, outcome signature) can be asserted exactly; timing fields excluded | Strong, same as wide-events |
| **Run overhead** | Requires either an always-on scrape target (doesn't exist for a CLI) or Pushgateway infrastructure the user doesn't have ([pushing metrics](https://prometheus.io/docs/instrumenting/pushing/)) | One append-only write per word; no server process; cost is disk, not CPU/latency of a scrape loop | Same low overhead, plus trivial live counters |
| **What the user actually asked for** | Does not match — user wants per-entry drill-down and a friendly total-time number, not a scrape-and-alert system | Matches directly — per-word JSONL, on-demand CSV aggregation, corpus-total + per-text reports is literally this shape | Matches, with a possible added convenience (live progress) not requested but cheap to add |

## What would break under the naive reading "make it like Prometheus"

- **Cardinality explosion on day one.** A single mid-size project (10^4–10^5 entries) instrumented
  as Prometheus labels would, by the ecosystem's own stated benchmarks, already sit in the range
  where dedicated remote-storage systems measure tens-of-seconds query latency at millions of
  series — for one project, one run ([sanj.dev](https://sanj.dev/post/prometheus-scaling-thanos-mimir-victoriametrics/)).
- **No scrape target exists.** A batch CLI process exits before any interval-based scraper could
  ever reach it; the ecosystem's own patch for this (Pushgateway) requires infrastructure
  ("an intermediary job which Prometheus can scrape" — [pushing
  metrics](https://prometheus.io/docs/instrumenting/pushing/)) that a linguist running a local tool
  does not have.
- **%hit stored directly instead of derived would silently misaggregate.** Averaging a pre-computed
  per-run ratio across texts/corpora violates the sum-numerator-then-divide rule and produces a
  number that is simply wrong when text sizes differ — the exact mistake the recording-rules docs
  warn against ([recording rules](https://prometheus.io/docs/practices/rules/)).
  Since a per-text vs corpus-total report is explicitly wanted, this failure mode is not
  hypothetical — it will be hit on the very first two-report design the feature ships.
- **A one-shot process never runs its own upkeep.** The `metrics-exporter-prometheus` upkeep-task
  model assumes a background loop the process never gets to run more than once, so histogram
  buckets/gauges silently never drain
  ([docs.rs](https://docs.rs/metrics-exporter-prometheus/latest/metrics_exporter_prometheus/)).
- **Per-call timing would be reported with false precision.** Leaf rule applications can be fast
  enough that timer overhead (order tens of ns per read) is the same magnitude as the value being
  measured, and instrumentation-style profiling is measured to carry 0–53% variable overhead versus
  ~1–2% for sampling — a naive "wrap every rule call in `Instant::now()`" implementation would report
  numbers dominated by measurement noise, especially for the cheapest/most-frequently-hit rules,
  which are exactly the ones a %hit-sorted report surfaces first.

## Open questions this research cannot settle

1. **Exact DuckDB embedding shape for PanGloss's Rust codebase.** Whether to shell out to a bundled
   `duckdb` binary, use the `duckdb-rs` bindings in-process, or hand-roll the (much smaller) specific
   GROUP BY/top-N aggregations needed without a general SQL engine at all — this is an
   implementation choice this research did not evaluate against PanGloss's existing dependency and
   build-hardening policies (`rust/tools/pg.ps1`, sccache, etc.).
2. **What the stable authored-object "key" is for the JSONL schema**, and whether it matches the
   key already chosen for the tracing/profiler design in
   `docs/research/fieldworks-run-tests-backend-profiler.md` — this research surveyed how *other*
   systems key their records (Soufflé's rule ID/name, OTel's trace/span ID) but cannot settle
   PanGloss's own grammar-object identity scheme.
3. **Whether "total time" per rule should be self time, inclusive time, or both** — the
   Soufflé/profiler precedent strongly suggests reporting both (their NREC_T/REC_T split is exactly
   a self/recursive-inclusive distinction), but whether PanGloss's rule-application model has a
   clean self-vs-descendant-work boundary to attribute against is a codebase question, not a
   research one.
4. **How much wide-event volume is actually tolerable** for the target deployment (a linguist's own
   machine, disk space, patience for a report-generation pass) — this research found general
   JSONL/Parquet breakpoints in the literature but has no PanGloss-specific number for "how large a
   run before JSONL-then-DuckDB stops feeling instant."
5. **Whether any live/streaming signal is wanted at all** (e.g., a progress indicator during a long
   corpus run) — the hybrid row in the comparison table above is speculative; nothing in the feature
   request asks for it, and this research did not investigate whether PanGloss's batch CLI already
   has a progress-reporting convention that such a counter would need to match.
