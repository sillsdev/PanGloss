# pg-foma cover_compounding_recursive_depth_bound.rs: design notes moved out of comments

Longer arguments pulled out of
`rust/crates/pg-foma/tests/cover_compounding_recursive_depth_bound.rs` implementation comments so
the source can carry a one- or two-line pointer instead of the full argument. Each section
corresponds to one call site; the site names the function/type so this doc can be found from either
direction.

## Module doc: the depth-bound and the depth-budgeted construction

`crate::capability::compounding_max_depth` (`CompoundingDetail::max_depth`) turns the existing
boolean `recursive` flag into an exact, always-finite stem-count bound. `crate::emit`'s "bounded
compound loop" no longer hardcodes exactly one extra root: `build_compound_chain` unrolls
`max_depth - 1` extra (non-head) root levels, consuming this predicate's own precomputed bound
directly (one source of truth), and `crate::capability::CompoundingRecursionSafePredicate` now
reaches `ConfirmOnly` unconditionally for every observed `Compounding` rule, recursive or not.
Growth is checked eagerly, before any lexc text is written, against
`crate::emit::DEFAULT_COMPOUND_CHAIN_DEPTH_BUDGET` — a genuinely oversized `multipleApplication`
value gets a typed `FomaTier::Unsupported` outcome, not a hang or an OOM.

Fixture: `conformance-staging/edge-cases/recursive-endocentric-compounding` (reused as-is, not
duplicated). Its own `STAGING.md`/`grammar.xml` document the shape: `cr1 multipleApplication="9"`,
so `compounding_max_depth` bounds it at exactly 10 stems. The depth-bound and budget tests use small,
self-contained inline fixtures instead, since they need specific, small, exactly-controlled depth
numbers the staged fixture's own `multipleApplication="9"` does not give them.

## `max_depth_cannot_distinguish_four_ways_to_compound_from_four_levels_of_nesting`: the rule-count-versus-depth conflation

`compounding_max_depth` sums `max_apps` across the transitive closure of rules that could feed a
rule. That makes it blind to a distinction which decides how big a construction gets:

- one rule at `multipleApplication="4"` — a rule that genuinely may re-apply to its own output four
  times, so a single derivation really can reach five stems;
- four rules at the DTD default `multipleApplication="1"` — four alternative ways to compound, none
  of which may apply twice at all.

Both compute `max_depth == 5`. The formula cannot tell them apart, because the first quantity is
typology and the second is grammar-counting. Eight ways to compound is not nine levels of nesting,
and this test is that sentence as an executable collision — it needs no FST, no corpus and no
emitter, so nothing else can confound it.

That multiplier is live, not hypothetical: the private `sena` grammar declares 8
`CompoundingRule`s, none with `multipleApplication`, so it lands on `max_depth = 9` and
`crate::emit::compound_extra_levels_checked` unrolls 8 non-head root levels for it.

The operative bound is much smaller and lives elsewhere entirely: C#'s `Morpher.MaxStemCount` (ctor
default 2), ported as `pg_rules::stratum::AnalyzerConfig::max_stem_count`, gates `Compounding` rule
application as soon as `non_heads.len() + 1 >= max_stem_count`, so a default-configured engine
confirms at most two stems.
`raised_cap_oracle_finds_the_recursive_analysis_confirm_at_default_would_miss` in this same file
pins that half against the staged fixture: at the default cap the 3-stem compound confirms zero
analyses, and only `with_max_stem_count(3)` makes it one.

This test is deliberately a pin, not a behavior change. The over-approximating direction is sound,
and the deeper levels are real recall for a raised-cap caller —
`depth_budgeted_compound_loop_contains_the_raised_cap_oracle_analysis` would break if the
construction were simply clamped to the default. Sizing the construction by
`min(ceiling, operative stem bound)` would require the operative bound to travel from whoever sets
`max_stem_count` into `crate::emit`, with this test moving in lockstep; see
`crate::capability::compounding_max_depth`'s own doc for the full write-up. What this test
guarantees meanwhile is that the conflation is asserted, so it cannot be quietly re-argued away.
