# STAGING: bistratal-overlapping-segment-representation

## Why this fixture exists

`docs/conformance/representative-typology-basis.md` S1.2.5 identifies the shared-representation
multi-table configuration as a genuine gap: the two existing `MultiTable`-covered fixtures
(`rust/crates/pg-foma/tests/two_table_symbol_divergence.rs`/`phase_c_multi_table.rs`) deliberately keep
their two tables' representations pairwise-disjoint; no fixture exercises the residual case where a
literal spelling (here, "s") is a legitimate `SegmentDefinition` in BOTH tables while denoting a
DIFFERENT segment identity in each -- the general native/loan-stratum-sharing-most-of-one-alphabet
phenomenon (Kiparsky 1982; see the research doc's own citations). This fixture pins:

1. **The structural characterization.** `pg-foma::capability::multi_table_detail` computes
   `representations_pairwise_disjoint == false` for this grammar's two tables, with
   `shared_representation_witness` naming the shared spelling "s".
2. **The capability gate's honest Refuse.** `MultiTableFaithfulThreadingPredicate`
   (`multi-table.faithful-table-threading`) Refuses this grammar via `evaluate_capability` --
   `pg_foma::replace::SegAlphabet::token`'s raw-per-table-index token scheme cannot, even after
   `fix-multitable-fst-compilation`'s per-rule table-threading fix, rule out a residual cross-table
   token collision once two tables share a representation (that predicate's own module doc).
3. **A separate, honestly-documented architectural fact**, NOT a MultiTable-specific bug: this
   codebase's own surface-tokenization convention (`pg_foma::emit::surface_table`, mirrored by
   `pg_parse::Morpher`'s own initial input segmentation) uses ONLY the grammar's LAST stratum's own
   table. A bare root entered on a non-final stratum (the Inner stratum here) can therefore never be a
   complete, tokenizable surface word by itself -- the SAME finding
   `rust/crates/pg-foma/tests/two_table_symbol_divergence.rs`'s own module doc already documents for
   the disjoint-table case ("a bare, unaffixed root declared on a NON-final stratum... is never a
   complete surface word by itself in this architecture"). This fixture's Inner-stratum roots
   (`basi`/`abis`) are pinned `expect_skip`, not silently omitted, for exactly this reason.

## What it pins

- `des`/`sed`: two Outer-stratum (table t2, the grammar's LAST/surface-facing table) roots, each
  using t2's OWN "s" identity -- proving ordinary lookup is unaffected by the shared representation.
- `basi`/`abis`: **`expect_skip: true`** -- Inner-stratum (table t1) roots, unreachable at the
  surface per the architectural fact above (not a MultiTable-specific finding, but honestly pinned
  rather than omitted).
- `eds`: **`expect_fail: true`** -- a well-formed string over t2's own alphabet that is not any real
  lexical entry, a plain negative control.

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Authored fresh for
this task; `words.yaml` signatures captured by driving `pg_parse::Morpher::parse_word_opts` directly
over every word (a throwaway test, deleted after transcription -- see "Verification").

## Verification

Signatures were captured via a throwaway test (`rust/crates/pg-foma/tests/zz_throwaway_sig_dump.rs`,
deleted after transcription) driving `pg_parse::Morpher::parse_word_opts` directly over every word in
`words.yaml`. An earlier draft additionally redeclared the Inner stratum's own "a"/"b"/"i" inside table
t2 (to let bare Inner-stratum roots reach the surface) and added a bridging `MorphologicalRule` with an
`ncAny` stem pattern on the Outer stratum -- this crashed (`index out of bounds` inside
`pg-grammar/src/chardef.rs`), an interesting but out-of-scope finding for a DIFFERENT, deeper
cross-stratum-rule-threading question; this fixture sidesteps it entirely by keeping each stratum's own
lexicon independent (no rule threads material between the two tables), matching
`two_table_symbol_divergence.rs`'s own established "scope: stratum 1 (last stratum) only" precedent.
Cross-checked in-repo by `rust/crates/pg-parse/tests/conformance_fixtures_gate.rs`'s
`all_discovered_fixtures_match_oracle` test (dual-root discovery, default `cargo test --workspace`
suite) -- that test is what actually gates CI. The capability-gate Refuse verdict is additionally
pinned directly by `rust/crates/pg-foma/tests/
cover_bistratal_overlapping_segment_representation.rs`, which asserts `evaluate_capability` returns
`CompileDecision::Refuse` naming `MultiTable`/`multi-table.faithful-table-threading`.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/bistratal-overlapping-segment-representation/`. On acceptance, delete
this staged copy in the same change (graduation guard enforces this mechanically).

## Coverage-tag correction (post-G9)

`constructs.txt` row 36 (`sillsdev/machine` PR #465, "G9") added
`"CharacterDefinitionTable: more than one table, one per stratum"` as this construct's own dedicated
row. `words.yaml`'s `exercises:` entries here previously read the bare characteristic name
`"MultiTable"`, which is NOT a `constructs.txt` row id and therefore matched nothing in
`conformance_coverage::construct_ids_for`'s byte-for-byte cross-check -- the tag silently contributed
zero coverage despite this fixture genuinely exercising the construct. Fixed to the exact row-36
string; no signature, `parses:`, or ground truth changed.
