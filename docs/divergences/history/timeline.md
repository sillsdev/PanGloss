# Divergence timeline

Reverse-chronological. Each entry names the repo the event happened in, a locator (commit SHA,
PR number, or file path), and either `OPEN` (the divergence it created or found is still live) or
`CLOSED` (it resolved one). Dates are the commit author-date (or the GitHub-recorded event
timestamp for PR activity), in the repo's local convention; where a PR was created on one date and
merged on another, both are given. All SHAs are at least 8 hex characters; PanGloss SHAs resolve
in this repo, `sillsdev/machine` SHAs resolve in `C:\Users\johnm\Documents\repos\machine`.

Read `mutual-catches.md` for the narrative behind the entries marked `[mutual catch]`, and
`upstream-prs.md` / `machine-branches.md` for the full detail behind any PR number or branch name
below.

## Current reconciliation (2026-09-15)

Rust Exact is on main at `149f88df`, not merely an unmerged branch. Machine #493 merged as
`52d069f8`; it removes template-required-feature accumulation rather than adopting Rust's union
widening. Issues [504](https://github.com/sillsdev/machine/issues/504),
[505](https://github.com/sillsdev/machine/issues/505), [506](https://github.com/sillsdev/machine/issues/506),
and [507](https://github.com/sillsdev/machine/issues/507) track the remaining shared correctness work.
The rows below are historical observations at their stated dates, not current branch status.

## 2026-09 — historical PriorityUnion / MergeEquivalentAnalyses work

| Date | Repo | Locator | Summary | Status |
|---|---|---|---|---|
| 2026-09-14 13:45 | C# | PR [#493](https://github.com/sillsdev/machine/pull/493), commit `a98228a6` | ddaspit commits "Don't add template required features to analysis output" directly onto PR 493. This differs from the earlier PanGloss union-widening approach. Review decision: **APPROVED**. Not yet merged (`mergeStateStatus: BEHIND`). | OPEN (approved, unmerged) |
| 2026-09-11 20:01 | C# | PR 493 comment | jtmaxwell3 raises a further, still-unresolved concern: `AnalysisAffixTemplateRule` folds a template's `RequiredSyntacticFeatureStruct` into `SyntacticFeatureStruct`, which `Word.ValueEquals` ignores — a rare shape (one inflectional affix reachable from two templates with different required FS) can still drop a valid analysis. Preferred fix (2): stop adding the template's FS at that site. Not yet implemented on either side. | OPEN |
| 2026-09-11 18:59 | C# | PR 493 comment | jtmaxwell3: `AnalysisStateKey` and `Word.ValueEquals` disagreeing is provably harmless in today's grammar model, because `SyntacticFeatureStruct` is fully determined by `_mruleApps` + `_realizationalFS` — *unless* the template case above is real. | OPEN |
| 2026-09-11 (branch `hc/memo-key-saturation`) | C# | `0eb2c45c` | "Saturate AnalysisStateKey rule-unapplication counts at each rule's cap" — **reverse-direction port**: brings a PanGloss-measured memo fix back into C#. Not yet a PR. | OPEN, not yet a PR — see `machine-branches.md` |
| 2026-09-11 | PanGloss | `da564946` (branch `fix/exact-analysis-fs`, unmerged) | "Exact inverse of synthesis for the analysis syntactic FS" — ports the C# owner's own experimental "Exact" merge mode (from `perf/pr494-priority-union`), going beyond both open C# PRs. Deliberately admits parses `hc.dll` currently misses. | **OPEN — deliberate, unmerged divergence from C# master** |
| 2026-09-10 19:50 / 21:45 | C# | PR 493 comments | jtmaxwell3 lays out three candidate fixes (his own `output.Add` fix, ddaspit's suggested rewrite, PanGloss's `3be1eb96`) and flags a second, independent `output.Add` bug. | OPEN |
| 2026-09-10 | PanGloss | `27cb405d` | Measures the PriorityUnion + state-keyed merge port (`5f06e428`) on five grammars: identical analysis sets on all 199 compared words, 1.3x-4.9x fewer rule attempts, Aweti no longer exceeds the 8GB job ceiling. Merged to `main`. | CLOSED (measurement; the port itself is `5f06e428`) |
| 2026-09-10 00:18 / 01:00 / 03:29 | C# | PR 494 comments | johnml1135 (PanGloss's author) posts step-count/wall-time measurements from four FieldWorks grammars and names the one regression PriorityUnion introduces (a real parse can be filtered out when `MergeEquivalentAnalyses` keeps the stricter of two merged analyses); points both PR 493 and 494 at branch `perf/pr494-proposed` as the combined fix. ddaspit and jtmaxwell3 both say they prefer routing the fix through PR 493. | OPEN |
| 2026-09-09 | PanGloss | `5f06e428` | "Port upstream HC analysis fixes — PriorityUnion and state-keyed analysis merge." Ports C# PR 494 (`Add`→`PriorityUnion`) and PR 493 (merge keyed on `AnalysisStateKey`), **plus** the review follow-ups neither PR had landed yet (identity fallback, `FeatureStruct.Union` widening on every fold). Merged to `main`. Full suite: identical failure set vs. base `c659ccaa` (2293 vs. 2278 tests). | **CLOSED on the Rust side; the C# PRs it ports are still open** |
| 2026-09-08 16:40 | C# | branch `perf/pr494-proposed`, commit `3be1eb96` | John Lambert (PanGloss's author, also a `sillsdev/machine` member) authors the combined PriorityUnion + FS-Union fix for compounding, on top of PR 494's tip. Pushed to `origin`, never opened as its own PR — offered as "cherry-pick or open against this branch" in the PR 494 comment thread. | OPEN, not a PR |
| 2026-09-04 15:36 / 19:36 | C# | PR [#494](https://github.com/sillsdev/machine/pull/494) opened / reviewed | "Change Add to PriorityUnion" (`dc8efec5`). ddaspit requests changes same day: add a unit test, and apply the same change to `AnalysisCompoundingRule`. | OPEN (`CHANGES_REQUESTED`) |
| 2026-09-03 14:43 / 20:04 | C# | PR [#493](https://github.com/sillsdev/machine/pull/493) opened / reviewed | "Fix bug in MergeEquivalentAnalyses" (`4562502b`). ddaspit finds a same-day defect: `wordCache` and `output` disagree on identity, so a rejected word can become permanently canonical. | OPEN (`CHANGES_REQUESTED` at the time) |
| 2026-09-03 (submodule) | PanGloss | `ab7ad2d960` → `machine@25ddf914c0`, then same-day `abfecf90a0` → `machine@100d7befed` | Two submodule repoints on the same day; the second is the pin PanGloss carries today. | see pin-history table below |
| 2026-09-02 19:14 | C# | `25ddf914c0` (branch `conformance/hc-rust-port-divergences`) | "Pin two behaviours a port diverged on" — two new C#-authored fixtures, `rewrite-analysis-feature-neutralization` and `synthesis-stratum-render-stale-table`, each with a positive witness and negative controls, for behaviours "a HermitCrab port got wrong." **Rust-equivalent status: not confirmed fixed in this investigation — see `mutual-catches.md`.** | OPEN (unverified from the Rust side) |

## 2026-08 — the founding-oracle doctrine, the repoint, and PR 471/474

| Date | Repo | Locator | Summary | Status |
|---|---|---|---|---|
| 2026-08-31 22:48 | PanGloss | `056af8e3` | "Make the C# founding oracle the standard, and gate on it." Adds `# oracle-provenance:` markers to every staged fixture, a gate ratcheting the rust-only backlog at 21, and `rust/tools/oracle-conformance.ps1`. Finds two new divergence classes (HC-Rust's XML loader over-accepts six schema-invalid staged grammars; nine `filter-passes/**` fixtures are structurally invisible to the C# harness) and **reverses its own earlier over-restrictive circumfix refusal**, which had mis-cited the W3.3 fix (see `mutual-catches.md`). `-Scope all`: 60 fixtures, 0 genuine signature divergence among the 16 C#-loadable staged ones. | CLOSED (doctrine landed); the two new divergence classes are OPEN |
| 2026-08-19 21:39 | C# | PR [#456](https://github.com/sillsdev/machine/pull/456) merged, `5d26fac6` (John Lambert, author) | "Memoize HermitCrab's sequential analysis cascade" — introduces `AnalysisStateKey`, later reused by PR 493's fix. | CLOSED (merged) |
| 2026-08-19 | PanGloss | `dc568ca0` → `da1b45a0` + `08d1cd2e` (same day) | Repointing the submodule to `integrate-conformance-framework` surfaces, then closes same-day, the "one reimplementation bug" PR 480 records: a `rightToLeftIterative` rule with multiple firing sites in one pass behaved as if `leftToRightIterative` were declared. `dc568ca0` pins it with two `#[ignore]`d regressions; `da1b45a0` fixes `pg_rules::metathesis::match_candidates` (never consulted rule direction); `08d1cd2e` fixes `syn_feature`/`ana_feature` re-considering a rejected rewrite site. | **CLOSED same day** |
| 2026-08-19 | PanGloss | `b106276a` | Records `metathesis-comparison-crash` as an accepted divergence (Rust reaches the correct analysis where C# used to throw), citing PR 471 by number as the upstream fix. | CLOSED |
| 2026-08-19 | PanGloss | `7b9ea42f` / `82aa9e29` / `8cb05155` (submodule) | Three repoints in one day: first to `integrate-conformance-framework`'s tip, then following its force-push to a new tip. | see pin-history table |
| 2026-08-18 11:07 | C# | PR [#471](https://github.com/sillsdev/machine/pull/471) merged, `60a0925f` | "Make metathesis switch-name order not matter" — fixes a real crash (`InvalidOperationException` / `ArgumentException: Only nodes from the same list can be compared`) plus a silent zero-analysis case, found while building generated-coverage fixtures on the conformance branch. Authored with Claude Code. | CLOSED |
| 2026-08-18 11:14 | C# | PR [#474](https://github.com/sillsdev/machine/pull/474) merged, `ba0e245a` | "Cap RealizationalRule synthesis application at once per word" — fixes an infinite-hang (`SynthesisRealizationalAffixProcessRule.Apply` never checked `MaxApplicationCount`). Found via a conformance fixture with three `mprRRealTest`-tagged roots. Authored with Claude Code. | CLOSED |
| 2026-08-14 | C# | PR [#475](https://github.com/sillsdev/machine/pull/475) opened | "Add a grammar health checker" for two unenforced grammar-authoring preconditions plus a partial-morpheme performance risk. Split out of the conformance PR (#480). Still open, `REVIEW_REQUIRED`. | OPEN |
| 2026-08-10 | PanGloss | `a763189d` → `machine@db0d5c7e42`, `a57e7ad2` → `machine@caa4ddde87` | Two same-week repoints: "repin to the upstream dead-rule attribution fix" and "repin to the trace-verified `blocked_by` attribution." | see pin-history table |
| 2026-08-09 | PanGloss | `bce19a1f` → `machine@37dfd679d0` | "Graduate the four coverage fixtures upstream and repin." | see pin-history table |

## 2026-07 — squash-copy, the conformance submodule lands, Rust's own oracle-fidelity pass

| Date | Repo | Locator | Summary | Status |
|---|---|---|---|---|
| 2026-07-28 | PanGloss | `2639067a` → `machine@73599a89` | Submodule repoint, "complete four-grammar FST parity recipes." | see pin-history table |
| 2026-07-26 | PanGloss | `docs/hermitcrab-rust-port-audit.md` §3a | Records the day's closures: guesser surface, analysis-side tracing, `FailureReason` gate order aligned to C#, `max_stem_count` exposed as a builder, `syn_epenthesis` fixed (two causes, both cited to C#), plus newly-found items including the RTL-bounded-quantifier shallow-reverse risk (structural, not yet reproduced) and the `XmlLanguageWriter` non-goal decision. | CLOSED (audit-doc snapshot); several sub-items remain OPEN, see `docs/hermitcrab-rust-port-audit.md` |
| 2026-07-25 | PanGloss | `471a865c` | "Correct 4 silently-dead exercises: tags + gate the whole class" — three staged fixtures' `exercises:` tags never matched a `constructs.txt` row id, so they contributed zero coverage silently; adds `exercises_tag_liveness` gate. | CLOSED |
| 2026-07-25 | PanGloss | `7bab72cd` [mutual catch] | "Make iterative rewrite pick-order direction-aware (oracle fix)" — a real bug in Rust's OWN confirm engine (candidate matches always picked leftmost, ignoring `rule.dir`), found while implementing RTL FST compilation, fixed by citing C# source directly (`IterativePhonologicalPatternRule.cs:17-48`, `AnalysisRewriteRule.cs:33`). | CLOSED |
| 2026-07-25 | PanGloss | `2cdccd08` [mutual catch] | "Make metathesis analysis a true inverse of synthesis (oracle fix)" — two bugs in Rust's metathesis analysis path (switch ordering by tag name instead of physical position; a dropped middle context node), found the same way. | CLOSED |
| 2026-07-25 | PanGloss | `c4c51df0` [mutual catch] | Resolves the last two open "oracle gaps." One is confirmed a real C# bug (`width_matches`/quantifier-as-focus: a top-level `Quantifier` throws `InvalidCastException` in C# even though the DTD permits it and the loader builds it) that Rust deliberately does **not** replicate — Rust is safer, and the divergence is **not yet proposed upstream**. The other ("ana_epenthesis finds no analysis") does not reproduce; the earlier report was a mis-attribution. | CLOSED (both investigated); the width_matches finding is **OPEN as an unfiled upstream defect** |
| 2026-07-24 | PanGloss | `d6ceea1c` → `machine@dd8f95c5` | Submodule repoint, "delanguaging part B." | see pin-history table |
| 2026-07-17 | PanGloss | `22d61db4` → `machine@3c8972c0` | Submodule repoint, adds `latin-diacritic-segments` conformance fixture. | see pin-history table |
| 2026-07-15 | PanGloss | `60e5d34c` → `machine@4c79ed0e` | Submodule fast-forward bump. | see pin-history table |
| 2026-07-13 | PanGloss | `d477c79d` → `machine@b04abfed` | **Conformance submodule added**, pinned to the `conformance-framework` (later `integrate-conformance-framework`) branch tip — this is PR [#480](https://github.com/sillsdev/machine/pull/480)'s branch, still open upstream as of 2026-09-14. | OPEN upstream, load-bearing here since this date |
| 2026-07-12 03:37-03:57 | C# | PRs [#446](https://github.com/sillsdev/machine/pull/446), [#451](https://github.com/sillsdev/machine/pull/451), [#453](https://github.com/sillsdev/machine/pull/453) all **closed without merging** | The `hc-rustify` / `parse-optimization` / `lt22613-nogood-cache-fix` experimental C#-side performance branches PanGloss's port was originally squash-copied from (per `docs/hermitcrab-rust-port-audit.md` §1). **Could not confirm the LT-22613 fix these PRs describe ever reached `origin/master`** — `GrammarAnalyzer.ComputeMaxAnalysisLength`, the method the audit doc names, does not exist anywhere in the current `machine` checkout. Treat the audit doc's "fixed on PR #453" claim as describing a since-abandoned branch, not current `master` state, until someone re-verifies. | **CLOSED (as PRs) without merging; the underlying claim is unverified** |
| 2026-07-10 | PanGloss | initial import, `a60d6169` | "Initial import: HermitCrab Rust port, foundation for PanGloss" — one squash commit from `machine`'s `rust` branch at commit `6781b9ac` (no preserved line history; that branch no longer exists in this local `machine` checkout or on `origin`, so its own commit-by-commit history is **unrecoverable from here**). Already includes the W3.3 discontinuous-morph environment-anchoring fix and its regression pin (`pg-parse/tests/discontinuous_env_gate.rs`, still `#[ignore]`d — see `mutual-catches.md`). | CLOSED (landed); the pre-import history behind W3.3 could not be determined |

## Conformance submodule pin history (`.gitmodules` branch: `integrate-conformance-framework`)

Every commit in this repo that moved the `machine` gitlink, oldest first:

| Date | PanGloss commit | New pin (`machine@`) | What it brought in |
|---|---|---|---|
| 2026-07-13 | `d477c79d` | `b04abfed` | Submodule added. |
| 2026-07-15 | `60e5d34c` | `4c79ed0e` | Fast-forward bump, no PanGloss-side notes. |
| 2026-07-17 | `22d61db4` | `3c8972c0` | Adds `latin-diacritic-segments` fixture. |
| 2026-07-24 | `d6ceea1c` | `dd8f95c5` | "Delanguaging part B" — fixture-name consumers updated. |
| 2026-07-25 | `b6e2a1d8` | `4560e9e2` | G8/G9 coverage cross-check + construct vocabulary work landing alongside. |
| 2026-07-28 | `2639067a` | `73599a89` | "Complete four-grammar FST parity recipes." |
| 2026-08-09 | `bce19a1f` | `37dfd679` | Graduates four coverage fixtures upstream. |
| 2026-08-10 | `a763189d` | `db0d5c7e` | Upstream dead-rule attribution fix. |
| 2026-08-10 | `a57e7ad2` | `caa4ddde` | Trace-verified `blocked_by` attribution. |
| 2026-08-19 | `7b9ea42f` | `b5b6411a` | Repoint to `integrate-conformance-framework`. |
| 2026-08-19 | `82aa9e29` | `b5b6411a` | Same pin, re-recorded (no-op on the gitlink itself). |
| 2026-08-19 | `8cb05155` | `74351b80` | Follows the branch's force-push to its new tip. |
| 2026-09-01 | `1900c146` | `f42d9591` | 0.2.0 release prep bump. |
| 2026-09-03 | `ab7ad2d9` | `25ddf914` | Pins at the two new port-divergence fixtures (see 2026-09-02 entry above). |
| 2026-09-03 | `abfecf90` | `100d7bef` | **Current pin.** Adds cross-table root-respelling fixture. |

**Currently behind the live branch.** `integrate-conformance-framework`'s tip in `machine` is
`4823a05a` ("test: make memoization conformance default"), one commit ahead of the `100d7bef` pin
PanGloss carries — both dated 2026-09-03, so the gap is small, but it means the memoization-default
conformance change is not yet exercised by PanGloss's own submodule.

## Pre-2026-07-10 (before this repo's own history begins)

| Date | Repo | Locator | Summary |
|---|---|---|---|
| 2018-11-28 | C# | `97fa7721` | "Do not allow non-contiguous morph annotations" — `MarkMorphs`' per-contiguous-run split. This is the C# behaviour the W3.3 Rust fix (pre-dating this repo's history) matches; see `mutual-catches.md`. |
