# 041 — Oracle-side (`pg-rules`) phonological/metathesis/allomorph resolution defaulted to table 0

## Kind
Behavioural.

## Status
Fixed-in-rust.

## C# site
`Stratum.CharacterDefinitionTable` (`Stratum.cs:95`): every `PhonologicalRule`, `MorphologicalRule`
and `Allomorph` holds (directly or via its owning `Stratum`) a live reference to the ONE
`CharacterDefinitionTable` that stratum was constructed with. There is no separate "which table does
this rule belong to" resolution step in C# at all — no ambient table-0 default exists to fall back to,
because a rule can only ever see the table object its own `Stratum` already carries.

## Rust site
`pg_rules::cache` (`rust/crates/pg-rules/src/cache.rs`)'s `owning_table_for_prule`/
`owning_table_for_metathesis_rule`/`owning_table_for_morpheme`/`owning_table_for_allomorph`/
`owning_table_for_mrule`/`owning_table_for_compounding_rule` helpers, and their callers:
`pg_rules::metathesis::synthesize`/`analyze` (`metathesis.rs`), `pg_rules::rewrite::
synthesize_with_mpr_cached`/`analyze_cached` (`rewrite.rs`), and roughly a dozen call sites in
`pg_rules::morph` (`cd_lanes`, `ctx_pins`, `ctx_lanes`, `ctx_cd_set`, `segs_of`, `compile_parts`,
`strip_boundaries`, `build_analysis_lhs`, `generate_shape`, `untruncate`, `copy_part`,
`insert_segments`, `build_compound_cache`, `build_allomorph_lhs_cache`).

## What differs
Rust represents a grammar's tables as an indexable `Vec` (`Grammar::char_tables`) rather than each
rule holding its own table reference, so it needs an explicit per-rule "which table" resolution step
that C# structurally cannot need. Before the fix, that step did not exist: `pg-rules/src/cache.rs`
and `metathesis.rs` each carried a plain `const TABLE: TableId = TableId(0)`, and several `morph.rs`
functions did the same, so every rule/allomorph on a non-zero-table stratum had its pattern compiled,
matched, or its output constructed against table 0's char-defs regardless of which stratum actually
owned it. This was live in `rewrite.rs`'s PRODUCTION hot paths (`synthesize_with_mpr_cached`/
`analyze_cached`, the entry points for every ordinary rewrite rule reached through `crate::stratum`)
as well as in `metathesis.rs` and the dozen `morph.rs` sites — eleven total, briefed as five.

**Precise rule each side follows:** C# never has an "implicit table 0" because a rule's owning table
is a field, not a lookup. Pre-fix Rust's oracle-side (confirm-engine) resolution used a compile-time
constant instead of the rule's own owning stratum, mirroring the antipattern `pg_foma::replace::
owning_table` was already built to avoid on the FST-compile side (see entry 040 for that side's
own, structurally distinct bug).

## Can it change a parse?
Yes: on any grammar with more than one `CharacterDefinitionTable`, a phonological/metathesis rule or
allomorph owned by a non-zero-table stratum had its pattern or output resolved against the WRONG
table's char-defs, causing the confirm engine (`pg_parse::Morpher`, the reference every FST
propose-confirm containment proof is measured against) to reject or misconstruct analyses a
correctly-resolved oracle accepts.

## Evidence
`multi-table-metathesis-shared-representation`'s own STAGING.md names the metathesis instance
directly (`metathesis.rs:497/646` hardcoding `TableId(0)`, found while root-causing that fixture's
"xm" analysis gap) and confirms, via a throwaway probe, that correcting only the metathesis site was
not sufficient (a second, structurally distinct bug — entry 042 — remained). `pg-rules/src/cache.rs`'s
own `owning_table_tests` module is the first in-repo test to exercise the table-DEPENDENT
(`SegmentNaturalClass`) path against a non-zero table; reverting the fix there was confirmed to panic.
`conformance-staging/edge-cases/segment-natural-class-table-binding/` makes the same class of defect
visible to the CONFORMANCE suite for the first time: every pre-existing multi-table fixture builds its
rules from `FeatureNaturalClass` only, whose lanes are table-agnostic by construction
(`pg_rules::bridge::PatternBridge::nat_class_lanes`'s `Feature` branch never reads `self.table`), so
none of them could ever have detected a wrong-table resolution — this fixture's own
`SegmentNaturalClass`-based rule (raw per-table `CharDefId` members) can, and its own paired test
`rust/crates/pg-foma-backend/tests/segment_natural_class_table_binding_discriminates.rs` demonstrates the
discriminating power directly (`ncK` resolved against the wrong table becomes non-unifiable with a
real table-1 "k"). Fix commits: `84f11d69` (eleven-site sweep, `cache.rs` resolvers) and `9cfcd4c2`
(`morph.rs`'s remaining `const TABLE` removed).

## Upstream
None, not applicable — pure Rust-side representational gap (Rust's multi-table `Vec` needs a
resolution step C#'s per-stratum table reference never needed); C# was already correct by
construction.

## Notes
Two further, narrower findings are tracked separately because each is a structurally distinct
mechanism, not part of this same sweep: entry 042 (a metathesis-only stale-identity bug this sweep
did not touch, found while root-causing the same fixture) and entry 010 (a later, unrelated fix to
`pg_parse::surface::matching_reps_for_node`'s natural-class-membership gate, found only once
`segment-natural-class-table-binding` became loadable by the founding oracle — see that entry's own
update).
