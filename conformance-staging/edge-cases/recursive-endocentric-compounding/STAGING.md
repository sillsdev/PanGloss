# STAGING: recursive-endocentric-compounding

## Why this fixture exists

`docs/conformance/representative-typology-basis.md` S1.2.1 identifies recursive/self-feeding
endocentric compounding as a genuine gap: no fixture anywhere exercises a `CompoundingRuleDef` whose
own output part-of-speech re-enters its own input part-of-speech set (unlike
`conformance-staging/edge-cases/compounding-non-recursive`, which deliberately keeps input/output PoS
disjoint and stays capped at `multipleApplication`'s DTD default of 1). This fixture pins:

1. **The structural characterization.** `pg-foma::capability::compounding_recursive`'s rule-graph
   reachability pass marks a `CompoundingRuleDef` "recursive" purely from `max_apps > 1` (this
   fixture's `cr1` declares `multipleApplication="9"`) -- a fact about the RULE DEFINITION, true
   regardless of whether any actual word in this fixture exercises 3+ levels of nesting.
2. **The capability gate's honest, unconditional Refuse.** `CompoundingRecursionSafePredicate`
   (`compounding.non-recursive`) Refuses this grammar via `evaluate_capability`/`compose_envelope`,
   citing `compounding.recursive` -- `crate::emit::compound_license`'s license-gated propose shape
   is only proven for the non-recursive case (design.md D2 item 3's own scope cut).
3. **An independently-discovered oracle resource ceiling.** `pg_parse::Morpher::parse_word_opts`
   hardcodes `AnalyzerConfig::max_stem_count = 2` (mirroring C#'s `Morpher.MaxStemCount` default,
   `rust/crates/pg-rules/src/stratum.rs`'s own doc: "at most one non-head may ever be split off, so a
   word can be de-compounded once, never recursively re-compounded"). This means the STANDARD oracle
   this fixture's own ground truth is drawn from ALSO cannot confirm a genuinely 3-stem self-feeding
   compound today, entirely independently of the FST capability gate's own Refuse verdict. Both
   layers currently refuse the same construct, for different, independently-verifiable reasons -- see
   `words.yaml`'s `tevimaflisra` entry for the empirical finding.

## What it pins

- `tevi`/`mafl`/`isra`: three plain bare-root controls, proving ordinary lookup is unaffected.
- `tevimafl`/`maflisra`: two depth-1 (single-application) compounds, proving the self-feeding-CAPABLE
  rule shape (`headPartsOfSpeech`/`nonHeadPartsOfSpeech`/`outputPartOfSpeech` all = `posRoot`) still
  works completely normally at the non-recursive depth `compounding-non-recursive` already covers.
- `tevimaflisra`: **`expect_fail: true`** -- the load-bearing recursive/self-feeding witness. Zero
  analyses from `pg_parse::Morpher`, for the resource-ceiling reason above -- NOT hand-asserted, see
  "Verification" below.

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Authored fresh for
this task; `words.yaml` signatures captured by driving `pg_parse::Morpher::parse_word_opts` directly
over every word (a throwaway test, deleted after transcription -- see "Verification").

## Verification

Signatures were captured via a throwaway test (`rust/crates/pg-foma/tests/zz_throwaway_sig_dump.rs`,
deleted after transcription) driving `pg_parse::Morpher::parse_word_opts` directly over every word in
`words.yaml`, using the SAME grammar this directory's `grammar.xml` ships. The `tevimaflisra` = zero
analyses finding was cross-checked against `rust/crates/pg-rules/src/stratum.rs`'s own
`AnalyzerConfig::max_stem_count` field doc, which independently confirms the mechanism (a `>= 2`
non-head-count gate on any further compounding-rule application) -- not merely an unexplained empirical
zero. Cross-checked in-repo by `rust/crates/pg-parse/tests/conformance_fixtures_gate.rs`'s
`all_discovered_fixtures_match_oracle` test (dual-root discovery, default `cargo test --workspace`
suite) -- that test is what actually gates CI. The capability-gate Refuse verdict (structural, word-
independent) is additionally pinned directly by `rust/crates/pg-foma/tests/
cover_recursive_endocentric_compounding.rs`, which asserts `evaluate_capability` returns
`CompileDecision::Refuse` naming `Compounding`/`compounding.recursive`, and separately re-asserts the
oracle's own `tevimaflisra` = zero-analyses finding as an explicit regression gate -- this is the test
that should FAIL (prompting deliberate review, not silent staleness) the day either layer is promoted
to actually support this construct.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/recursive-endocentric-compounding/`. On acceptance, delete this staged
copy in the same change (graduation guard enforces this mechanically).

## G11 addendum (2026-07-25)

The `AnalyzerConfig::max_stem_count = 2` ceiling described above turned out to be a **faithful
default, not a permanent wall**: C#'s own `Morpher.MaxStemCount` (`Morpher.cs:72`) is a settable
per-instance property (ctor default `2`, `Morpher.cs:56`) that C#'s own
`CompoundingRuleTests.SimpleRules` (cs:87,105) raises to `3` to confirm a genuine 3-root compound.
`pg_parse::Morpher` previously hardcoded `2` with no way for any caller to raise it -- that missing
knob (not the value `2` itself) was the actual gap; see `pg-parse/src/morpher.rs`'s
`Morpher::with_max_stem_count`. This fixture's own ground truth is UNCHANGED: it still drives the
*default* `Morpher::new` (no override), so `tevimaflisra`'s zero-analyses pin above remains accurate
and was not touched. A genuine 3-stem compound of this exact shape DOES confirm today if a caller
opts in via `.with_max_stem_count(3)` -- pinned not here (this fixture is about the default oracle),
but in `rust/crates/pg-parse/tests/csharp_port_compounding.rs`'s
`simple_rules_4_three_root_compound_single_rule`/`simple_rules_5_three_root_compound_two_rules`,
which port C#'s own previously-omitted `MaxStemCount = 3` reconfiguration directly.
