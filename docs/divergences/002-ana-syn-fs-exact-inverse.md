# 002 — Analysis syntactic-FS fold: exact inverse of synthesis (unmerged Rust branch)

## Kind
Behavioural.

## Status
Open, and not yet merged into PanGloss `main` (branch `fix/exact-analysis-fs` at commit `da564946`,
based on `main`'s ancestor `459e8cb1`). Listed separately from entry 001 because it is a distinct,
stronger fold, not a variant of `PriorityUnion`.

## C# site
`AnalysisSyntacticFeatureMerge.cs` (`.worktrees/pr494/src/SIL.Machine.Morphology.HermitCrab/
MorphologicalRules/`, commit `2acb3c52` on the `pr494` research worktree — **this file does not
exist on `origin/master` at all**). It defines a same-binary, env-var-selectable toggle
(`HC_ANALYSIS_FS_MERGE=Add|PriorityUnion|Exact`) explicitly documented as a "research toggle for
sillsdev/machine PR #494", not a shipped feature:

```csharp
/// Exact inverse of synthesis. Synthesis produces PU(stem AND required, out), so the constraint on
/// the stem is (input with out's feature paths removed) unified with required, and the
/// un-application is only possible when the input unifies with PU(required, out). Never clears the
/// feature structure.
Exact,
```
```csharp
bool ok = Mode == AnalysisSyntacticFeatureMergeMode.Exact ? checkFs.IsUnifiable(input) : outFs.IsUnifiable(input);
```

## Rust site
`pg_rules::morph::ana_syn_fs` (`rust/crates/pg-rules/src/morph.rs`, branch `fix/exact-analysis-fs`
only):

```rust
fn ana_syn_fs(g: &Grammar, req: FsId, out: FsId, word: &Word) -> Option<FeatureStruct> {
    let req_fs = g.fs_interner.get(req);
    let out_fs = g.fs_interner.get(out);
    let check = priority_union(req_fs, out_fs);
    if !is_unifiable(&check, &word.syn_fs) { return None; }
    let stem = pg_featstruct::remove_paths(&word.syn_fs, out_fs);
    if req_fs.is_empty() {
        Some(stem)
    } else {
        Some(unify(&stem, req_fs).unwrap_or_else(|| priority_union(&stem, req_fs)))
    }
}
```

## What differs
Three-way comparison at this one gate:
- **C# master:** gate on `out.IsUnifiable(word.syn_fs)`; fold with `Add` (value-set union, entry 001).
- **PR #494 / Rust `main` (entry 001):** gate on `out.IsUnifiable(word.syn_fs)`; fold with
  `PriorityUnion` (required overwrites accumulated).
- **This branch:** gate on `PriorityUnion(required, out).IsUnifiable(word.syn_fs)` — a *stronger*
  check than either of the above, since `PriorityUnion(required, out)` is generally more specific
  than `out` alone. Then it strips `out`'s own feature paths off the accumulated FS before unifying
  in `required`, so a downstream rule's `out` feature never survives past this unapplication step
  unless `required` also names it. Never clears the whole FS the way `Add`'s `out.IsEmpty` branch
  does.

This is the exact algebraic inverse of what synthesis does going forward
(`PriorityUnion(Unify(stem, required), out)`), which is why the C# comment calls it "Exact." Neither
C# master nor PR #494 implements it as a real code path — it exists upstream only as a labelled
third arm of a research toggle whose default is `PriorityUnion`, not `Exact`.

## Can it change a parse?
Yes, in both directions relative to `PriorityUnion`, and measurably so.
`docs/research/2026-09-11-exact-analysis-fs-measurements.md` reports the Exact mode finding parses
that `PriorityUnion` (and so also `Add`, since `Add` is even weaker on the check side) loses: an
Amharic worst word (`ተማሪዮቹን`) goes from `TIMEOUT` at 60s under `main`'s `PriorityUnion` to a
completed 2-analysis parse in 35.6s under Exact, and an Aweti sample word (`otope`) that exhausted an
8GB memory ceiling under `PriorityUnion` completes under Exact. The doc states the direction
explicitly: "it admits a strict subset of the rule attempts, so it cannot lose a parse hc.dll finds"
— an argument, not (yet) a proof, and specific to comparison against `PriorityUnion`/Rust `main`, not
against C# master's `Add`.

## Evidence
`rust/crates/pg-parse/tests/exact_analysis_fs_recall.rs` on the `fix/exact-analysis-fs` branch pins
a case where `Exact` finds a parse `PriorityUnion` misses (an outer rule's `out` feature overwriting
an inner rule's `out` feature on the stem). `docs/research/2026-09-11-exact-analysis-fs-measurements.md`
reports full five-grammar timing/attempt-count/parse-set comparisons; `parse_compare.py` reports zero
analysis-set differences on every word both a `before` (PriorityUnion) and `exact` binary completed,
across 199 sampled words, plus the two gains above. The same doc flags one specific gap: the
prerequisite the C# research note names for shipping Exact — FS-aware template-battery dedup — has a
Rust equivalent (`run_template_batch_raw`/`apply_slot_batch` widening) that is implemented but,
per the doc's own words, "no existing or new test in this repo could be made to fail with it
disabled," i.e. it is defensively correct and untested for being load-bearing.

## Upstream
None, and not applicable in the normal sense: this fold has no PR against `sillsdev/machine`, only a
same-binary research toggle on the `pr494` worktree that is not even the toggle's default. If this
branch is ever merged into PanGloss `main`, the repo's own oracle-hierarchy rule requires either
proposing `Exact` upstream as its own PR (naming it as going beyond #494) or documenting, at minimum,
why HC-Rust's default behaviour is allowed to exceed the founding oracle's demonstrated correctness
on this one mechanism — since per this repo's rules, `NotProductionReady`/readiness concerns aside,
a behavioural divergence from `hc.dll` is never self-authorizing.

## Notes
This is very likely the single highest-priority item in this catalogue: it is a live, deliberate,
in-progress widening of the gap between HC-Rust and its founding oracle, on a branch clearly intended
for merge (its own measurement doc frames it as a strict improvement). Before merging, this entry's
open questions should be closed: (1) does Exact ever admit a parse the *actual* founding oracle
(`Add`, not `PriorityUnion`) would reject as spurious — the branch's own recall argument is stated
relative to `PriorityUnion`, not `Add`; (2) is the untested template-battery-widening code path
actually reachable on some grammar this repo doesn't yet have a fixture for.
