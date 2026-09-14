# 001 verification — does `Add` vs `PriorityUnion` change a real parse set?

## Answer

**Yes.** Two independently constructed grammars (one flat, one with a nested complex feature)
each produce a real, forward-synthesizable word for which the C# founding oracle (master, `Add`
semantics) finds one valid analysis and HC-Rust `main` finds **zero**. Rust's output is byte-identical
to a from-source `PriorityUnion`-mode C# build on every word tested, confirming the loss is
attributable specifically to the `Add`→`PriorityUnion` fold change, not to some unrelated engine
difference. **The direction is the worse one named in the task brief: Rust `main` loses a parse
the C# founding oracle finds**, not the reverse.

A third, independent construction (replicating the PR author's own literal worked example) shows
the underlying gate-level divergence is real but does **not**, in that topology, propagate to a
parse-set difference — a genuine, useful negative result explained below (an artifact of how the
analysis search can route around the divergent gate).

No construction (adversarial or otherwise) produced the reverse direction — `PriorityUnion` finding
a parse `Add` misses. Section "Item 4" gives a structural proof (grounded in both engines' actual
source) for why that direction cannot occur for this fold, corroborated by the PR author's own
21-test adversarial sweep, which also never found it.

## Binaries and SHAs used, and how each one's semantics was confirmed

### C# founding oracle, `Add` semantics ("ADD")

- Binary: `hc.dll` built `Release` from
  `C:\Users\johnm\Documents\repos\machine\.worktrees\pr494-base\src\SIL.Machine.Morphology.HermitCrab.Tool\bin\Release\net10.0\hc.dll`.
- Source: worktree `pr494-base`, detached at **`d3b7643d7eee8e0471fb586874337a3d04d6296e`**
  (branches `conformance/fieldworks-witnesses`, `conformance/exact-analysis-fs-divergence`).
  Divergence entry 001 itself states this commit is confirmed `Add`-identical to `origin/master`.
- Rebuilt fresh this session (`dotnet build ... -c Release`, succeeded, 0 warnings/errors) before
  any measurement, so the binary reflects that exact source tree, not a stale artifact.
- Semantics confirmed by directly reading
  `src/SIL.Machine.Morphology.HermitCrab/MorphologicalRules/AnalysisAffixProcessRule.cs` at this
  commit: the un-application path calls
  `outWord.SyntacticFeatureStruct.Add(_rule.RequiredSyntacticFeatureStruct)` — the plain,
  undocumented-toggle `Add` call, with no `AnalysisSyntacticFeatureMerge` machinery present at all
  in this commit (that type doesn't exist yet on this branch's history).

### C# founding oracle, `PriorityUnion` semantics ("PU"), from PR #494's research branch

- Binary: `hc.dll` built `Release` from
  `C:\Users\johnm\Documents\repos\machine\.worktrees\pr494\src\SIL.Machine.Morphology.HermitCrab.Tool\bin\Release\net10.0\hc.dll`.
- Source: worktree `pr494`, branch `perf/pr494-priority-union`, at **`2acb3c52534cf6a8d879c22e5aaa5dfceba8d110`**.
- Semantics confirmed by directly diffing this commit's
  `AnalysisAffixProcessRule.cs` against `pr494-base`'s (`git diff d3b7643d 2acb3c52 --
  .../AnalysisAffixProcessRule.cs`): the `Add` call is replaced by
  `AnalysisSyntacticFeatureMerge.CanUnapply(...)` / a `priority_union`-shaped fold documented in
  `AnalysisSyntacticFeatureMerge.cs` and `docs/pr494-review.md` (read directly, see excerpts
  below). `AnalysisSyntacticFeatureMerge.Mode` was left at its default in every run here (no env
  var / test-only toggle was set), which per that file's own source is `PriorityUnion` — the mode
  PR #494 proposes for master.
- **This worktree's `hc.dll` could not initially run `batch` at all** — `perf/pr494-priority-union`
  branched from a point in history *before* `BatchCommand.cs`/`SignatureFormat.cs` were added
  upstream (`git merge-base d3b7643d 2acb3c52` = `d7347226`; neither commit is an ancestor of the
  other; `git log --diff-filter=A -- .../BatchCommand.cs` shows it was added in `f42d9591`, which
  is on `pr494-base`'s side of the fork and never reached `pr494`). Confirmed by listing each
  worktree's `.../HermitCrab.Tool/*.cs`: `pr494` is missing `BatchCommand.cs` and
  `SignatureFormat.cs` outright, and running `-s <script containing "batch ...">` against it prints
  the interactive help text instead of executing (there is no `batch` command registered).
  **Fix applied, mechanical and non-semantic, not committed:** copied `BatchCommand.cs` and
  `SignatureFormat.cs` verbatim from `pr494-base` into `pr494`'s
  `src/SIL.Machine.Morphology.HermitCrab.Tool/`, and added the one line
  `new BatchCommand(context),` to `pr494`'s `Program.cs` command list (the only other difference
  between the two branches' `Program.cs`, per a direct diff, is an unrelated exit-code plumbing
  change and a visibility change on `SplitCommandLine`). `BatchCommand`/`SignatureFormat` only call
  `Morpher.ParseWord` and format output — they touch no analysis-side code path, so this restores
  the CLI plumbing this branch's history simply predates without altering anything under test.
  Rebuilt (`dotnet build ... -c Release`, succeeded). `git status --short` in this worktree after
  the fact: `M Program.cs`, `?? BatchCommand.cs`, `?? SignatureFormat.cs` — uncommitted, as
  required; `pr494-base` was never touched (`git status --short` there is empty).
- With `batch` restored, this build's behavior was cross-checked against Rust `main` on every
  grammar below and found byte-identical except where noted — the strongest available confirmation
  that this binary really runs `PriorityUnion`, since Rust `main`'s `ana_syn_fs` is documented (and
  read directly, below) to implement exactly that fold.

### HC-Rust (`pangloss`)

- Binary: `G:\cargo-build-cache\baseline-459e8cb1\release\pangloss.exe`, built from PanGloss `main`
  at `459e8cb1` (as supplied; not rebuilt).
- Invoked exclusively through the managed launcher, one run slot at a time:
  `& .\rust\tools\pg.ps1 -Mode run -Exe <exe> -RunMemoryGB 4 -- batch <grammar.xml> <words.txt>
  <out.tsv> --threads 1 --word-timeout-ms 30000`. No bare `cargo`; no Rust rebuild performed.
- `pangloss batch` has no `--engine` flag in this build (`pangloss --help` lists none), so there is
  no ambiguity about which analyzer ran — it is whatever `batch` always runs, which is
  `pg_rules::morph::analyze`, the function containing `ana_syn_fs` (read directly below).

### Host configuration parity (PROTOCOL.md section 8)

None of `MaxStemCount`, `DeletionReapplications`, `MaxAlternatives`, `guessRoot`,
`LexEntrySelector`/`RuleSelector` are exercised by any grammar below (no compounding, no deletion
rules, no alternative cap hit, no guessing invoked, full lexicon/rule set used in every run) — the
three C# runs and all Rust runs share the only configuration that could matter here (plain
constructor defaults), so a divergence cannot be attributed to host-configuration skew.

### Confirming the two C# builds actually disagree before trusting any grammar-specific result

Direct diff of the analysis-side fold, `d3b7643d` → `2acb3c52`,
`src/SIL.Machine.Morphology.HermitCrab/MorphologicalRules/AnalysisAffixProcessRule.cs`:

```diff
-                if (!_rule.RequiredSyntacticFeatureStruct.IsEmpty)
-                    outWord.SyntacticFeatureStruct.Add(_rule.RequiredSyntacticFeatureStruct);
-                else if (_rule.OutSyntacticFeatureStruct.IsEmpty)
+                if (!AnalysisSyntacticFeatureMerge.CanUnapply(
+                        _rule.OutSyntacticFeatureStruct, _checkFs, input.SyntacticFeatureStruct))
+                    ...
```

(full context read in-session; `AnalysisSyntacticFeatureMerge`'s `Add`/`PriorityUnion`/`Exact`
modes and `CanUnapply` are documented at length in `docs/pr494-review.md`, read directly on the
`pr494` worktree.)

Rust `main`'s own fold, `rust/crates/pg-rules/src/morph.rs` (read at PanGloss `main`, unchanged
through the commit range this repo's own divergence catalogue names):

```rust
fn ana_syn_fs(g: &Grammar, req: FsId, out: FsId, word: &Word) -> Option<FeatureStruct> {
    let out_fs = g.fs_interner.get(out);
    if !is_unifiable(out_fs, &word.syn_fs) { return None; }
    let req_fs = g.fs_interner.get(req);
    if !req_fs.is_empty() {
        Some(priority_union(&word.syn_fs, req_fs))   // PriorityUnion, not Add
    } else if out_fs.is_empty() {
        Some(FeatureStruct::EMPTY)
    } else {
        Some(word.syn_fs.clone())
    }
}
```

`ana_compound`/`ana_compound_cached` (`pg-rules/src/morph.rs` lines 2998, 3042) call this exact same
function with `rule.head_required_syn_fs, rule.out_syn_fs` — compounding uses the identical fold,
so a compounding-specific grammar would exercise identical arithmetic to the affixation grammars
below rather than a new mechanism; none was built for that reason (see "What wasn't built," below).

`pg_featstruct::ops`'s `add`/`priority_union` (`rust/crates/pg-featstruct/src/ops.rs`, read in full
this session) documents, and its own doc comments cite line numbers for, the exact same two C#
routines (`FeatureStruct.AddImpl`/`SimpleFeatureValue.AddImpl`, `FeatureStruct.PriorityUnion`) that
were read directly in the C# source above — both engines' own code and comments agree on what each
side does, independent of this investigation.

## The four grammars

All four adversarial requirements from the task are covered by three built-and-run grammars, plus
a structural proof for the fourth (no empirical construction was found that satisfies it, and the
proof says why none can exist). Full XML, word lists, and every raw TSV are under
`docs/divergences/evidence/001/<grammar>/`.

### G1 — `evidence/001/g1-maxwell-chain/` (item 1: the PR author's own worked example)

A direct full-word translation of PR #494's own unit test
(`CategoryChangeChain_AccumulatedPosAndThirdRuleGate_MatchModeSemantics`,
`tests/SIL.Machine.Morphology.HermitCrab.Tests/MorphologicalRules/AnalysisSyntacticFeatureMergeTests.cs`
on `pr494`, read directly): head feature `cat` = {n, v}; `mrA2B` (req N, out V, suffix `u`);
`mrB2A` (req V, out N, suffix `i`); `mrThird` (req N, out V, **no affix** — same
Required/Output shape as `mrA2B`, deliberately, matching the C# test's own `ruleThird`). Root
`sag` carries no assigned features.

**Result: no divergence.** `parse_compare.py` on ADD vs PU: 3/3 words `IDENTICAL`. Rust vs either
C# build: 2/3 `IDENTICAL`, 1 `DIFFERENT` (`sagui`) — but the difference is a **labeling artifact in
C#'s own signature construction**, not an Add/PriorityUnion difference: `--trace` on the ADD build
(full trace under `evidence/001/g1-maxwell-chain/csharp-add-trace-sagui.txt`) shows `sagui` has a
genuine second parse where `third` is un-applied **outermost** (`Input: sagui, Output: sagui`,
contributing zero characters), wrapping the *entire* real `a2b+b2a` chain — not sandwiched between
them as originally hypothesized. Because `third` is tried at the very outermost layer, its own gate
check (`is_unifiable(out=V, fs)`) runs against `fs` before any divergent folding has happened
(`fs` is still the initial unconstrained value), so **both** `Add` and `PriorityUnion` admit it
equally: `csharp-add.tsv`/`csharp-pu.tsv` for `sagui` are byte-identical
(`SAG+A2B+B2A|sagui;SAG+A2B+THIRD|sagui`). C#'s own signature builder additionally drops `b2a`'s
morpheme ID from that entry (`SAG+A2B+THIRD`, not `SAG+A2B+B2A+THIRD`) because the zero-width
`third` morph shares a shape-node boundary with `b2a`'s and the two collide in
`AllomorphsInMorphOrder`'s traversal — Rust's signature builder does not have this collision and
correctly reports `SAG+A2B+B2A+THIRD|sagui`. This labeling quirk is orthogonal to the question this
task asks (both C# builds agree with each other regardless of the label), so it is reported as a
curiosity, not a finding.

**Why this matters:** it directly confirms the PR review's own recorded prediction for its
end-to-end version of the same scenario — "Task 1's CanUnapply-gate divergence is real but does
not, by itself, cause a lost end-to-end parse in this topology" — and explains *why*: a search that
can try the divergence-triggering rule at multiple structural positions only needs **one** of them
to be mode-independent for the parse-set to end up identical.

### G2 — `evidence/001/g2-stale-branch-num/` (items 2 and 3: 3-symbol lane, real parse loss)

Head feature `num` = {sg, du, pl} (a genuine 3-way symbol, so the widened Add value is a real
2-of-3 disjunction, never the degenerate "covers every declared symbol" case). Four suffixes on
root `zud`, unordered stratum:

| rule | RequiredHeadFeatures | OutputHeadFeatures | suffix |
|---|---|---|---|
| mrA (innermost) | num=du | num=pl | i |
| mrB | *(none)* | num=sg | u |
| mrC | num=sg | num=pl | e |
| mrD (outermost) | num=pl | *(none)* | o |

`zud`→`mrA`→`mrB`→`mrC`→`mrD` is a real, forward-synthesizable chain ending in `zudiueo`. Analysis
un-applies outside-in (D, C, B, A). Un-applying D folds onto the initial empty FS (both modes
agree: `pl`). Un-applying C folds `req_C=sg` against `pl`: **PriorityUnion → `sg`; Add → `{pl,sg}`**
— the divergence. Un-applying B's own gate (`out_B=sg`) is satisfied by both (`sg` is in Add's set
too), and B's own `RequiredHeadFeatures` is absent, so the fold passes the FS through unchanged for
both modes — **the stale `pl` alternative survives in Add's set three steps later.** Un-applying A
(innermost) checks `out_A=pl` against the surviving FS: **PriorityUnion has `sg` → refused (dead
end, 0 analyses); Add still has `{pl,sg}` → `pl` is a live member → admitted**, and the derivation
reaches the root successfully.

Controls, all as predicted and confirmed:
- `zud`, `zudi`, `zudiu` — both engines agree (`ZUD|zud`, `ZUD+A|zudi`, `ZUD+A+B|zudiu`).
- `zudiue` (mrA+mrB+mrC, **mrD skipped**) — negative control: **both** engines find 0 analyses
  (mrD never contributed the stale `pl`, so Add's own set is just `sg` here too — confirms the
  divergence is attributable specifically to mrD's contribution, not this rule shape in general).
- `zudueo` (mrB+mrC+mrD, **mrA skipped**) — non-vacuousness control: **both** engines agree
  (`ZUD+B+C+D|zudueo`) — the same override-then-require pair succeeds identically whenever the rule
  whose own gate is what diverges (mrA) is absent.
- **`zudiueo` (the full real chain) — THE result.** ADD (C# master): `ZUD+A+B+C+D|zudiueo` (found).
  PU (C# PR #494 build): `-` (0 analyses). **Rust `main`: `-` (0 analyses) — matches PU exactly.**

`parse_compare.py`, ADD vs PU: 5/6 `IDENTICAL`, 1 `DIFFERENT` (`zudiueo`: `rust[ok]:
ZUD+A+B+C+D|zudiueo` / `ref[ok]: -` — labels are just the tool's generic column names, both sides
here are C#). `parse_compare.py`, Rust vs ADD (founding oracle): same 5/6 `IDENTICAL`, 1
`DIFFERENT` on `zudiueo`, **Rust reporting `-` where the founding oracle reports a real parse**.
`parse_compare.py`, Rust vs PU: **6/6 `IDENTICAL`** — Rust and the from-source PriorityUnion C#
build agree on every single word, byte-for-byte.

### G3 — `evidence/001/g3-nested-complex-agr/` (item 4 attempt: does nesting change the direction?)

Identical topology to G2, but the 3-symbol feature (`pers` = 1/2/3) sits one level inside a
`ComplexFeature` `agr` (`RequiredHeadFeatures`/`OutputHeadFeatures` are
`FeatureValue(agr → FeatureValue(pers: X))` rather than a bare top-level value), root `vez`,
words `vez`/`veza`/`vezao`/`vezaou`/`vezoue`/`vezaoue`. Both `add` and `priority_union` recurse
identically into a nested `FeatureStruct` on both engines (read directly in `ops.rs` and
`FeatureStruct.cs`), so this predicts the exact same result as G2, one level deeper.

**Confirmed, not merely assumed.** ADD: `VEZ+A+B+C+D|vezaoue` (found). PU: `-`. Rust: `-`, matching
PU exactly (`parse_compare.py` Rust-vs-PU: 6/6 `IDENTICAL`). Every control matches G2's pattern
(`vezaou`, mrD skipped: 0 analyses both; `vezoue`, mrA skipped: found both). Nesting the divergent
feature one level down changes nothing about which direction the divergence runs — evidence against
a reverse-direction case existing in a nested-feature variant of this construction specifically,
which is exactly what this grammar was built to check.

### Item 4 (reverse direction: PriorityUnion finds a parse Add loses) — not found, and a proof for why

No construction here or in G1–G3 produced this direction. Beyond the empirical attempts, both
engines' own source support a structural argument that it cannot occur for this fold:

**Claim:** at every step of the analysis fold, for every feature, the set of symbol values Add's
accumulated FS admits is a **superset** of the single value PriorityUnion's accumulated FS holds.

**Proof sketch (by induction over the unapplication chain, grounded directly in the code read
above):** both folds start from the same initial FS (empty, or, at a feature, absent). At each
rule: if `req` is empty, both `add` and `priority_union`/C#'s `Add`/`PriorityUnion` pass the
existing value through unchanged for that feature (verified in both `ana_syn_fs`'s `else` branch
and `AnalysisAffixProcessRule`'s `Add`/`OutSyntacticFeatureStruct.IsEmpty` branch) — the superset
relation is preserved trivially. If `req` is non-empty at a feature: PriorityUnion's new value at
that feature is exactly `req`'s value (a single symbol, by construction of `priority_union`/C#'s
`PriorityUnion` always taking `b`'s leaf value wholesale); Add's new value is the **union** of the
old value and `req`'s value (`SymbolBits::union_with` / `SymbolicFeatureValue.UnionWith`), which by
definition contains `req`'s value as a member. So PriorityUnion's new singleton is always an
element of Add's new set — the superset relation is preserved inductively. Since `is_unifiable`
(the only kind of check `ana_syn_fs`'s gate and every `RequiredXFeatures`/`OutputXFeatures` check in
this fold performs) is a monotonic function of the accumulated set (non-empty intersection with a
*wider* set can only be easier to achieve than with a *narrower* one), **any check that
PriorityUnion's narrower FS passes, Add's wider FS also passes** — so Add can only ever admit a
superset of what PriorityUnion admits, never a strict subset. Add's "delete the feature when the
union covers every declared symbol" rule (`pg-featstruct::ops::add_value`'s `has_all` branch; C#'s
`FeatureStruct.AddImpl`'s `_definite.Remove` on `AddImpl` returning `false`) only ever makes Add's
side **more** permissive still (an absent/deleted feature is unconstrained, i.e. compatible with
everything) — it strengthens the argument, it does not weaken it.

This is corroborated independently: the PR author's own 21 adversarial C# tests
(`AnalysisSyntacticFeatureMergeTests.cs` on `pr494`, read in full this session — `CategoryChangeChain_*`,
`OverrideLoss_*`, `DisjunctiveRequired_*`, `NonOverlappingNestedHeadFeatures_*`,
`EmptyRequiredAndOut_*`, `SameRuleAppliedTwice_*`, `GuessedRoot_*`, `Compounding_*`,
`ShapeMerge_*`) never produce a case where `PriorityUnion` finds an analysis `Add` misses — every
recorded divergence in that suite runs the same direction (`Add` finds/attempts something
`PriorityUnion` refuses), including the `ShapeMerge_OneRulePathFirst_NoWidening_PriorityUnionLosesParse`
case, which is mechanically distinct (interacts with `MergeEquivalentAnalyses` across strata, not
the plain linear fold) yet still runs the same direction.

**What wasn't built, and why:** a compounding-rule grammar (would exercise the identical
`ana_syn_fs` call, see above — no new mechanism to find) and a literal `MaxApplicationCount=2`
"same rule twice" full-word grammar (worked through by hand: replicating the C# unit test's
artificial preset starting FS in a *real* full-word parse requires the same "stale branch via an
intervening empty-`req` rule" construction already built as G2/G3, so it would not exercise a
mechanism distinct from what's already tested).

## Full result tables

**G1 (`evidence/001/g1-maxwell-chain/`)**, ADD vs PU: `IDENTICAL 3/3`. Rust vs ADD and Rust vs PU:
`IDENTICAL 2/3`, `DIFFERENT 1/3` (`sagui`, labeling artifact only, see above).

**G2 (`evidence/001/g2-stale-branch-num/`)**, ADD vs PU: `IDENTICAL 5/6`, `DIFFERENT 1/6`
(`zudiueo`). Rust vs ADD: `IDENTICAL 5/6`, `DIFFERENT 1/6` (`zudiueo`, Rust reports `-`, ADD
reports `ZUD+A+B+C+D|zudiueo`). **Rust vs PU: `IDENTICAL 6/6`.**

**G3 (`evidence/001/g3-nested-complex-agr/`)**, ADD vs PU: `IDENTICAL 5/6`, `DIFFERENT 1/6`
(`vezaoue`). Rust vs ADD: `IDENTICAL 5/6`, `DIFFERENT 1/6` (`vezaoue`, Rust reports `-`, ADD reports
`VEZ+A+B+C+D|vezaoue`). **Rust vs PU: `IDENTICAL 6/6`.**

Raw `parse_compare.py` output for every pairing above is reproduced in full in this document's
construction; the underlying TSVs are the source of truth and are checked in under
`evidence/001/<grammar>/{csharp-add,csharp-pu,rust}.tsv`.

## Which engine is linguistically right, and which direction the loss runs

**Rust `main` loses a parse the C# founding oracle finds** (`zudiueo`, `vezaoue`) — the direction
the task brief names as "far worse than the reverse." Whether the *lost* parse is the linguistically
correct one is a separate question from which oracle's *current* behavior this repo's policy binds
to: `zudiueo`/`vezaoue` are real, forward-synthesizable words under their grammars (every
constituent rule application is one a real synthesis pass would make), so the C# master's `Add`-side
analysis is not a spurious over-generation artifact here — it is recovering a derivation that a
correct forward pass actually produces from these rules. `PriorityUnion`'s narrower fold discards
the information (`num`/`pers` at mrC's own written value) needed to satisfy mrA's own gate three
steps later, purely because of an *unrelated*, non-conflicting rule (mrB) sitting between them that
never touches that feature at all. That is exactly the shape of loss the PR author's own soundness
argument (`docs/pr494-review.md` section 1: "the only hazard is under-generation, and PriorityUnion
never rejects a stem synthesis would accept **at the rule itself**") does not cover, because the
rejection here happens two rules later than "at the rule itself," via the intervening empty-`req`
rule.

Per this repo's oracle-hierarchy rule, this is a real, reproducible violation of "HC-Rust must never
produce a different parse than C#" on `main` today, and the two remedies named in the task
statement (revert Rust to `Add`, or get PR #494 merged upstream) both remain open — PR #494 as
currently written does not fix this loss either (the PU build reproduces it identically to Rust),
so merging PR #494 as-is would not close this particular gap; only its own `Exact` mode is reported
elsewhere (`docs/pr494-review.md` section 4; entry 002's fixture) as finding some of these lost
parses, and that mode is explicitly "not shippable yet" per the same document.

## What could not be verified

- No compounding-rule or `MaxApplicationCount`-based full-word grammar was built (see "What wasn't
  built" above) — the code-level equivalence argument was checked by reading source, not by an
  additional empirical run.
- `hc-conformance.exe`'s own `--fixtures`/self-check mode was not used at all; every C# result here
  comes from `hc.dll` run directly through the documented adapter-equivalent `batch` command (per
  `PROTOCOL.md` section 7), which is a legitimate but different code path from the conformance
  harness's in-process self-check. The two are not expected to differ (both ultimately call
  `Morpher.ParseWord`/`SignatureFormat`), but this was not independently cross-checked against a
  self-check run for these specific ad hoc grammars.
- The `ShapeMerge`/`MergeEquivalentAnalyses` mechanism (PR review Task 3, a *different*,
  strata-crossing divergence that also happens to run in the Add-finds-more direction) was read and
  cited but not independently reproduced against Rust `main` here — reproducing the C# unit test's
  two-stratum, dual-rule-order setup as a `HermitCrabInput` XML grammar was judged out of scope for
  settling this specific question (the plain linear fold), given G2/G3 already give an unambiguous,
  real-word answer to it.
