# STAGING: multi-table-metathesis-shared-representation

## Why this fixture exists

`docs/conformance/multitable-shared-representation-design.md`'s own "Residual gap this fix does NOT
close" section: `crate::replace::compile_metathesis_swap_net` used to render every switch-position
token DIRECTLY (`SegAlphabet::token`, table-blind, no cross-table alias expansion) rather than
through the alias-expanded path `two-table-shared-representation-recall` (task 4.4b) built for
ordinary rewrite rules. A `MetathesisRule` in a grammar whose tables share a normalized
representation therefore kept exactly the false negative 4.4b already fixed everywhere else. This
fixture combines `two-table-shared-representation-recall`'s own two-table, misaligned-shared-
representation structure with `right-to-left-metathesis-reversal`'s own multi-member-natural-class
`MetathesisRule` shape, per that closing task's own instructions.

Structural shape: two `CharacterDefinitionTable`s (`t0`="Inner", `t1`="Outer"), each stratum's own
`StratumDef::table` pointing at a different one. BOTH switch spellings ("m" and "x") are declared in
EACH table, at DELIBERATELY MISALIGNED raw indices (`t0`: m=0,x=1; `t1`: z=0[decoy],m=1,x=2,w=3
[decoy]). ROOT1 is entered on the Inner stratum (table `t0`), spelled "mx"; the `MetathesisRule`
lives on the Outer stratum (table `t1`) and must swap ROOT1's material to "xm" even though its own
natural classes (`ncSwitchA`={m,w}, `ncSwitchB`={x}) are resolved against `t1`'s different raw
indices for the same two spellings. ROOT2 (Outer stratum, spelled "wx" using `ncSwitchA`'s OTHER,
table-t1-only member "w") is a same-table positive control.

## What it pins

- `xw`: ROOT2 (Outer stratum), correctly metathesized. Positive, same-table control -- proves
  ordinary same-table metathesis recall is untouched by the fix.
- `wx`: ROOT2's own raw (un-metathesized) spelling -- `expect_fail: true` (metathesis is obligatory).
- `xm`/`mx`: ROOT1's (Inner stratum) expected metathesized/raw spellings -- see the finding below;
  both `expect_fail: true` as TRANSCRIBED, not as "should be."
- `z`: table `t1`'s own decoy segment -- `expect_fail: true`, a plain negative control.

## A second, separate discovered finding (transcribed honestly, not hidden)

Authoring this fixture surfaced a DIFFERENT, pre-existing gap from the one this task closes --
entirely inside `pg_rules::metathesis`/`pg_rules::bridge` (the oracle), never `pg_foma::replace`
(this task's own single-owner boundary). `pg_parse::Morpher` finds **zero** analyses for "xm" (ROOT1's
correctly-metathesized surface) even though the grammar is a faithful, DTD-legal multi-table
metathesis grammar and the corresponding same-table case (ROOT2/"xw") works correctly.

Two things were confirmed by direct experiment while narrowing this down:

1. `pg_rules::metathesis::synthesize`/`analyze` (`metathesis.rs:497,646`) hardcode
   `let table_id = TableId(0);` regardless of which table the rule's own stratum actually owns --
   the SAME "implicit table-zero default" antipattern `docs/conformance/
   multitable-shared-representation-design.md`'s whole narrative is about, just in the oracle instead
   of the proposer.
2. That hardcoding is NOT the sole cause: a throwaway probe that reordered the grammar's own
   `<CharacterDefinitionTable>` declarations so the rule's real table coincidentally became
   `TableId(0)` (confirmed via `g.strata[1].table == TableId(0)`) still found zero analyses for the
   cross-table root's swapped surface, with the misalignment otherwise unchanged. A THIRD throwaway
   probe with the raw indices for "m"/"x" made to coincide across both tables (no misalignment at
   all) found ROOT1 correctly (`"ROOT1|xm"`, 1 analysis) -- confirming cross-stratum metathesis
   threading itself works, and isolating the failure specifically to raw-index misalignment,
   independent of the `TableId(0)` hardcoding's own correctness. `pg_rules::bridge::nat_class_lanes`'s
   `NaturalClassKind::Feature` branch (used by this fixture's `FeatureNaturalClass`-based
   `ncSwitchA`/`ncSwitchB`) does not read `self.table` at all and `PatternBridge::feature_width` is
   grammar-wide, so the exact remaining mechanism was not fully isolated within this task's own
   `pg-foma`-only boundary -- reported as a real, reproducible, NOT-yet-root-caused finding rather
   than silently worked around or guessed at.

All three throwaway probes were deleted after recording this finding here, per this repo's own
established convention (mirrors `two-table-shared-representation-recall`'s own STAGING.md
precedent for its own unrelated discovered anomaly). This is entirely orthogonal to, and does not
block, the `pg_foma::replace`-level fix this fixture's own task was scoped to (routing
`compile_metathesis_swap_net`'s token rendering through the SAME alias-expanded path
`compile_rewrite_rule_subset` already uses) -- that fix is demonstrated and verified directly against
the compiled net in `rust/crates/pg-foma/tests/multi_table_metathesis_shared_representation.rs`,
bypassing this separate oracle gap entirely (the same "hand-render the pre-fix-equivalent net
directly" technique `two_table_shared_representation_recall.rs`'s own steps 1-2 already established).

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Authored fresh for
this task; `words.yaml` signatures captured via a throwaway test driving
`pg_parse::Morpher::parse_word_opts` directly over every word, deleted after transcription.

## Verification

Cross-checked in-repo by `rust/crates/pg-parse/tests/conformance_fixtures_gate.rs`'s
`all_discovered_fixtures_match_oracle` test (dual-root discovery, default `cargo test --workspace`
suite) -- that test is what actually gates CI, and only ever checks this fixture's words against the
ORACLE (never the FST proposer). The `pg_foma::replace`-level recall fix this fixture exists to pin
(loss reproduced, fix confirmed, switch-position identity preserved under aliasing) is pinned
directly by `rust/crates/pg-foma/tests/multi_table_metathesis_shared_representation.rs`, which also
demonstrates the fix over the real production compile path (`pg_foma::replace::
compile_and_compose_rules_with_budget`), not a hand-rolled token-math simulation, for every claim
that fixture-word's own oracle recall does NOT already block (the same-table ROOT2 case).

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/multi-table-metathesis-shared-representation/`. On acceptance,
delete this staged copy in the same change (graduation guard enforces this mechanically).
