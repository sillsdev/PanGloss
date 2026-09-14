# 001 — Analysis syntactic-FS fold: `Add` (C# master) vs `PriorityUnion` (Rust main)

## Kind
Behavioural.

## Status
Open. This is a divergence Rust has already shipped on `main`, not a hypothetical — it is not
proposed upstream, and hc.dll master has not adopted it. It happens to match an *unmerged* upstream
PR (sillsdev/machine #494), but "matches an unmerged PR" is not "matches the founding oracle."

## C# site
`AnalysisAffixProcessRule.cs` and `AnalysisCompoundingRule.cs`
(`src/SIL.Machine.Morphology.HermitCrab/MorphologicalRules/`), both at the line
`outWord.SyntacticFeatureStruct.Add(_rule.RequiredSyntacticFeatureStruct)` (affix) /
`outWord.SyntacticFeatureStruct.Add(_rule.HeadRequiredSyntacticFeatureStruct)` (compounding).
Revision read: `machine` branch `conformance/fieldworks-witnesses` at `d3b7643d`, confirmed identical
at `origin/master`. `FeatureStruct.Add`/`AddImpl` (`FeatureStruct.cs:453-505`) is a per-feature
**value-set union** — it keeps whatever was already on the accumulated FS *and* adds the rule's
required value as an alternative, never replacing.

## Rust site
`pg_rules::morph::ana_syn_fs` (`rust/crates/pg-rules/src/morph.rs`, `main` at commit `459e8cb1`
and every later `main` commit through `4c870938`):

```rust
fn ana_syn_fs(g: &Grammar, req: FsId, out: FsId, word: &Word) -> Option<FeatureStruct> {
    let out_fs = g.fs_interner.get(out);
    if !is_unifiable(out_fs, &word.syn_fs) { return None; }
    let req_fs = g.fs_interner.get(req);
    if !req_fs.is_empty() {
        Some(priority_union(&word.syn_fs, req_fs))   // <-- PriorityUnion, not Add
    } else if out_fs.is_empty() {
        Some(FeatureStruct::EMPTY)
    } else {
        Some(word.syn_fs.clone())
    }
}
```

## What differs
C# widens: unapplying a rule with `RequiredSyntacticFeatureStruct = num:pl` onto a word whose
accumulated FS says `num:sg` leaves the accumulated FS able to satisfy *either* `sg` or `pl` at that
feature, because `Add` unions the value sets rather than replacing. Rust narrows: the same
unapplication leaves the accumulated FS at exactly `pl`, because `priority_union` lets the rule's
required value overwrite the stem's accumulated value at that feature. A later rule's `IsUnifiable`
gate against the accumulated FS can therefore pass in C# where it fails in Rust, or vice versa,
whenever the two orders disagree about which value should be considered "current."

**Precise rule each side follows:**
- C#: `accumulated' = accumulated ∪ required` (a per-feature value-set union; both values remain
  live alternatives on the FS going forward).
- Rust: `accumulated' = PriorityUnion(accumulated, required)` (per-feature: `required`'s value wins
  wherever `required` specifies one; `accumulated`'s value survives only where `required` is silent).

## Can it change a parse?
Yes. `docs/research/pg-rules-analysis-syn-fs-gate-notes.md` gives a concrete pinned fixture: a
two-rule chain where an inner rule's `RequiredHeadFeatures = num:pl` is unapplied onto a `num:sg`
word, and an outer rule's `OutputHeadFeatures = num:pl` gate is checked against the result. Under
`Add`, the accumulated FS after the inner unapplication is the widened `{sg, pl}`, which is
unifiable with `pl` — the same widened value would also unify with a hypothetical conflicting
`third` symbol the accumulated FS never actually held, over-admitting a chain no real stem state
supports (demonstrated at the `add(v, n, mask)` contrast points in the same fixture file). Under
`PriorityUnion`, the accumulated FS narrows to exactly `pl`, correctly rejecting that same
over-admission. So: for a chain of two or more analysis-side rules with required/output syntactic
features on the same feature, the two folds disagree on which downstream unifications are
admissible, and each one admits an analysis the other rejects on some grammar shape.

## Evidence
`docs/research/2026-09-10-hc-analysis-fs-port-measurements.md` measures both sides on five real
grammars (Indonesian, Sena, Amharic, Mbugwe, Aweti): 100% byte-identical parse sets and statuses on
199 compared words between the pre-PriorityUnion baseline and the PriorityUnion branch — meaning
none of the five reference/stress grammars this repo currently has on hand happens to exercise the
divergent case, even though the gate fixture proves the case is real and reachable in principle.
Unit gate: `pg-rules/tests/analysis_syn_fs_gate.rs`
(`analysis_required_fs_overrides_accumulated_value_priority_union`,
`analysis_affix_process_rule_category_change_chain_required_overrides_accumulated_pos`) pins the
`PriorityUnion` behaviour and asserts, by contrast, what the old `Add` semantics would wrongly admit.

## Upstream
None merged. `sillsdev/machine` PR #494 (branch `perf/pr494-priority-union`, worktree `pr494` at
`2acb3c52`) proposes exactly this change to hc.dll but has not been merged to `origin/master` as of
this reading. Rust adopted the PR's semantics on its own `main` before the PR was accepted upstream —
this is the divergence: Rust today matches a *proposal*, not the founding oracle. The correct
disposition per this repo's oracle-hierarchy rule is either (a) get PR #494 merged upstream, which
would close this from the C# side, or (b) revert Rust to `Add` until it does. Neither has happened;
Rust `main` has shipped the divergent behaviour since before commit `459e8cb1`.

## Notes
See entry 002 for a *further* divergence stacked on top of this one (an unmerged Rust branch that
goes beyond even PR #494's `PriorityUnion` to an "exact inverse of synthesis" fold that exists
upstream only as a same-binary research toggle, never as a PR). The two entries describe three
distinct behaviours in total: C# master (`Add`), PR #494 / Rust `main` (`PriorityUnion`), and Rust's
unmerged `fix/exact-analysis-fs` branch (`Exact`).
