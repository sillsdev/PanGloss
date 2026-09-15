# Upstream `sillsdev/machine` pull requests

Every PR here touches `SIL.Machine.Morphology.HermitCrab` or its conformance suite. State and
review data read via `gh pr view`/`gh pr list --repo sillsdev/machine` on 2026-09-15 (focused rows refreshed; review chronology retained) — re-check
before trusting "current state" for anything still `OPEN`.

## Summary table

| # | Title | Author | State | Created | Merged | Rust equivalent |
|---|---|---|---|---|---|---|
| [494](https://github.com/sillsdev/machine/pull/494) | Change Add to PriorityUnion | jtmaxwell3 | OPEN, `CHANGES_REQUESTED` | 2026-09-04 | — | historical `5f06e428`; superseded by Exact `149f88df` (entry 002) |
| [493](https://github.com/sillsdev/machine/pull/493) | Fix bug in MergeEquivalentAnalyses | jtmaxwell3 | MERGED | 2026-09-03 | 2026-09-14, `52d069f8` | Earlier Rust port differs from merged template-FS treatment; entry 034 |
| [491](https://github.com/sillsdev/machine/pull/491) | Filter final templates in analysis | jtmaxwell3 | OPEN, `REVIEW_REQUIRED` | 2026-08-28 | — | Port exists with partial/invocation guards; entry 033 |
| [490](https://github.com/sillsdev/machine/pull/490) | HermitCrab optimization ledger: 22 attempts, no speedup, one located target | jtmaxwell3 | OPEN, `REVIEW_REQUIRED` | 2026-08-27 | — | N/A (docs only, negative-result ledger) |
| [489](https://github.com/sillsdev/machine/pull/489) | Fold-step synthesis memo: built, correct, and does not pay | jtmaxwell3 | CLOSED | — | — | N/A (evidence, not proposed for merge) |
| [488](https://github.com/sillsdev/machine/pull/488) | Measure where HermitCrab's time actually goes, across 33 grammars | jtmaxwell3 | CLOSED | — | — | N/A (measurement only) |
| [480](https://github.com/sillsdev/machine/pull/480) | Add a conformance suite for HermitCrab, with its adequacy argument | jtmaxwell3 | OPEN, `REVIEW_REQUIRED` | 2026-08-19 | — | PanGloss's `machine` submodule is pinned directly to this PR's branch (`integrate-conformance-framework`) since 2026-07-13 — see `timeline.md`'s pin-history table |
| [475](https://github.com/sillsdev/machine/pull/475) | Add a grammar health checker for two unenforced preconditions | jtmaxwell3 | OPEN, `REVIEW_REQUIRED` | 2026-08-14 | — | Port recorded at `3541fbc2`; outside this focused verification |
| [474](https://github.com/sillsdev/machine/pull/474) | Cap RealizationalRule synthesis application at once per word | jtmaxwell3 | **MERGED** | 2026-08-13 | 2026-08-18, `ba0e245a` | not separately verified — PanGloss's own `MaxApplicationCount`/`Blockable` handling predates this fixture; see "what still needs doing" below |
| [471](https://github.com/sillsdev/machine/pull/471) | fix(hermitcrab): make metathesis switch-name order not matter | jtmaxwell3 | **MERGED** | 2026-08-12 | 2026-08-18, `60a0925f` | `b106276a` (records the divergence as closed, cites #471 by number) |
| [456](https://github.com/sillsdev/machine/pull/456) | Memoize HermitCrab's sequential analysis cascade | johnml1135 | **MERGED** | — | 2026-08-19, `5d26fac6` | PanGloss's own memo work (`pg-memo` crate) is independent; `AnalysisStateKey` from this PR is what PR 493 later reuses |
| [452](https://github.com/sillsdev/machine/pull/452) | Support LT-22605: Add ability to limit HermitCrab parses | johnml1135 | **MERGED** | — | 2026-07-17 | **could not confirm** — no `LT-22605` or matching parse-limit reference found in PanGloss's history; unverified |
| [446](https://github.com/sillsdev/machine/pull/446) | RUSTIFY: allocation- and CPU-optimized HermitCrab parsing | jtmaxwell3 | CLOSED, not merged | — | — (closed 2026-07-12) | this is the branch family PanGloss's port was squash-copied from; see `docs/hermitcrab-rust-port-audit.md` §1 |
| [453](https://github.com/sillsdev/machine/pull/453) | LT-22613: fix analysis-length bound over-pruning | jtmaxwell3 | CLOSED, not merged | — | — (closed 2026-07-12) | **the underlying claim is unverified** — see below |
| [451](https://github.com/sillsdev/machine/pull/451) | HermitCrab parse optimization: memoize analysis redundancy, fix corpus scheduling, bound memory | jtmaxwell3 | CLOSED, not merged | — | — (closed 2026-07-12) | superseded in spirit by PR #456 |

## Live and load-bearing

### PR #493 — Fix bug in MergeEquivalentAnalyses

**What it changes.** `AnalysisStratumRule.MergeEquivalentAnalyses` used `Word.ValueEquals` (which
ignores `SyntacticFeatureStruct`) to decide when two analyses are the same state and can be
collapsed into one, kept-and-explored analysis with the rest recorded as `Alternatives`. That is
wrong once syntactic-feature-sensitive rules matter, so this PR keys the merge on the memoization
code's `AnalysisStateKey` instead (from PR #456).

**Full review-comment thread, in order:**

1. **ddaspit, 2026-09-04 (`CHANGES_REQUESTED`, on `4562502b`).** `wordCache` and `output` disagree
   on identity: `Word.ValueEquals` ignores `SyntacticFeatureStruct`, which the new key includes, so
   analyses differing only in that FS get distinct keys, skip the merge, and are then dropped by
   `output.Add` (a `Dictionary` keyed on `ValueEquals`, which treats them as duplicates of something
   already present). Also, `wordCache[key]` is written *before* `output.Add` accepts the word, so a
   later-rejected word can stay the permanent canonical for its key and silently swallow everything
   else that hashes there.
2. **ddaspit, 2026-09-09 (`COMMENTED`).** Proposes a concrete rewrite: check `wordCache` first, then
   also check `output` itself for a `ValueEquals`-equal word before inserting, and only register the
   `wordCache` entry once `output.Add` has actually accepted the word.
3. **jtmaxwell3, 2026-09-09.** Replies (by email, quoted in the thread) that this "fixes the AI's
   complaint" but not the deeper bug: if two words are equal under `ValueEquals` but differ in
   `SyntacticFeatureStruct`, and one would succeed downstream while the other would fail, folding
   the would-succeed one as an alternative of the would-fail one loses a valid analysis outright.
4. **johnml1135 (PanGloss's author), 2026-09-10T00:25:24Z (`COMMENTED`).** Cross-links PR 494: the
   fix there (loosen the canonical analysis's FS via `FeatureStruct.Union` on every fold) is exactly
   the general fix for this class of problem too, and "whichever PR lands first should carry it."
5. **jtmaxwell3, 2026-09-10T19:50:30Z.** Asks which of three fixes ddaspit actually wants: his own
   `output.Add` fix (can drop solutions), ddaspit's suggested rewrite (can lose valid solutions the
   same way), or PanGloss's `3be1eb96` (the Union-widening fix).
6. **jtmaxwell3, 2026-09-10T21:45:01Z.** States a belief that there is a second, independent
   `output.Add` bug even without `MergeEquivalentAnalyses`: two `ValueEquals`-equal words that differ
   only in `SyntacticFeatureStruct`, one of which would succeed and one fail, would still see one
   dropped by `output.Add` alone.
7. **ddaspit, 2026-09-10 (`COMMENTED`).** Prefers leaving `Word.ValueEquals` untouched (too many
   downstream effects) and making a local fix instead.
8. **ddaspit, 2026-09-10 (`COMMENTED`).** Proposes a three-step plan: (1) fix `output.Add` by adding
   `SyntacticFeatureStruct` to `Word.ValueEquals`, (2) merge #493, (3) merge #494.
9. **jtmaxwell3, 2026-09-11T18:59:57Z.** Argues no `Word.ValueEquals` change is needed:
   `SyntacticFeatureStruct` is uniquely determined by `_mruleApps` + `_realizationalFS`, so
   `AnalysisStateKey` and `output` cannot actually disagree — *unless* `AnalysisAffixTemplateRule`
   changes `SyntacticFeatureStruct` independently.
10. **ddaspit, 2026-09-11T19:30:36Z.** Points out `AnalysisAffixTemplateRule.cs:54` does exactly
    that — folds a template's `RequiredSyntacticFeatureStruct` into the word's
    `SyntacticFeatureStruct`.
11. **jtmaxwell3, 2026-09-12T17:18:11Z.** Constructs the minimal repro (delete
    `RequiredSyntacticFeatureStruct` from `edSuffix` in the existing `AffixTemplateTests.cs` test at
    line 409) and offers three fixes: (1) make inflectional affixes require the template's category
    at compile time, (2) **stop folding the template's `RequiredFeatureStruct` into the word's FS at
    `AnalysisAffixTemplateRule.cs:54`** (preferred), (3) do nothing. This was open at that point; the next item and final merged diff supersede that status.
12. **ddaspit, 2026-09-14T13:45:06Z (`APPROVED`, commit `a98228a6`).** "In the interest of getting
    this in as quickly as possible," commits the suggested rewrite from comment 2 directly
    (matching solution 2, i.e. stopping the template-FS fold) rather than continuing to discuss it,
    adds tests, and approves. Reviewable shows "all discussions resolved."

**Current state (2026-09-15).** Merged as `52d069f845b43f8bc88a95a56a8511aa58def26f`.
The merged fix uses AnalysisStateKey, registers only after output insertion succeeds, and stops
adding template-required syntactic features in analysis. It is not the earlier union-widening proposal.
Rust still accumulates template-required features; [issue #505](https://github.com/sillsdev/machine/issues/505)
and entries 034/035 track reconciliation. Existing widening tests passing with widening disabled
leave a proof gap, not a confirmed need to add widening to Machine.

### PR #494 — Change Add to PriorityUnion

**What it changes.** `AnalysisAffixProcessRule.Apply` merges a candidate's syntactic FS onto the
rule's required FS with `Add` (which behaves like `Union` — keeps both possible values), matching
neither `SynthesisAffixProcessRule` (which uses `PriorityUnion`) nor the intuition that three or
more category-changing derivational rules in a row should filter down to one concrete category
after each step. Measured performance win on real grammars is large for pathologically slow words:
Mbugwe 14.7M→4.0M feature checks / 340s→133s wall time; Sena 747k→138k feature checks / 28s→8.5s.

**Review-comment thread:**

1. **ddaspit, 2026-09-04 (`CHANGES_REQUESTED`).** Wants a unit test for the three-rule-chain
   scenario in the PR description, and wants the same `Add`→`PriorityUnion` change applied to
   `AnalysisCompoundingRule` as well (not just the affix-process rule).
2. **johnml1135, 2026-09-10T00:18:49Z (`COMMENTED`).** Reports the measurement table above plus
   **one regression, with a fix**: PriorityUnion can make `MergeEquivalentAnalyses` keep the
   *stricter* of two merged analyses' feature requirements (previously `Add` guaranteed the kept
   one was always looser), so a rule valid for the discarded analysis gets wrongly filtered and a
   real parse disappears. On Mbugwe this fires ~100k times with <1% step-count change once fixed.
   Offers branch `perf/pr494-proposed` (`3be1eb96`) containing: this fix (`FeatureStruct.Union` on
   merge), the requested `AnalysisCompoundingRule` change, and three new tests.
   Also cross-references PR 493 in a same-day comment: prefers routing the fix through #493's
   `AnalysisStateKey` mechanism over ad hoc `FeatureStruct.Union` calls.
3. **ddaspit, 2026-09-11T15:19:34Z (`COMMENTED`).** Restates the same two asks (unit test,
   compounding-rule parity) — as of this comment, neither has landed on PR 494's own branch.

**Current state (2026-09-15).** Open, head `3ad6b65621ce7eb8fba0747a1329db43cfbcd16a`.
The current branch includes #493 and tests; historical requests above must not be presented as
still missing without inspecting this head. Its proposal remains PriorityUnion, not Exact.
Rust main now uses Exact (`149f88df`). The Exact proposal is a posted comment, not a merge-ready PR.
[Issue #504](https://github.com/sillsdev/machine/issues/504) tracks the maintainer's requested rerun
and current shared fixture; [#505](https://github.com/sillsdev/machine/issues/505) tracks merge interactions.
There is still Rust-side verification work; the former “nothing on the Rust side” claim was wrong.

## Merged, and what Rust did with them

### PR #471 — metathesis switch-name order

Fixes a crash (`ShapeNode.CompareTo` throwing "Only nodes from the same list can be compared") when
a `MetathesisRule`'s `leftSwitch` names the physically earlier of its two pattern groups, plus a
silent zero-analysis case in the same configuration. Found while building generated-coverage
fixtures for the HermitCrab XML surface on the conformance-framework branch; authored with Claude
Code. Rust never had this bug (its metathesis analysis path already reconstructs by physical
position after `2cdccd08`, 2026-07-25 — a separate, earlier fix to a different ordering mistake).
PanGloss records the crash as an accepted-then-closed divergence in `b106276a` (2026-08-19), citing
#471 by number, once the conformance repoint surfaced the crashing grammar shape here too.

### PR #474 — RealizationalRule application cap

Fixes an infinite hang: `SynthesisRealizationalAffixProcessRule.Apply` never checked
`GetApplicationCount` against a cap the way ordinary affix/compounding rules do, so a `Blockable`
`RealizationalRule` not rescued by `CheckBlocking` could reapply to its own output forever. Found
via a conformance fixture with three `mprRRealTest`-tagged roots (the first two dodge the bug for
unrelated reasons). **This investigation could not find a dedicated PanGloss commit confirming the
Rust port has (or never had) the equivalent unbounded-reapplication defect for realizational rules**
— `pg-rules`' `MaxApplicationCount`-style guards for ordinary affix/compounding rules exist, but no
commit message or test name matching "realizational" + "cap"/"hang"/"infinite" was found. Treat as
**unverified**; worth a targeted regression test mirroring PR 474's `MorpherTests.cs` case
(`ParseWord_UnblockedRealizationalRuleMatchesOwnOutput_DoesNotHang`).

### PR #456 — memoize the analysis cascade

Authored by johnml1135 (PanGloss's author). Introduces `AnalysisStateKey`, later reused by PR 493.
PanGloss's own memoization (`pg-memo` crate, `rust/crates/pg-memo/`) was developed independently and
predates this PR's merge; the two are not a port relationship in either direction for the base
memoization mechanism, but PR 493's fix (which Rust ports in `5f06e428`) depends on this PR's key
type. Separately, the `hc/memo-key-saturation` branch (`0eb2c45c`, 2026-09-11, not yet a PR) explicitly
ports a memo-key saturation fix **from PanGloss back into C#** — see `machine-branches.md` and
`mutual-catches.md`.

## Open upstream proposals

### PR #491 — Filter final templates in analysis

Addresses a real performance pathology: HermitCrab tries every interleaving of templates and affix
rules during analysis even when the templates involved are final, sometimes taking minutes to hours
per word. The synthesis side already has an equivalent guard
(`NonPartialRuleProhibitedAfterFinalTemplate`); the analysis side can't apply it directly because it
doesn't know yet whether the root is partial, so this PR adds `Morpher.IsFinal`,
`Morpher.AlwaysEnforceFinalTemplates`, `Word.FinalTemplateState`, and `AffixStateKey.FinalTemplateState`
to let analysis filter final-template un-application when the configured grammar policy permits enforcing final-template restrictions. Long
review history (`filter-final-templates-in-analysis` branch, 14 commits, "Fix issues reported by
John Lambert") suggests substantial back-and-forth; `reviewDecision: REVIEW_REQUIRED`, still open.
**Not ported to Rust.** This is a genuine, not-yet-adopted performance-behavior change — worth
tracking because if it ever changes which analyses are returned (not just how fast), Rust would
diverge silently until someone notices.

### PR #480 — the conformance suite itself

Not a bug fix — this is the PR that would formally land `conformance/PROTOCOL.md`,
`conformance/`'s 33 fixtures and 446 cases, and the coverage apparatus. PanGloss already depends on
its branch (`integrate-conformance-framework`) directly via the `machine` submodule (since
2026-07-13), so in practice this PR's content is "merged" into PanGloss's build even though it is
formally unmerged upstream. Its own body records "two C# HermitCrab defects" (one fixed as #471,
one `simultaneous-epenthesis-cascade`, an accepted infinite-loop-cap divergence PanGloss also
tracks) and "one reimplementation bug" (the `rightToLeftIterative` multi-site ordering issue Rust
fixed same-day in `da1b45a0`/`08d1cd2e` on 2026-08-19 — see `timeline.md`). Recommended next step
named in the PR body itself: differential testing between the two engines directly, which is
functionally what this repo's own conformance-staging work already does.

### PR #475 — grammar health checker

Reports two hard grammar-authoring requirements (every used segment must be declared in a
`CharacterDefinitionTable`; each segment needs a distinct phonological feature vector) plus a
partial-morpheme performance risk, none of which previously produced any diagnostic. Split out of
PR #480. Rust's port is recorded at `3541fbc2`; the earlier absence claim is stale. This cleanup does not re-verify its behavior.

## Unverified / could not determine

- **PR #452 (LT-22605, "Add ability to limit HermitCrab parses")** — merged 2026-07-17. No
  reference to `LT-22605`, a parse-count limit, or a matching feature name was found anywhere in
  PanGloss's git history. Either Rust already had an equivalent limit under a different name (e.g.
  a `max_alternatives`/analysis-count cap in `pg-rules`), never needed one, or has a real gap here.
  **Not established either way by this investigation.**
- **PR #450 ("Fix XmlLanguageWriter crash on dangling morpheme/allomorph co-occurrence rules")** —
  merged prior to the window this investigation searched closely; not examined in detail. PanGloss's
  `XmlLanguageWriter` round-trip is a declared non-goal (`docs/hermitcrab-rust-port-audit.md` §3a,
  "WON'T DO"), so this PR is likely moot for Rust, but that was not directly confirmed against #450's
  actual diff.
- **PR #453 (LT-22613 fix)** — see `timeline.md`'s 2026-07-12 entry. Closed without merging, and
  the method it describes fixing does not exist on current `origin/master`. Whether the underlying
  C# bug still exists on master, or was fixed through some other unlinked commit, was not
  established.
## Shared correctness issues

| Issue | Scope | Related PR |
|---|---|---|
| [504](https://github.com/sillsdev/machine/issues/504) | Exact inversion and stale-output parse loss | 494 |
| [505](https://github.com/sillsdev/machine/issues/505) | Discriminating merge coverage and Rust/C# reconciliation | 493, 494 |
| [506](https://github.com/sillsdev/machine/issues/506) | Unstable zero-width identities; suspected ordering dependency | 500 |
| [507](https://github.com/sillsdev/machine/issues/507) | Final-template correctness controls | 491, 456 |

#500 remains open: dropped-ID fix and remaining ordering instability are distinct. See entries 036/037.
