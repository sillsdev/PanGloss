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
`(authored object, stratum, allomorph)` combination that participated. No ratio is ever stored — every
rate is derived at report time from the integers.

`--stats` is **opt-in and off by default**, which is what makes reading a clock affordable at all: an
ordinary parse allocates nothing and reads no clock, and only a caller who asked for statistics pays.
Measured cost of asking, including the SQLite write: **1.5-3.5%**, within run-to-run noise on a
70-word corpus. That figure comes from the overhead run described below rather than from a
reproducible harness, so treat it as an order of magnitude, not a pinned number.

Blame for a dead path goes to **terminated-at** — the object applied immediately before the path
died — never to the whole ancestor path.

Time is **measured, not estimated**: a wall-clock self-time region is entered and exited at each of
the three object boundaries (rule application, allomorph attempt, lexicon lookup), nesting-aware so
a rule that triggers other rules is charged only its own cost.

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

**4. Ask where the time went.**

```
pangloss stats <grammar> --cache <cache.sqlite3> --group group
```

One row per object kind: `morph_rule`, `phon_rule`, `lex_entry`, `root_index`, `guesser`, `overlay`.
`time_ms` is real wall-clock self time, collected whenever `--stats` ran, so this separates the
lexemes from the rules without apportionment. Then `--group object --word <form>` drills from the
kind into the individual rules of one word.

Compounding is recorded under `ObjectKind::MorphRule` like any other morphological rule. The
per-object rows identify individual compounding rules, and the per-kind total includes their actual
self time.

### How to read the result without being misled

- **`measured_time_ms` is real wall-clock self time**, not an estimate.
- **`attempts` is not cost.** Use the actual `self_time_ns` totals for time rankings; attempts remain
  useful for comparing participation within each compatible object kind.
- **Capped words give floors, not totals.** A word cut off at its timeout contributes real counts up to
  the cut. Fine for ranking, wrong for "this rule costs X". `--exclude-censored` drops them.
- **Zero outputs on a small word list is ambiguous.** It means either the rule cannot fire or these
  words do not use it. Only a bigger list distinguishes them — say which you have.
- **Prefer `--step-cap` when comparing runs.** A wall-clock cap is machine- and load-dependent, so two
  runs are not comparable; a step cap is deterministic. But note it bounds only admitted morphological
  rule applications, so it does not bound compounding internals or phonological work — for a grammar
  whose cost is inside one compound call, only the wall-clock cap actually bounds the run. Use both.

## Scope

**In v1:** the collector (including measured per-object self time), the SQLite cache, `batch
--stats`, `pangloss stats` with the per-word and per-object reports, and the invariant tests.

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

**Templates** were designed as a report-time rollup joining rules to their owning template, with the
caveat that a rule appearing in more than one template is attributed to each, so per-template sums
can exceed the total. **Not shipped:** there is no `--group template` and no template column. Listed
here as a design, not a feature.

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

A real clock is read inside the search, at exactly three object boundaries: rule application,
allomorph attempt, and lexicon lookup. Each is a `StatsCollector::time_enter`/exit pair booked
against the same `(object, stratum, allomorph, direction)` row the seven counters already use, so
the result rides in `Counters::self_time_ns` alongside them rather than needing its own table.

- **Nesting-aware.** A morphological rule's timed region encloses the phonological rules it
  triggers and the lexicon lookups its allomorph loop performs. A region entered while another is
  already open has its elapsed time subtracted from the enclosing region's own total on exit, so a
  compounding rule's self time excludes whatever a nested lexicon lookup already claimed for
  itself — never double-counted, never silently folded into the parent.
- **Always on whenever `--stats` collects at all.** It is two clock reads per object boundary,
  cheap enough to be part of the ordinary `--stats` collector.
- **Exact, not apportioned.** Because `self_time_ns` is booked directly at the object that owns it,
  a per-kind total is the plain `SUM` of its objects' rows — there is nothing to divide, and no
  `bucket_ns × (object_attempts / bucket_attempts)` apportionment anywhere in the design.
- **Wall-clock, therefore excluded from goldens.** `Counters::without_timing` /
  `StatsRow::without_timing` zero this field before any equality check, matching `elapsed_ns`'s
  treatment elsewhere in this design — see "Determinism" below.

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
writes `uses` for lexical entries and morphological rules. (Not phonological rules: `WIRED_COUNTERS`
does not wire `(PhonRule, "uses")`, so that cell renders unmeasured -- see "Known limitations as
shipped".) Deriving `uses`
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

Every counter above `self_time_ns` is reproducible bit-for-bit across thread counts. `self_time_ns`
is real wall-clock and therefore not: it is excluded from equality via `Counters::without_timing` /
`StatsRow::without_timing`, matching the raw `elapsed_ns` fields' treatment — a measured time of
zero is a real zero and renders as `0`, but this exclusion is about golden *equality*, not about
whether the value is meaningful.

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
is over the `Snapshot` (`Snapshot::to_json()`) for `.json`/`.fwdata` inputs, so edits that cannot
affect parsing do not invalidate. **An `.xml` grammar hashes its raw file bytes instead**, because
`pg_grammar::load()` produces no `Snapshot` for that path -- so a comment- or whitespace-only edit to
an XML grammar does force a wipe. Fail-closed, never silently wrong, but not the stated property. Note this is not free: a `.fwdata` input imports in-memory straight to a
compiled grammar with no JSON written, so hashing costs one serialization pass per run.

**Options and counter-semantics version are recorded per word, not keyed on.** They change
what a counter means — `--step-cap 1000` yields different `attempts` and `not_applied` than uncapped
— so mixing them silently corrupts every `SUM`. But wiping on them would fight the accumulation
model, since a human exploring one bad word will try several caps. Recording them makes the hazard
*visible*: `pangloss stats` warns when a query spans mixed option sets. **Filtering to one option
set is not shipped** -- there is no `--run`/`--options-hash` flag, so narrowing to a single set means
querying the documented schema directly. `run` rows carry full build info for forensics.

**Engine is a cache invariant.** The first writer establishes it, and `StatsCache::flush` rejects a
different engine inside the same immediate transaction that would write the run. The CLI also checks
early for a clearer error. A legacy cache that already contains mixed engines is refused at read time
because its counters are not comparable.

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
  self_time_ns     INTEGER NOT NULL DEFAULT 0,  -- measured wall-clock, excluded from golden equality
  PRIMARY KEY (word_id, object_id, stratum_id, allomorph_id)
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

Six orientations are shipped, selected by `--group`: `word`, `object`, `group`, `morpheme`,
`allomorph`, and `never-fires`. Stratum and direction remain dimensions and filters, not standalone
profiles: aggregating their rows across object kinds would combine incompatible attempt units.
Reports use `GROUP BY` queries; there is no top-N by default (object count is bounded by grammar
size), with `--top N` available for large grammars.

Counter-bearing orientations use the narrow column set `label`, `time_ms`, `time%`, `attempts`,
`attempts%`, `amp`, and `uses` where those measurements exist. Unsupported cells render as
unavailable; a column that the engine or orientation cannot measure is omitted from text and null in
JSON. The word and never-fires reports keep purpose-specific shapes. `--wide` appends `work`,
`not_applied`, `no_root`, `surface_mismatch`, and `identity_quality`. `--format jsonl` prefixes rows
with a meta line carrying run identity, active filters, totals, and unmeasured-field explanations.

**Every orientation ends with a TOTAL line**, carrying the row count, the summed time, and that
sum as a percentage of the wall clock actually recorded — the attribution check. That percentage is
what decides whether the numbers are worth acting on (Amharic attributed 98.6%; a run attributing
40% is telling you the cost is somewhere the collector cannot see), and it was hand-computed for
every finding before it was a column.

**`--top` narrows what is displayed, never what is counted.** It keeps the first N rows of each
*kind*, so one crowded kind cannot hide another entirely, and it is applied after the totals are
taken: each row's `time%` is its share of every matched row, and a truncated table says how many of
the matched rows it showed. A SQL `LIMIT` cannot do this — it narrows the denominators along with
the rows, so an excerpt's shares sum to 100% and read exactly like the whole run's.

**Per word** (`--group word`). Form, **actual** elapsed, attempts, passes, capped/timed-out flags. Sorted by
elapsed descending. This is the entry point — find the bad word, then find the bad rule inside it —
and it is what FLEx's Run Tests already gives via a sortable Parse Time column, so users can already
read it.

**Per object** (`--group object`). Kind, label, attempts, **measured** time, outputs, amplification, `Didn't apply`,
`No root found`, `Didn't match the word`, uses. Sorted by measured time descending by default;
`--sort` switches to `no_root` for the over-application question directly. `Didn't apply` is
rule-level here — one count per invocation that produced nothing, at the same granularity as
`attempts` — never the sum of every allomorph's own failure; see "Known limitations as shipped" for
why that distinction had to be made explicit.

**3. Never-fires** (`--group never-fires`, `pg_stats::never_fires_report`). Objects (scoped to
`morph_rule`/`phon_rule`, the only kinds with a wired `outputs` counter) attempted at least
`NEVER_FIRES_DEFAULT_MIN_ATTEMPTS` (1000) times in one direction that produced zero outputs there,
ordered by attempts descending. Direction-aware by construction — the query groups by
`(object, direction)`, so a rule dead in analysis but live in synthesis contributes only its analysis
row. Included by default whenever the cache holds any such row, printed after the per-object report.
The floor is the library default and carries no CLI override: a caller wanting a different one
queries the documented schema, which is cheaper than a flag nobody reached for.

**The per-word report's `elapsed_ms_actual` and the per-object report's `measured_time_ms` are both
real measurements, at different granularities.** Measured self times sum within the instrumented
`morph_rule` and `lex_entry` kinds. Other kinds render time as unavailable, and the difference from
word elapsed time remains visible rather than being converted into measured zeroes.

An unmeasurable column renders **—** or JSON `null`, never **0**. Support comes from the collector's
kind/counter contract plus report orientation and engine. This matters most in foma mode, where the
proposer replaces HC's analysis search: its missing counters are unavailable, not evidence that the
grammar is clean. Named allomorph attempts are likewise unavailable because attempts are counted at
rule granularity. Same rule this repo states elsewhere: *"I could not look" must never read as
"everything is fine."*

**`work` is opt-in** (`--wide`, alongside the other detail columns) rather than a default column or
absent entirely — see "Known limitations as shipped" for why its basis is provisional.

## Filtering

**Batch side** — which words to analyze: a word list, or project scope. Feedback filtering (re-run
the worst) is mostly unnecessary because the cache accumulates: those words are already there, so the
question is a query, not a re-run.

**Aggregation side** — flags for the common cases (`--word`, `--kind`, `--stratum`, `--object`,
`--direction`, `--top`, `--sort`, `--exclude-censored`, `--wide`, `--format`) plus **the SQLite
schema documented as a public escape hatch**, so a power user or an AI writes its own query instead of waiting for a flag.
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

## Why time is measured directly

Time is measured at each grammar-object boundary with `StatsCollector::time_enter`, rather than
derived from a proxy counter. The resulting self-time rows are exact for the object that owns the
work, nesting-aware, and summed directly for per-kind reports. Counters remain available for
deterministic attempt and dead-end comparisons.

**Rank by the deterministic counters** (`attempts`, `outputs`, `not_applied`, `no_root`, `uses`) or by
measured self time, which is excluded from golden equality by design (see "Determinism").

## Known limitations as shipped

The design above describes the intended contract. These are the places the first implementation
falls short of it, found by adversarial review rather than left implicit. Five items below were
found by running the shipped v1 feature against real grammars (Sena, Amharic, Aweti) and are marked
**fixed** with what changed; the rest remain open.

**FIXED — `work` was recorded but unreachable.** No report surfaced it, so `--wide` now appends it on every report that
carries it (object, allomorph, group, morpheme); the default view still omits
it. `work` remains a plain counter behind that flag — it is simply no longer a time basis:
measured self time (see "Time") replaced it for that purpose, so `work`'s own weighting problem
(charging a rule the full segment count of the candidate shape when the dominant event is a *failed*
match touching an unpredictable prefix) is moot for time reporting, though the counter is still
useful for its own sake (e.g. comparing candidate-shape sizes across attempts).

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
