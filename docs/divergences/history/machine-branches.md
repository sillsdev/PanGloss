# C# (`sillsdev/machine`) branches carrying unmerged work

Read from `C:\Users\johnm\Documents\repos\machine` (read-only: no branch was checked out, nothing
rebased, committed, or pushed from here). Tip SHAs and `git log --oneline origin/master..<branch>`
read on 2026-09-14. A branch with a `+` in `git branch -a` is checked out in a linked worktree
elsewhere on this machine (`machine.worktrees/`, which was empty at the time of this investigation)
— that does not change its content, only where its working tree currently lives.

## Current audit (2026-09-15)

The branch table below is a historical snapshot, not current status. Machine #493 is merged at
`52d069f8`; #494 is open at `3ad6b656`; #491 is open at `7c9aadd8`; #480's conformance head is
`8bad1934`; #490's documentation head is `3252bbc2`. Rust now uses Exact (`149f88df`), includes
final-template pruning, and has a health-checker port (`3541fbc2`). See `upstream-prs.md` and the
numbered ledger for reconciliation. Archived sparse-forest work is not restarted by this cleanup.

## Historical branches backing open PRs (see `upstream-prs.md` for the PR-review detail)

| Branch | Tip SHA | PR | Rust reflects it? |
|---|---|---|---|
| `change-add-to-priority-union` | `dc8efec5` | #494 | Yes — `5f06e428` |
| `fix-merge-equivalent-analyses` | `a98228a6` | #493 | Partially — `5f06e428` ports the pre-review version; the tip commit (`a98228a6`, 2026-09-14) is newer than the port |
| `filter-final-templates-in-analysis` | `669a7942` | #491 | No |
| `docs/hc-optimization-ledger` | `cc806613` | #490 | N/A — documentation/negative-result branch |
| `integrate-conformance-framework` | `4823a05a` | #480 | Partially — PanGloss's submodule pin (`100d7bef`) is one commit behind this tip, see `timeline.md` |
| `feature/grammar-health-checker` | (not independently examined) | #475 | No |

## Branches with no open PR — performance-optimization archives

These four (`perf/pr494-proposed`, `perf/pr494-priority-union`, `perf/pr494-break`,
`perf/pr494-conformance`) are stacked experiments around PR 494, all pushed to `origin` but never
opened as their own PRs. Ordered by how directly each stacks on the last.

### `perf/pr494-break` (tip `ab793869`)

Three commits ahead of `change-add-to-priority-union`. Adds adversarial
`AnalysisSyntacticFeatureMerge` tests specifically designed to break PR 494's naive PriorityUnion
change (template-level override loss, shape-merge regression), then fixes the break by widening the
canonical analysis's FS with `Union` on every fold. This is the commit (`ab793869`) that first
states the fix johnml1135 later reports back into PR 494's own thread.

### `perf/pr494-proposed` (tip `3be1eb96`)

One commit ahead of `change-add-to-priority-union` directly (not stacked on `perf/pr494-break`):
`1d11e750` (PR 494's own tip) then `3be1eb96`, "Apply PriorityUnion to compounding; keep merged
analyses' feature structure general; tests." This is the exact branch johnml1135 offers in the PR
493/494 comment threads as "happy to open it against this branch or you can cherry-pick." **This is
what PanGloss's `5f06e428` ports.**

### `perf/pr494-priority-union` (tip `2acb3c52`)

The full research trail behind the fix: a same-binary Add/PriorityUnion/Exact merge-mode toggle
(`HC_ANALYSIS_FS_MERGE` env var), a measurement harness (`FsMergeBench`, `fs-merge-bench.ps1`),
adversarial tests, the `Union`-widening fix (merged in from `perf/pr494-break`), and — the part not
yet reflected anywhere upstream as a PR — a third merge mode called **"Exact"**, extended and
measured across four real grammars (Sena, Mbugwe, and two others named only in the branch's own
docs). Its final commit, `2acb3c52` ("PR #494 review, break attempts, Exact extension, ready-to-paste
comment"), is explicitly working notes rather than a proposed diff. **Rust ported this "Exact" mode**
in `da564946` ("exact inverse of synthesis for the analysis syntactic FS," on branch
`fix/exact-analysis-fs`, unmerged in PanGloss too) — a case of the Rust side adopting an experimental
C# research idea that has not itself been proposed as a C# PR.

### `perf/pr494-conformance` (tip `ca5da199`)

The largest of the four: 20 commits, sharing most of its history with
`conformance/fieldworks-witnesses` (see below) plus three commits specific to verifying PR 494
against the conformance suite (`1a073b5c` PriorityUnion-for-required-syntactic-features with a
generalized merge and tests, `8d841fb7` "conformance-suite break attempts and draft review
comments," `ca5da199` "record unit-test outcome"). Reads as the author's own pre-flight validation
of PR 494 against the full conformance suite before posting the measurement comment on GitHub.

## The memo-key-saturation branch — a reverse-direction port

### `hc/memo-key-saturation` (tip `d1f9302d`)

Four commits, not yet opened as a PR:
1. `0eb2c45c` — "Saturate `AnalysisStateKey` rule-unapplication counts at each rule's cap" (2026-09-11)
2. `efc6a776` — extends the memo-on/off parity gate to a `MaxApplicationCount > 1` rule
3. `7c0e47f2` — adds `MEMOPROF`-style diagnostic counters to `AnalysisScope` and `Morpher`
4. `d1f9302d` — the PR description itself, "citing PanGloss's measurements"

**This is the one branch in this investigation where the direction runs C# ← Rust, not Rust ← C#.**
PanGloss's own memo-key saturation fix (`d3f227ab`, 2026-09-10, "saturate the memo key rule counts
at each rule max_apps") landed one day *before* this C# branch's equivalent commit (`0eb2c45c`,
2026-09-11), and `d1f9302d`'s own commit message says the PR description cites "PanGloss's
measurements." **No PR exists for this branch yet** (`gh pr list --search "memo-key-saturation"`
returns nothing) — this is worth watching for when it opens, since it would be a rare case of a
Rust-discovered fix becoming a tracked upstream PR rather than the reverse.

## Performance-archive branches (no correctness content found)

These four are large archives of HermitCrab performance experiments. Skimmed for anything that
changes observable parse results (which would make it relevant to divergence tracking) rather than
read commit-by-commit; none was found to change results, only speed.

- **`perf/hc-edge-prefilter`** (tip `be7f241d`, one commit) — an edge-segment prefilter for analysis
  affix rules. Prefilters are a correctness risk category by nature (a wrong prefilter drops valid
  analyses), but this is a single unreviewed commit with no PR and no test evidence examined here.
- **`perf/hc-optimization-archive`** (tip `3a0a3316`, one squash commit) — "archive of code, harness,
  and records" for the 2026-08-20..09-03 optimization round. Companion to PR #490's ledger.
- **`perf/hc-optimization-archive-history`** (tip `528c5dbc`) — the un-squashed version of the above,
  20 commits, ending in copy-on-write `SyntacticFeatureStruct` sharing and a two-pass NFA traversal
  toggle (`Fst.UseTwoPassTraversal`). Its own commit `179708d2` ("finalize HermitCrab optimization
  findings") frames the round as **net negative**: "the fastest thing actually built ran 4% slower;
  a second ran 28-32% slower. Nothing in the closed section shipped" (per PR #490's own body).
- **`perf/hc-engine-alloc`** / **`perf/hc-defer-template-clone`** / **`perf/hc-conformance-check`** —
  three more stacked branches in the same optimization round (allocation reduction, deferred
  template cloning, two-pass FSA traversal, `ExpandAlternatives` memoization). All feed into
  `perf/hc-optimization-archive-history` above; not separately relevant to correctness.

**None of these six performance branches were found to change any parse result** in the commits
examined — they are speed work, and PR #490's own conclusion is that almost none of it shipped
because the speedups didn't materialize. Not tracked further here; revisit only if one of them is
ever proposed as a PR that also claims a correctness fix.

## Other named branches

- **`conformance/fieldworks-witnesses`** (tip `d3b7643d`) — **the branch currently checked out** in
  the `machine` working copy used for this investigation. Real-FieldWorks-grammar producibility
  triage: converts several conformance verdicts (`mpr-gated-exception`, `compounding-breadth`,
  `polysynthetic-stratal-derivation-chain`, `fusional-realizational-morphology`,
  `morphotactic-attribute-breadth`) from "engine-only" to "fieldworks_producible," and resolves five
  previously-unclassified producibility verdicts as either a loader gap or a genuine data-model
  limit. Shares its full history with `integrate-conformance-framework` up to the point it
  diverges — it is downstream conformance-classification work, not a behavioral engine change.
- **`conformance/hc-rust-port-divergences`** (tip `25ddf914`) — see `timeline.md`'s 2026-09-02 entry
  and `mutual-catches.md`. Two commits ahead of `integrate-conformance-framework` at the point it
  was cut: `f42d9591` (the conformance suite baseline) and `25ddf914` ("pin two behaviours a port
  diverged on"). **This is the branch PanGloss's submodule was pinned to from 2026-09-03 through
  2026-09-03** (both the `ab7ad2d9` and `abfecf90` repoints happened the same day; the second moved
  past this branch's tip to `conformance/respelling-fixture-notes`'s).
- **`conformance/respelling-fixture-notes`** (tip `100d7bef`) — **PanGloss's current submodule pin.**
  One commit ahead of `hc-rust-port-divergences`: `100d7bef`, "pin cross-table root respelling as its
  own construct." Two docs-only commits precede it (`be851e3a` plans a memoized-conformance default,
  `853168d9` specifies it) that `integrate-conformance-framework`'s own tip (`4823a05a`) later acts
  on.
- **`docs/hc-optimization-ledger`** — see PR #490 above; same branch.
- **`investigate_linear_traverse`** (tip `998bb8af`, 9 commits) — an investigation into making
  `Fst.Traverse`'s acceptance check run in O(n) time (hash tables to eliminate duplicates, a lazy
  nondeterministic traversal method). No PR, no correctness claim; a research spike.
- **`investigate-memoization-rule-order`** (tip `4f5aeeb4`) — shares history with
  `conformance/hc-rust-port-divergences` up to `4f5aeeb4` ("tolerate CS0117 in the single-file source
  census"); the memoization-rule-order investigation itself does not appear to have produced further
  commits beyond that shared point, or those commits were not found under this branch name.
- **`pr-475`**, **`pr-491`**, **`pr-493-head`**, **`pr-494-head`** — local mirrors of the PR branches
  above, used for local review; no independent content.
