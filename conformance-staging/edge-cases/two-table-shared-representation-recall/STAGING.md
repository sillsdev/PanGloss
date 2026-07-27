# STAGING: two-table-shared-representation-recall

## Why this fixture exists

`plan-construct-coverage-completion` task 4.4b / `docs/conformance/multitable-shared-representation-
design.md`: the RECALL-side counterpart to `conformance-staging/edge-cases/
bistratal-overlapping-segment-representation/` (which pins the REFUSAL/verdict side only, with each
stratum's own lexicon deliberately independent -- no rule threads material between its two tables).
This fixture is the one that DOES thread material across tables: a root entered on the Inner
stratum (table t0) is devoiced by the Outer stratum's (table t1) own obligatory phonological rule,
and table t1 spells the shared segment ("x") at a DIFFERENT raw index than table t0 does.

`pg_foma::replace::SegAlphabet::token` (`PUA_BASE + cd.0`) is a pure function of a char-def's raw
per-table index, blind to which table it came from. Pre-fix, a rule compiled against table t1
rendered its "x" atom using table t1's own index; a root emitted from table t0 carries table t0's
own index for "x". These differ, so the rule never fired on the Inner-stratum root's material, and
the FST proposer never proposed the devoiced ("y") surface form the oracle (`pg_parse::Morpher`,
which resolves every segment via genuine feature-lane unification, never a raw-index comparison)
correctly finds. `crate::replace::RepresentationAliasMap`/`SegAlphabet::render_tokens` (render-time
cross-table token aliasing, consumed by `crate::lower::render_slots`) closes exactly this gap.

This fixture pins:

1. **The structural characterization.** Two tables (`Grammar::char_tables.len() == 2`), NOT
   pairwise-disjoint (both declare "x"), each stratum's own `StratumDef::table` pointing at a
   DIFFERENT table.
2. **The capability gate's `ConfirmOnly` verdict** (same as `bistratal-overlapping-segment-
   representation`'s own pin, task 4.4b's flip from the old `Refuse`) -- not re-pinned by a
   dedicated Rust test here (that predicate-level pin already lives on the OTHER fixture); this
   fixture's own paired Rust test (`rust/crates/pg-foma/tests/
   two_table_shared_representation_recall.rs`) instead exercises the MECHANISM the verdict flip
   rests on.
3. **The oracle's own correct, cross-table analysis** of every word below (`pg_parse::Morpher`).
4. **The FST proposer's own recall**, demonstrated in three steps by the paired Rust test:
   - the pre-fix-equivalent (table-blind, unaliased) rule net provably MISSES the Inner-stratum
     root's material (never rewrites it to "y"),
   - the CURRENT (fixed) compile provably CATCHES it, and
   - end-to-end containment against the oracle holds for every word in this file.

## What it pins

- `y`: ROOT1 (Inner stratum), correctly devoiced through the Outer stratum's own rule. Positive,
  the cross-table recall case.
- `x`: ROOT1's own raw (undevoiced) spelling -- `expect_fail: true` (the rule is obligatory).
- `z`: table B's own decoy segment (declared FIRST in `t1`'s `SegmentDefinitions`, so `t1`'s own
  "x" sits at raw index 1, not 0 -- the deliberate misalignment) -- `expect_fail: true`, a plain
  negative control.
- `q`: ROOT2 (Outer stratum, table B) -- a same-table positive control, unaffected by the rule.

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Authored fresh for
this task; `words.yaml` signatures captured via a throwaway test (`rust/crates/pg-foma/tests/
zz_throwaway_sig_dump.rs`, deleted after transcription) driving `pg_parse::Morpher::parse_word_opts`
directly over every word.

## A discovered, out-of-scope finding (transcribed honestly, not hidden)

`morpher.parse_word_opts("y", ..).signature()`'s SURFACE half renders empty (`"ROOT1|"`, not
`"ROOT1|y"`), even though the underlying MORPHEME-level analysis (root identity) is exactly correct
-- `n_analyses == 1`, correctly naming ROOT1. Confirmed, by direct experiment, NOT specific to this
fixture's multi-table shape: an equivalent SINGLE-TABLE grammar with the identical environment-free,
natural-class-to-natural-class feature-changing phonological rule renders its surface half
correctly (`"ROOT|y"`); confirmed also NOT keyed to "the rule's owning table has TableId(0)"
specifically (swapping which of the two `CharacterDefinitionTable` elements is declared first in
the XML, so the rule's own table becomes `TableId(0)` instead of `TableId(1)`, does not fix it
either). This looks like a genuine, narrow `pg_parse`/`pg_rules` synthesis-side bug: `pg_rules::
stratum::synthesize_stratum_traced` never assigns a candidate `Word`'s own `.stratum` field the way
the ANALYSIS-direction `analyze()` does (`input.stratum = self.stratum_id;`, `pg-rules/src/
stratum.rs:1242` -- no equivalent assignment anywhere in the synthesis-direction functions), so
`Morpher::surface_of`'s `g.strata[w.stratum.0].table` lookup can resolve the WRONG table for a root
synthesized past its own entry stratum. A DIFFERENT crate (`pg-rules`/`pg-parse`), a DIFFERENT bug
class, entirely out of scope for `plan-construct-coverage-completion` task 4.4b's `pg-foma`-only
single-owner boundary -- flagged here for a follow-on, not silently avoided (mirrors `bistratal-
overlapping-segment-representation`'s own STAGING.md precedent, which similarly documents an
unrelated "index out of bounds" crash discovered while authoring that fixture). `words.yaml`'s own
`y` entry pins the signature AS TRANSCRIBED (`"ROOT1|"`) -- this is the honest, current, real engine
output, not a hand-derived value -- and the paired Rust test's own containment check compares
MORPHEME-level `structured` analyses directly, never this signature's surface half, so this
unrelated gap cannot contaminate this fixture's own multi-table-aliasing pin.

## Verification

Cross-checked in-repo by `rust/crates/pg-parse/tests/conformance_fixtures_gate.rs`'s
`all_discovered_fixtures_match_oracle` test (dual-root discovery, default `cargo test --workspace`
suite) -- that test is what actually gates CI. The deeper FST-proposer-vs-oracle containment claim
(items 3-4 above) is pinned directly by `rust/crates/pg-foma/tests/
two_table_shared_representation_recall.rs`, which also demonstrates the pre-fix loss and the fix
closing it over the real production compile path (`pg_foma::replace`), not a hand-rolled token-math
simulation.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/two-table-shared-representation-recall/`. On acceptance, delete
this staged copy in the same change (graduation guard enforces this mechanically).
