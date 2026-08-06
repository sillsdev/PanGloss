# `typology_speedup` — per-construct engine timing harness

`rust/crates/pg-foma/tests/typology_speedup.rs` times every word in the synthetic-language
conformance suite against both engines, grouped so speedup is attributable per construct/typology
rather than as a single aggregate. It writes a CSV (the canonical data) plus a rendered Markdown
table (a view over it).

## Why this lives in `pg-foma/tests/`, not `pg-parse/tests/`

The harness needs both engines: `pg_parse::Morpher` (the complete engine) and
`pg_foma::composite::FomaAnalyzer` plus `pg_foma::capability_entry::evaluate_capability` (the
compiled path and its capability gate). `pg-foma` already depends on `pg-parse` normally; putting
this harness in `pg-parse/tests/` would require adding `pg-foma` as a new dev-dependency of
`pg-parse` — a reversed layering edge, since `pg-foma` is downstream of `pg-parse`, not the other
way around.

## Driving both engines in-process, not via the CLI

This harness calls `Morpher::parse_word` and `FomaAnalyzer::analyze_word` directly rather than
shelling out to a CLI batch command, for two reasons: (1) a CLI's elapsed-time column is typically
integer milliseconds, which is exactly the floor this harness needs to get under — driving
`Instant`/`Duration` at the measurement site gets nanosecond resolution for free; (2) shelling out
would create a build dependency on a CLI binary for no benefit, since nothing here needs one.

## The measurement floor: never emit `0` for a fast word

`measure_timer_floor_ns` calibrates this process's actual `Instant` tick granularity once — some
platforms/virtualized CI runners have coarser clocks than a bare-metal workstation, so a hardcoded
constant would be wrong somewhere. Any measured value at or below that calibrated floor is reported
as below-floor, never as a literal `0`: `Row::timed` stores `None`, not `Some(0)`, for a field once
its value is below the floor, and `format_ns_cell` renders that `None` as `<{floor}ns` in both the
CSV and the Markdown table.

## Refusal as its own outcome, not an edge case

Before ever calling `FomaAnalyzer::new` (which would force-compile), each fixture's grammar is
evaluated once via the same `evaluate_capability` entry point the production capability gate uses.
A `Refuse` verdict is recorded as its own fixture-level outcome row per diagnostic, naming the
refusing predicate, construct, and witness — never a zero time and never a dropped row. This harness
never force-compiles a refused grammar: publishing a force-compiled number for a permanently
carved-out construct would invite exactly the kind of over-reading a refusal-outcome exists to
prevent, and the refusal itself is the interesting result for those fixtures.

## Grouping and noise

The CSV is per-word; the Markdown table aggregates per fixture, which is also per
construct/typology, since fixtures are named by construct or typology, one grammar per fixture.
Aggregation uses the median of each word's own median (median-of-medians, not a mean), so one
slow/fast outlier word cannot swing a whole fixture's reported speed. `fixture_is_noisy` flags a
fixture's speedup as unreliable — rather than silently reporting a precise-looking ratio computed
from noise — if either (a) the aggregated value on either engine sits within 20x of the calibrated
timer floor (too close to the clock's own resolution limit for a ratio to mean anything), or (b) any
word's own repeat spread (max/min across timed samples) exceeds 3x (the repeats did not agree
closely enough to trust their median). These are deliberately simple, judgment-call thresholds.
These fixtures are tiny synthetic grammars (single digits to ~55 words); both conditions are
expected to fire often, which is the honest result at this scale, not a harness bug.

## A real bug this harness found

While first running this harness over the full corpus, a multi-`CharacterDefinitionTable` fixture
crashed `FomaAnalyzer::new` with an out-of-bounds index inside the compiled path's multi-table
handling. That is a real, pre-existing bug outside this harness's own scope (it owns measurement,
not the emitter), and exactly the kind of thing a full-corpus measurement run exists to surface. A
harness over dozens of independently-authored synthetic pathology fixtures cannot assume every
engine call succeeds or even returns cleanly, so each engine stage is independently panic-guarded in
`process_fixture`: a bug triggered by one engine on one fixture must not discard the other engine's
already-valid rows for the same fixture, and must never abort the whole corpus run.

## A real bug the Markdown-table gate caught

`render_fixture_table_row` once emitted a leading `root:category` cell that the table header did
not declare, so every rendered table was misaligned by one column. Markdown silently tolerates a
row with more cells than its header — it renders, just wrong — which is why the bug produced
plausible-looking output instead of failing outright, and why `markdown_header_matches_row_cell_count`
has to be a mechanical cell-count check rather than something caught by reading the file.
