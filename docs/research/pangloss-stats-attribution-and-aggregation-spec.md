# `--stats`: attribution semantics and aggregation format

Date: 2026-08-22

Status: design of record. Settled by product grill (25 rounds). Implementation plan:
`docs/superpowers/specs/2026-08-22-hc-stats-implementation-plan.md`.

Companion research: `search-tree-failure-attribution.md` (why `terminated-at`),
`pangloss-stats-metrics-precedent-research.md` (why not Prometheus),
`fieldworks-parser-report-storage-and-scoping.md` (what FieldWorks does),
`pangloss-hc-stats-feature-report.md` (the synthesis this refines),
`pangloss-profiler-health-design-freeze.md` (prior product decisions this must not contradict).

## Conclusion

A gated collector records, per analyzed word, seven integers for every
`(authored object, stratum, allomorph)` combination that participated. Nothing else is collected
during the run, no ratio is stored, and no timer is read inside the search.

Blame for a dead path goes to **terminated-at** — the object applied immediately before the path
died — never to the whole ancestor path.

Time is a **derived estimate**: one universal work unit (segments touched per attempt) times a
per-kind constant, measured by `pangloss calibrate` over the conformance suite on one core and
committed to the repo with provenance. The claim is explicitly relative, not absolute.

Records live in **one** SQLite cache per FieldWorks data path, owned by PanGloss, accumulating across
runs and wiped only when the grammar changes. Words already cached are not recomputed. Reports are
`GROUP BY` queries.

The objective is troubleshooting: give a human or an AI enough to say **"look here"**, at rules and
lexemes that **over-apply**. Correctness is deliberately out of scope — FieldWorks' Run Tests already
does that job. This report asks whether the grammar is *efficient*; Run Tests asks whether it is
*right*. The two are used together, one tuning and one guarding against regression.

## The quick way to get a usable answer

For a human or an AI who wants to know "what is wrong with this grammar" and does not want to read the
rest of this document. Four commands, bounded runtime, qualitative answer.

**1. Bound the run so a pathological grammar cannot hang it.**

```
pangloss batch <grammar> <words.txt> out.tsv --engine=hc --stats \
        --threads 1 --word-timeout-ms 1000 --cache <cache.sqlite3>
```

`--threads 1` is not optional: the default thread count fans words out concurrently and multiplies
memory, and a probe on this machine once reached 30+ GB RSS where single-threaded plus a timeout
finished the same work in minutes. `--word-timeout-ms 1000` makes the worst case *n* seconds for *n*
words — usually far less, since a word that parses in 5ms does not consume its second. **Ten words is
ten seconds, worst case.** That is what makes a first look at an unknown grammar tractable.

**2. Ask what never fires.** The single most actionable report.

```
pangloss stats <grammar> --cache <cache.sqlite3> --group never-fires
```

Rules entered many times that produced nothing, ever. Across three real grammars this found four Sena
rules entered 2.37M times for zero outputs, and five Aweti affixes ~1.56M times each.

**3. Ask what manufactures non-words.**

```
pangloss stats <grammar> --cache <cache.sqlite3> --group object --sort no-root
```

High `no_root` means a rule keeps producing forms the lexicon does not contain — the expensive kind of
over-application, because each bogus form drags a whole subtree of searching behind it.

**4. If you need to know where the *time* goes, not which rule is busiest**, build with the
`stats-calibrate` feature and set `HC_PHASE_PROFILE=1`. This is the only measurement that reads a real
clock, and it attributed 98.6% of Amharic's wall clock and ~100% of Indonesian's.

### How to read the result without being misled

- **Rank by counters, never by `estimated_time_ms`.** See the warning below: measured, that column is
  wrong by ~180x.
- **`attempts` is not cost.** Compounding is entered 2.9% as often as affix allomorphs in Amharic and
  costs more than all of them combined. A ranking by attempts points at the wrong thing; only the
  phase profile settles where time goes.
- **Capped words give floors, not totals.** A word cut off at its timeout contributes real counts up to
  the cut. Fine for ranking, wrong for "this rule costs X". `--exclude-censored` drops them.
- **A cap preserves *which* phases matter, but can invert their order.** Measured on Amharic's full
  673 words, varying only the timeout:

  | cap | total time | parsed | timed out | traversal | compounding |
  |---|---|---|---|---|---|
  | 20s | 1369.7s | 631 | 38 | 41.5% | **52.6%** |
  | 1s | 258.8s | 520 | 149 | **65.6%** | 27.7% |

  The same two phases carry ~93-94% under both caps, so the *candidate set* is stable. But the leader
  flips. A tighter cap **systematically under-weights work that happens deep in a word's search and
  over-weights work that happens constantly from the start** — compounding is entered late, traversal
  runs throughout. The trend is monotone across three samples: worst-22 at 20s gave 64.3/29.5, full at
  20s gave 52.6/41.5, full at 1s gave 27.7/65.6.

  So: **use a cap to discover what to look at, never to decide what to optimise.** Confirm the
  ordering at a cap generous enough that few words hit it, and quote the capped fraction whenever you
  quote a share.
- **Zero outputs on a small word list is ambiguous.** It means either the rule cannot fire or these
  words do not use it. Only a bigger list distinguishes them — say which you have.
- **Prefer `--step-cap` when comparing runs.** A wall-clock cap is machine- and load-dependent, so two
  runs are not comparable; a step cap is deterministic. But note it bounds only admitted morphological
  rule applications, so it does not bound compounding internals or phonological work — for a grammar
  whose cost is inside one compound call, only the wall-clock cap actually bounds the run. Use both.

## Scope

**In v1:** the collector, the SQLite cache, `batch --stats`, `pangloss stats` with the per-word and
per-object reports, `pangloss calibrate`, and the invariant tests.

**Out of v1:** the per-text report. It needs text and occurrence extraction, and `pg-fwdata` handles
no `Text`, `StText`, `StTxtPara`, `Segment`, or `WfiWordform` at all; that work belongs to the
approved-but-unimplemented transient project cache design. Nothing else depends on it.

**Out of scope permanently:** analysis-correctness reporting, signature reconciliation against
FieldWorks-approved analyses, and multi-cache management. The first two are FieldWorks' job; the
third is Motif's.

## The seven counters

Per `(word, object, stratum, allomorph)` row:

| Counter | Meaning | Where it increments |
|---|---|---|
| `attempts` | this object ran once | at the tick site (`rust/crates/pg-rules/src/stratum.rs:586` for morphological rules) |
| `work` | Σ segments touched, one term per attempt | same site, one `.len()` and one add |
| `outputs` | candidates produced | `outs.len()` after the rule body |
| `not_applied` | ran and matched nothing | `outs.is_empty()` |
| `no_root` | applied fine, but the form it produced is not in the lexicon | at the failed lookup, charged to the last rule applied (`w.mrule_apps.last()`) |
| `surface_mismatch` | this root was rebuilt and did not match the actual word | at the synthesis gate, charged to the `lex_entry` |
| `uses` | appeared in at least one surviving analysis | commit-on-pass (below) |

Report labels: **Didn't apply**, **No root found**, **Didn't match the word**. `not_applied` and
`surface_mismatch` are inherited from HC's `FailureReason`
(`rust/crates/pg-rules/src/trace.rs:106-130`, where `SurfaceFormMismatch` appears verbatim);
`no_root` is coined, because a failed lexical lookup is not a rule failure and HC has no name for it.

Counters are **u64** in the collector. The default `--step-cap` is `usize::MAX`
(`rust/crates/pg-cli/src/main.rs:879`), so attempts per word are unbounded unless the caller caps
them, and `work` overflows u32 long before `attempts` does. On disk, SQLite `INTEGER` is dynamically
sized to i64 and varint-encoded, so small counts stay cheap and `SUM()` promotes automatically.

**Pre-tick rejections are not attempts**, matching the engine's own semantics — "a rejected-by-gate
rule was never attempted" (`stratum.rs:568-571`). No `blocked` counter. Accepted blind spot: a rule
absent from the report was gated out, not idle.

### Over-application, stated directly

- **amplification** = `outputs / attempts` — this object is generating candidates
- **`no_root` rate** = `no_root / attempts` — this object is manufacturing forms that are not words

High on either with `uses` near zero is over-application, attributed to the object that *did* it
rather than to the objects that cleaned up after it. Both are computed at report time; neither is
stored.

### What each dead-end flavor means

- high `not_applied` → tried where it cannot apply. Ordering, or an environment too narrow. Cheap
  waste: only the rule body was spent, already counted in `work`.
- high `no_root` → **the expensive over-application.** The rule's body *plus* the entire subtree that
  went on exploring a bogus form.
- high `surface_mismatch` on an entry → that root matches words it has no business matching.

`no_root` lands on whichever rule ran last before the failed lookup, which can be a phonological
rewrite as easily as a morphological rule.

## Dimensions, not objects

`stratum` and `allomorph` are columns in the fact key, never counted objects. Summing over a
dimension recovers the parent's exact total, so there is no second measurement to reconcile and no
double counting. Counting containers instead would reintroduce inclusive-time attribution — the gprof
failure.

**The `NONE` allomorph sentinel is load-bearing.** A rule attempt has cost belonging to no allomorph
— setup before the loop, and failures that never reach one. That residue lands on the `NONE` row, so
`SUM(work) GROUP BY object` equals the rule's true total. Without it an allomorph breakdown silently
fails to add up to its rule: the classic profiler bug.

**Why allomorph granularity is not optional.** The per-allomorph loop calls `compile_parts`, and its
comment states it is "recompiled per call, deliberately"
(`rust/crates/pg-rules/src/morph.rs:1242-1248`). The dominant per-attempt cost happens *per
allomorph*, so rule-level `work` is a poor cost proxy and a rule-level report cannot see where the
time went.

Inside one allomorph loop, four states are distinguishable and worth keeping distinct: gated out by
MPR group (`continue`), tried and failed (`synth_process_allomorph` returns `None`), succeeded, and
**never reached** because an earlier allomorph triggered the disjunctive `break`.

**`direction` is a dimension**, valued `analysis` (unapply) or `synthesis` (apply). It was dropped
once, on the grounds that it only earned its place for engine-to-engine comparison while the ground
truth here is FieldWorks' analyses. That reasoning was wrong for a reason the dead-end columns hide:
merging the directions left the apply-direction paths uninstrumented entirely, and the primary
invariant could not detect the gap because `SUM(attempts) == steps` is analysis-only on *both* sides
— self-consistent rather than complete. A word cheap to analyse and expensive to confirm therefore
under-reported the rules burning its time, silently.

So the invariant is scoped: `SUM(attempts)` over morphological rows equals the word's `steps` only
when filtered to `direction = analysis`, because `StepBudget::tick()` fires solely in
`apply_one_mrule`. Synthesis rows are pinned by their own test, since an invariant that cannot see a
whole direction cannot guard it.

**Templates** are a report-time rollup joining rules to their owning template. One caveat: a rule
appearing in more than one template is attributed to each, so per-template sums can exceed the total.
A rollup artifact, never in stored data.

## Object kinds and identity

`morph_rule`, `phon_rule`, `lex_entry`, `root_index` (one per stratum), `guesser`, `overlay`
(supplied roots).

Identity is retained at ingest, not reconstructed, and every object carries a display label — a GUID
alone cannot say "look here". What exists today:

| Kind | Identity | Quality |
|---|---|---|
| `lex_entry` | `LexEntryDef.authored_id`, the source `LexEntry.Guid` under snapshot compilation (`rust/crates/pg-grammar/src/model.rs:757-764`) | authored |
| `phon_rule` | `RewriteRuleDef.xml_id` (`model.rs:403-412`) | authored |
| `morph_rule` | indirect: `AffixProcessRuleDef.morpheme` → morpheme registry → MSA GUID. The def itself has no id, only `morpheme` and `name` (`model.rs:622-640`) | authored |
| `stratum` (dimension) | structural locator: index + name. `StratumDef` has only `name` (`model.rs:1050-1060`) | structural |
| `allomorph` (dimension) | structural locator: owning object + index. `AllomorphId` is a dense runtime handle | structural |
| `root_index`, `guesser`, `overlay` | synthetic stable ids; no authored counterpart exists | synthetic |

Structural locators for stratum and allomorph are deliberate. Retaining real identities for them
means changes inside grammar loading, a far larger blast radius than a stats feature should carry.
An `identity_quality` column records which is which, so a report can never present a locator as an
authored identity, and a later upgrade needs no schema change.

## Why terminated-at, not the whole path

Charging every ancestor of a dead leaf makes a common, innocent early rule absorb the waste caused by
an overbroad rule deeper in the tree. Rejected independently in four fields:

- **CSP.** `wdeg`/`dom-wdeg` bumps a constraint's weight only when it is the *immediate* cause of a
  domain wipeout (Boussemart, Hemery, Lecoutre & Sais, ECAI 2004,
  <https://www.researchgate.net/publication/220838185_Boosting_Systematic_Search_by_Weighting_Constraints>).
- **SAT.** CDCL conflict analysis credits the minimal 1UIP cut, not the whole implication graph
  (Moskewicz et al., DAC 2001,
  <https://rg1-teaching.mpi-inf.mpg.de/advancedc-ws08/exercises/Chaff.pdf>).
- **Parsing.** HPSG "quick check" ranks feature paths by per-path failure rate — the closest domain
  analogue.
- **Profiling.** gprof's own manual documents why propagated inclusive time becomes unattributable
  across shared and recursive call structure (<https://sourceware.org/binutils/docs/gprof/Cycles.html>).

The strongest finding is empirical: the simplest immediate-cause counters have been the production
default in CSP and SAT for roughly twenty years, refined on normalization but never replaced.

Two honest gaps. Exact Shapley/Aumann-Shapley allocation is theoretically correct but exponential,
with no source found applying it at this scale. Subtree-size-weighted blame — which would catch a
rule whose fault is pure fan-out — has no precedent as a blame technique; `outputs` is this design's
answer to that gap, and `amplification` is labelled heuristic rather than principled.

## Normalization is post-processing

`dom-wdeg` divides by domain size; quick-check ranks by rate. Both are computable after the fact.
Nothing is normalized during collection: attempts per word, dead ends per attempt, amplification,
cohort lift. Storing raw counts is what keeps the ranking question re-openable without re-running.

## Time

No timer is read inside the search. `work × op_cost[kind].ns_per_unit`, where the constants come from
`pangloss calibrate`:

- **Corpus:** the conformance suite — broad, already maintained, already in-repo, and it cannot rot
  into unrepresentativeness the way a curated bad-word list would.
- **One core.** Measures the code path rather than the load; contention and bandwidth sharing are
  exactly the variance that would make a re-measurement disagree for no informative reason.
- **Estimator: Σns / Σwork per kind**, never the mean of per-item rates. The constant is a multiplier
  against work units, so it must be work-weighted; the conformance suite deliberately contains
  pathological fixtures, and one would distort a mean of rates badly.
- **Self time.** A morphological rule's timed region encloses the phonological rules it triggers.
  Timed naively, both constants inflate and every nested call is double-counted. The harness subtracts
  nested instrumented regions.
- **Committed to the repo as data with provenance** — version, CPU, date, and the fixtures used. A
  *change* in a constant is itself a finding: if phonological per-attempt cost triples between
  measurements, that is an engine regression visible in a diff.
- **Copied into the cache's `op_cost` rows** at run time, so an exported report carries the constants
  it was computed with and stays self-describing.

**The claim is relative.** A constant measured on one machine and applied on another is wrong in
absolute terms but right in ratio terms, and ratios are what a ranking needs. Single-core constants
also systematically under-state a threaded run; all kinds inflate together, so rankings survive. The
report header says so; nothing tries to correct it.

**Within a kind the constant cancels entirely** — ranking rule A against rule B is a pure `work`
comparison. Since reports are sectioned per kind, the default views are calibration-independent, and
the constant only matters across kinds, which is not a default view. The time column's real job is
human legibility, not ranking.

Two gates, for two different things: the **collector** is runtime-gated (`--stats`), while the
**calibration timers** are compile-time gated behind a Cargo feature and never exist in a normal
build. That is the only place real per-call timers appear anywhere in the design.

## Memoization: physical versus semantic

The memo replays stored results without entering the rule body (`stratum.rs:~650-700`). Memo hits are
deliberately not counted: the hunt is for what is slow, and a replay is not slow.

Consequence, accepted: `attempts`, `work`, `outputs`, and the three dead-end counters are *physical*;
`uses` is *semantic*. A replayed derivation contributes `uses` without physical counts, so `uses` can
exceed `outputs`. **The two families are never divided into each other.** `amplification` stays valid
because both its terms are physical.

### `uses` is written by one mechanism

Synthesis runs only for candidates that survived analysis, and 91-98% of the search dies before
reaching it (`rust/crates/pg-parse/src/morpher.rs:397-472`). So a scratch buffer — cleared per
candidate, reused, no allocation per candidate — accumulates the objects that fired during that
candidate's synthesis, and commits to the collector only if the candidate passes the gate.

The candidate `Word` carries `morphs` (allomorph + morpheme) and `mrule_apps`, so this one mechanism
writes `uses` for lexical entries, morphological rules, and phonological rules alike. Deriving `uses`
from `WordAnalysis` for some kinds and from synthesis for others would be two mechanisms drifting
apart on one column.

## Collector implementation

Gated: `--stats` off allocates nothing, and each instrumentation site is one perfectly-predicted
branch. The collector rides alongside `StepBudget`, already threaded to exactly these places, with the
same per-word lifetime and the same non-`Sync` `Cell` pattern — no locks, no atomics.

Two storage shapes, because one does not fit both:

- **dense** `Vec<u64>` per counter for rules and strata — dozens to hundreds of entries, a few KB per
  word.
- **sparse** map or insertion-ordered `Vec<(LexEntryId, Counters)>` for lexical entries. Dense would
  be 0.5-5 MB zeroed *per word* at 10^4-10^5 entries, and ~99.99% zeros, since the trie returns only
  a handful of matches per word.

Counter arrays must **not** live on `Word`: it already clones a `BTreeMap` per clone, and a
hot-cloned struct must not grow a vector.

Accepted hazard: gated instrumentation is not exercised by ordinary test runs, so it needs its own
tests or it rots.

### Determinism

Every counter is thread-count invariant. `RuleCache` is built once and is "compiled once and shared
read-only across `--threads=N` workers" (`rust/crates/pg-parse/src/morpher.rs:43-44`) — a
compiled-matcher cache carrying nothing between words — and the results memo (`AnalysisScope`) is
per-parse. So no word's counters can depend on whether another word ran first.

Since the per-object time column is `work ×` a frozen constant, **the entire report is reproducible
bit-for-bit across thread counts.** The only non-deterministic values in the design are the raw
`elapsed_ns` fields, which goldens exclude.

## The lexicon side: there is no mini-FST

HC-mode lexical lookup is a hand-built trie — a port of C# `RootAllomorphTrie` — one per stratum,
walked by feature unification (`rust/crates/pg-parse/src/root_trie.rs:1-45`). Deliberately not
`pg-fst`: a compiled `pg_fst` FSA carries a single `accept_id` and cannot express a trie whose every
leaf carries a distinct root id, and `pg-fst` is a frozen contract. Root guessing is likewise a direct
recursive node walk, outside `pg_fst` because C# itself does not route it through its own matcher —
"the Matcher doesn't preserve the unifications of the nodes"
(`rust/crates/pg-parse/src/guess.rs:1-30`).

Three cost sites, and no single lexical entry owns the expensive one:

| Cost site | Charged to |
|---|---|
| the trie walk | the stratum's **`root_index`** — the trie is shared, so no entry owns a walk |
| candidate materialization, one per (matched entry × allomorph) (`morpher.rs:474-493`) | the **lexical entry** |
| guessing | the **`guesser`** pseudo-object; the root is fabricated and `morpheme_ids` carries `u32::MAX` |

Trie walk cost is grammar-shape-dependent, not lexicon-size-dependent: a zero-phonological-feature
grammar reduces the arc predicate to `char_def` identity, while a feature-bearing grammar also
consults a `unifiable_cds` bitset closure whenever identity misses, widening the walk.

**A lexical entry's `attempts` means something different from a rule's.** The trie emits only matches,
so an entry's attempts are all successful matches, and `attempts` versus `uses` reads as "matched N
times, survived M times". Per-kind report sections must say so.

## Storage

**One cache per FieldWorks data path.** Location is user-data, in a directory named by a hash of the
canonical `.fwdata` path, with the full path recorded inside for diagnosis. Deliberately not
`ConfigurationSettings/PanGloss/`, which the prior storage decision reserved for small shared project
data that Chorus/FLExBridge will synchronize — a cache this size must never be synced. No split
between CLI and FieldWorks clients: an analysis is uniquely identified regardless of who asked for it,
and a split gains nothing.

**The cache accumulates.** A word already present is not recomputed. Words add up across runs, so
asking about more words is incremental.

**The grammar hash is the only destructive event.** On a mismatch the next `batch --stats` wipes and
starts fresh, and says so — a silent wipe would look like the accumulation feature failing. The hash
is over the `Snapshot` (`Snapshot::to_json()`), not the `.fwdata` bytes, so edits that cannot affect
parsing do not invalidate. Note this is not free: a `.fwdata` input imports in-memory straight to a
compiled grammar with no JSON written, so hashing costs one serialization pass per run.

**Options, engine, and counter-semantics version are recorded per word, not keyed on.** They change
what a counter means — `--step-cap 1000` yields different `attempts` and `not_applied` than uncapped
— so mixing them silently corrupts every `SUM`. But wiping on them would fight the accumulation
model, since a human exploring one bad word will try several caps. Recording them makes the hazard
*visible*: `pangloss stats` warns when a query spans mixed option sets and can filter to one. `run`
rows carry full build info for forensics.

The counter-semantics version is hand-maintained, bumped when counting semantics change, on the same
discipline as `schema_version`. It will occasionally be forgotten; the mitigation is that `run` rows
make that diagnosable after the fact, and the invariants below usually catch a semantics change
loudly at the next test run.

**`--cache <path>` overrides the location**, and then the caller owns lifetime, retention, and
concurrency. This is Motif's door: Motif manages multiple caches in its own folders, comparison
reports, grammar-change assessment, and concurrency. PanGloss's own cache management stays
deliberately dumb.

```sql
PRAGMA journal_mode = WAL;

CREATE TABLE run (
  run_id              INTEGER PRIMARY KEY,
  schema_version      INTEGER NOT NULL,
  counter_semantics   INTEGER NOT NULL,
  build_info          TEXT    NOT NULL,
  fwdata_path         TEXT    NOT NULL,
  grammar_hash        TEXT    NOT NULL,
  engine              TEXT    NOT NULL,
  options_hash        TEXT    NOT NULL,
  options_json        TEXT    NOT NULL,
  created_utc         TEXT    NOT NULL,
  word_count          INTEGER NOT NULL,
  total_elapsed_ns    INTEGER NOT NULL
);

CREATE TABLE object (
  object_id        INTEGER PRIMARY KEY,
  key              TEXT NOT NULL,   -- authored id, structural locator, or synthetic id
  -- morph_rule | phon_rule | lex_entry | root_index | guesser | overlay
  kind             TEXT NOT NULL,
  label            TEXT NOT NULL,   -- what a human looks for in FLEx
  identity_quality TEXT NOT NULL    -- authored | structural | synthetic
);
CREATE UNIQUE INDEX object_key_kind ON object(key, kind);

CREATE TABLE stratum (
  stratum_id INTEGER PRIMARY KEY,   -- 0 = not applicable
  key        TEXT,
  label      TEXT
);

CREATE TABLE allomorph (
  allomorph_id INTEGER PRIMARY KEY, -- 0 = NONE sentinel: cost belonging to no allomorph
  key          TEXT,
  label        TEXT
);

CREATE TABLE word (
  word_id       INTEGER PRIMARY KEY,
  run_id        INTEGER NOT NULL,   -- which run computed this word
  form          TEXT    NOT NULL,
  elapsed_ns    INTEGER NOT NULL,   -- actual, non-deterministic, excluded from goldens
  attempts      INTEGER NOT NULL,   -- == SUM(fact.attempts) over morph_rule kinds
  passes        INTEGER NOT NULL,   -- surviving analyses
  capped        INTEGER NOT NULL,
  timed_out     INTEGER NOT NULL,
  invalid_shape INTEGER NOT NULL
);
CREATE UNIQUE INDEX word_form ON word(form);

-- The fact table. Sparse: a row exists only where some counter is non-zero.
CREATE TABLE fact (
  word_id          INTEGER NOT NULL,
  object_id        INTEGER NOT NULL,
  stratum_id       INTEGER NOT NULL,
  allomorph_id     INTEGER NOT NULL,
  attempts         INTEGER NOT NULL,
  work             INTEGER NOT NULL,
  outputs          INTEGER NOT NULL,
  not_applied      INTEGER NOT NULL,
  no_root          INTEGER NOT NULL,
  surface_mismatch INTEGER NOT NULL,
  uses             INTEGER NOT NULL,
  PRIMARY KEY (word_id, object_id, stratum_id, allomorph_id)
) WITHOUT ROWID;

-- Frozen measured constants, copied in from the committed table.
CREATE TABLE op_cost (
  kind          TEXT PRIMARY KEY,
  ns_per_unit   INTEGER NOT NULL,   -- per segment touched
  provenance    TEXT NOT NULL       -- version, CPU, date, fixtures
);

-- Which counters this run could measure at all, so an unmeasurable column renders "—" not "0".
CREATE TABLE coverage (
  kind    TEXT NOT NULL,
  counter TEXT NOT NULL,
  state   TEXT NOT NULL,            -- measured | unsupported | censored
  PRIMARY KEY (kind, counter)
) WITHOUT ROWID;
```

`WITHOUT ROWID` with a composite primary key on `fact` is load-bearing, not tidiness: it removes the
rowid and its secondary index, the largest single space saving available.

Never write inside the parse. Accumulate the per-word structures, then flush in batched transactions.
Word rows use upsert semantics, since two runs can compute the same word concurrently.

### Size

A `--stats` run analyzes **10-100 words typically, 10,000 at the outside** — a diagnostic aimed at
suspect words, not a project sweep. The large number is attempts *within* one word (1,000,000 for a
single pathological word is the motivating case), not word count. At ~450 fact rows per word and ~35
bytes per varint-encoded row:

| Words in cache | `fact` rows | Approx. size |
|---|---|---|
| 100 | 45k | ~1.6MB |
| 1,000 | 450k | ~16MB |
| 10,000 | 4.5M | ~160MB |

Aggregation needs nothing beyond SQLite at this scale.

## Reports

Three defaults as shipped (originally two; **never-fires** was added after real-grammar use found
its absence the single most actionable gap — see "Known limitations as shipped"). All are
`GROUP BY` queries; per-kind sections, no top-N by default (object count is bounded by grammar size,
so print everything), `--top` for large grammars.

**1. Per word.** Form, **actual** elapsed, attempts, passes, capped/timed-out flags. Sorted by
elapsed descending. This is the entry point — find the bad word, then find the bad rule inside it —
and it is what FLEx's Run Tests already gives via a sortable Parse Time column, so users can already
read it.

**2. Per object.** Kind, label, attempts, **estimated** time, outputs, amplification, `Didn't apply`,
`No root found`, `Didn't match the word`, uses. Sorted by estimated time descending by default;
`--sort` switches to `no_root` for the over-application question directly. `Didn't apply` is
rule-level here — one count per invocation that produced nothing, at the same granularity as
`attempts` — never the sum of every allomorph's own failure; see "Known limitations as shipped" for
why that distinction had to be made explicit.

**3. Never-fires** (`--group never-fires`, `pg_stats::never_fires_report`). Objects (scoped to
`morph_rule`/`phon_rule`, the only kinds with a wired `outputs` counter) attempted at least
`NEVER_FIRES_DEFAULT_MIN_ATTEMPTS` (1000) times in one direction that produced zero outputs there,
ordered by attempts descending. Direction-aware by construction — the query groups by
`(object, direction)`, so a rule dead in analysis but live in synthesis contributes only its analysis
row. Included by default whenever the cache holds any such row, printed after the per-object report;
`--min-attempts` overrides the floor explicitly (0 admits everything, which is the point of *not*
defaulting there).

**Actual and estimated time never appear in the same table.** They will disagree — the constants are
measured elsewhere on one core — and a user seeing "estimated 2s, actual 6s" rightly distrusts the
estimate, which is the column doing the real work. So actual belongs to the per-word report and
estimated to the per-object report, and the per-object report carries a one-line header stating its
times are estimates that **will not sum to the actual total**: constants are approximate, and time
spent outside instrumented objects is not attributed at all. Without that line, "my rules add up to
40s but the run took 90s" reads as a missing-time bug rather than as a ranking tool.

An unmeasurable column renders **—**, never **0**, driven by the `coverage` table. This matters most
in foma mode, where the proposer replaces HC's analysis search, so `no_root` is zero on every row —
not because the grammar is clean, but because that phase never ran. Same rule this repo states
elsewhere: *"I could not look" must never read as "everything is fine."*

**`work` is opt-in** (`--show-work`), on every report that carries it, rather than a default column
or absent entirely — see "Known limitations as shipped" for why its basis is provisional.

## Filtering

**Batch side** — which words to analyze: a word list, or project scope. Feedback filtering (re-run
the worst) is mostly unnecessary because the cache accumulates: those words are already there, so the
question is a query, not a re-run.

**Aggregation side** — flags for the common cases (`--kind`, `--stratum`, `--object`,
`--min-attempts`, `--top`, `--sort`, `--exclude-capped`) plus **the SQLite schema documented as a
public escape hatch**, so a power user or an AI writes its own query instead of waiting for a flag.
Consequence accepted deliberately: once documented, `schema_version` is a compatibility promise, not
a note.

## Invariants

These are the defence against a silently wrong report, and they must exist before the instrumentation
sites do:

- `SUM(fact.attempts)` over `morph_rule` kinds == `word.attempts` == the engine's `steps`
- allomorph rows sum to their rule's row, per word — catches a missed allomorph loop
- counters identical across `--threads N` for any N

## Ceilings

**Interactions are out of scope.** The fact table records *that* an object participated in a word,
never *that two objects were on the same path*. "This rule is only expensive when it follows that
rule" is unanswerable. This follows from aggregating per word, which is what keeps the cache at
megabytes; answering it needs path-level records, the 10^6-10^8 row design deliberately rejected.

**Run-to-run comparison is Motif's job**, or a human's via `--cache`. The default cache holds one
grammar's data and is wiped when the grammar changes.

**This measures grammar shape, not FST cost.** A rule that over-applies while peeling may compile
into a perfectly cheap FST, or may blow up the network instead. Over-generation is a property of the
grammar and shows up either way, which is why this is a useful grammar-tuning instrument — but
`fst-health` and `make-report` remain the FST-cost instruments.

## Do not trust `estimated_time_ms`. Measured, it is wrong by ~180x.

The constants are calibrated over the conformance suite, which is synthetic by hard rule and whose
patterns are trivial next to a real grammar's. Measured against real corpora the gap is not marginal:

- Amharic, 673 words: summing every per-rule `estimated_time_ms` accounts for a couple of seconds
  against a **1369.7 second** run. The model explains roughly 0.2% of wall clock.
- Per attempt: ~1.7ms actually elapsed against ~9.6µs predicted, about **180x**.

Two independent causes. The `work` unit charges a rule the full segment count of the candidate shape,
while the dominant event is a *failed* match touching an unpredictable prefix. And per-kind constants
do not discriminate — `morph_rule` 471.1, `phon_rule` 457.2, `lex_entry` 438.9 ns per unit, a 7%
spread — so the whole per-kind dimension collapses to roughly one scalar.

**Rank by the deterministic counters** (`attempts`, `outputs`, `not_applied`, `no_root`, `uses`), which
are exact and reproducible. Where real time is needed, use the feature-gated phase instrumentation,
which attributes 98.6% of wall clock on Amharic and ~100% on Indonesian. Every published grammar
finding to date rests on counters and phase times, never on this column.

## What the phase instrumentation found, and why it matters to this design

Two buckets dominate every grammar measured, and neither is visible in the counter-based reports:

| | Amharic (673 words) | Indonesian (121 words) |
|---|---|---|
| compounding analysis | 52.6% | 60.8% |
| affix pattern traversal | 41.5% | 33.0% |
| everything else | 5.9% | 6.2% |

This is a caution about the whole counter model, not a footnote. Compounding is entered *rarely* —
42,807 times against 1,480,923 affix-allomorph attempts in Amharic, about 2.9% — and costs more than
all of them combined. **A report ranked by `attempts` therefore points at the wrong thing**, because
attempts and cost are not proportional across kinds. Indonesian is the control that proves the point
is about cost rather than share: compounding is 60.8% of its time too, and the grammar is fast
(1.28s for 121 words). The discriminator is per-call cost — 0.6ms in Indonesian against 17.2ms in
Amharic.

## Known limitations as shipped

The design above describes the intended contract. These are the places the first implementation
falls short of it, found by adversarial review rather than left implicit. Five items below were
found by running the shipped v1 feature against real grammars (Sena, Amharic, Aweti) and are marked
**fixed** with what changed; the rest remain open.

**FIXED — `work` was recorded but unreachable.** No report surfaced it, so it existed only as an
input to `estimated_time_ms`. Now `--show-work` appends it as an opt-in column on every report that
carries it (object, allomorph, stratum, direction); the default view still omits it. This does not
fix the counter's own weighting problem: it still charges a rule the full segment count of the
candidate shape, while the event that dominates every measured corpus is a *failed* match, which
touches an unpredictable prefix and often stops at the first segment — so it still systematically
over-charges fast failures and under-charges a rule that scans a whole shape before failing. That
basis is **provisional pending a concurrent measurement effort** into what actually drives per-attempt
cost (an Amharic run showed the current model explaining roughly 0.23% of wall clock); a per-object
weight model is deliberately not built on top of an unmeasured basis. Making the counter observable,
without pretending its weighting is settled, is what shipped.

**FIXED — per-kind calibration constants did not discriminate.** Measured: `morph_rule` 471.1,
`phon_rule` 457.2, and `lex_entry` 438.9 ns per unit sit within 7% of each other; only `root_index`
(83.9) was distinct. `pangloss calibrate` now collapses every kind into one of two `CostBucket`s —
`default` (`morph_rule`, `phon_rule`, `lex_entry`, `guesser`, `overlay`) and `root_index` (its own
bucket) — so `morph_rule`/`phon_rule`/`lex_entry` report the *same* constant (their bucket's
Σns/Σwork), and the collapse is stated in `Provenance.calibration_model` in the committed
`stats_op_cost.json`. `work_observed` stays per-kind for diagnosability, and a kind with zero of its
own instrumented work (`guesser`, `overlay`) stays unmeasured regardless of its bucket's total — the
collapse shares a *constant*, never manufactures evidence a kind doesn't have. A genuine per-object
cost model (pattern length, variable count, determinism) remains future work, gated on the concurrent
per-attempt-cost measurement mentioned above.

**FIXED — a "never fires" pattern had no dedicated report.** The single most actionable fact found
scanning three real grammars by eye was "this rule is entered hundreds of thousands of times and
produces nothing" (Sena: four rules, 2.37M entries, zero outputs; Aweti: five affixes, ~1.56M each).
`--group never-fires` (and inclusion in the default view whenever the cache holds a qualifying row)
now surfaces this directly — see "Reports".

**FIXED — the per-object report mixed two different countable units under `Didn't apply`.** Summing
`fact.not_applied` across every allomorph row alongside the rule-level residual meant a two-allomorph
rule failing both ways booked 2 against 1 `attempts` — arithmetically defensible, but Sena showed
635,762 attempts against 1,271,524 "didn't apply" and it reads as broken. The per-object report now
sums `not_applied` only over the rule's own `allomorph_id = 0` row — one count per invocation that
produced nothing, matching `attempts`'s granularity — via a new collector-side counter
(`record_mrule_invocation_not_applied`) that ticks once when an invocation reaches one or more
allomorphs but none of them produce output. The per-allomorph report is unchanged: it still shows
each allomorph's own failure count, which is the right place for that detail.

**MEASURED, not fixed — `--stats` overhead.** "Must not add appreciable time to parsing" was a
stated requirement with no evidence behind it; see this file's overhead measurement (added
alongside the four fixes above) for the number and the recommendation that follows from it.

**Phonological rules get no `uses` and no `no_root`.** `Word` carries `mrule_apps` but no
`PRuleId` trail, and growing `Word` is forbidden (it already clones a `BTreeMap` per clone). So
commit-on-pass cannot attribute a surviving analysis to a rewrite rule, and `no_root` is charged only
to the last *morphological* rule. `WIRED_COUNTERS` is honest about both, so those cells render `—`
rather than a misleading zero.

**`--engine=foma --stats` records word-level rows only.** The foma confirm path has no collector
hook, so no per-object facts exist in that mode; every counter is marked `unsupported`. Accepting
foma was a product decision, and what ships satisfies it only nominally.

**`uses` counts once per surviving analysis, not once per word.** A word with several ambiguous
parses that all use one rule increments that rule's `uses` several times. The spec wording admits
either reading; this is the one that shipped.

**The traced-plus-stats combination is unreachable.** The only production constructor of a collector
passes a no-op trace sink, so the hand-duplicated recording sites in the traced siblings never run
together with stats. A divergence between a traced site and its untraced twin would not surface
today.

**"Stats off allocates nothing" is unverified.** Inspection supports it — the collector is
constructed in exactly one entry point and every other path threads `None` — but no test pins it.

## `--stats` overhead, measured

_Placeholder pending the measurement run — see the implementation report for this change for the
actual numbers and recommendation once filled in._

## What this refuses to say

An object's counts never license an automatic grammar edit. This report localizes magnitude and names
the object; it does not certify that a narrower grammar remains linguistically correct, and it must
not rank a grammar as better or worse. An object absent from a run has no evidence recorded against
it — never proof it is dead, and possibly only gated out.
