# Phase 2 sub-plan: Narrowing analysis + the search-budget model (W8) — COMPLETED (with follow-ups)

> **OUTCOME (2026-07-09, commit `1abb849c` ff-merged onto `rust`):** budget model + general
> narrowing/expansion analysis are in. The global per-`parse_word` `StepBudget` replaced the
> per-StratumAnalyzer counter (the "undesigned amplifier" below); see `rust/docs/budget-model.md`.
> Landing was preceded by root-causing a catastrophic Amharic regression in two passes:
> (1) BoundaryMarker mis-kinding (`a697ad06`/`7d194ec0`); (2) **the real fix, found by a
> Fable-model agent after Sonnet passes missed it**: `untruncate()` emitted `max(min,1)` copies for
> quantifiers where C# (`Infinite==-1` → zero iterations) emits none, fabricating a phantom
> optional wildcard segment that let unrelated rules "unapply" through it (`92d2d5da`, verified
> 5-6x speedup on the regressed words, zero signature change). The scary residual
> (ሌባዬ family "superlinear blowup") was a false alarm — too-short test timeouts; ሌባዬ terminates
> naturally at ~298s with the correct gold signature, ~25.8k steps vs C#'s ~28k rule-attempts on
> the same word. Diagnostic instrumentation landed alongside: `StepBudget::steps()`,
> `ParseOutcome.steps`, `HC_STEP_STATS=1`.
>
> **Open follow-ups spun out to the finish plan (`rust-optimizations-phase2.md`):**
> (a) genuine ~2x per-step FST-traversal wall-clock gap vs C# (→ O2);
> (b) `--step-cap` does not bound wall-clock — full-corpus Amharic needs a per-word watchdog
> (→ O1); (c) step-5 acceptance (Merge/Expand D-batch-5 tests, quantified Amharic gains) is
> partially open — Amharic full has NOT been re-measured post-landing (→ V1);
> (d) the syn_epenthesis word-initial fix this plan inherited was NOT included — still open (→ P1).

**Why one plan:** Tier-1 #6 (general narrowing/expansion analysis — Amharic's prule1/2/3/6/7,
5 of its 7 phonological rules) is implementation-complete but cannot land because the **budget
model** makes it a measured net regression; and honest Sena full-corpus parity is gated on the same
budget question. Fixing the budget model unblocks both.
Sources: `NARROWING-FINDINGS.md` (worktree `../.worktrees/tier6b-rebase`), audit
`rust/parity-out/audit/phase2/B-phonology-parity.md` §5, rust-conversion.md §13.2 step 4.

## Facts (all measured, none speculative)

- HEAD's `ana_narrow` handles only the deletion case; non-empty-RHS narrowing subrules silently
  contribute no analyses. Amharic 532/673 is achieved WITHOUT narrowing.
- The complete implementation + the decisive over-wide-span guard live on
  `wip/tier1-6-plus-cache-probe` @ `97504528` (probe with #14: `wip/tier1-6-plus-14-probe`).
  With the guard: ሄደ 87s→1.8s; ሌባው completes and matches gold.
- **Narrowing has no demonstrated corpus gain at cap=100k**: its hypothesized target words already
  match at HEAD via the deletion path; activating it pushes 3 currently-matching words
  (ሌባዬ/በቅሎው/በቅሎዬ) past the cap → net −3/0. They complete AND match at higher budgets.
- C# floods on the same rule family too (mechanism byte-identical, Gate B off, UseDefaults no-op)
  but **runs uncapped** — C# has no step budget at all; it pays wall-clock (Sena gold worst words
  ~25-29s each) and survives on memoization.
- Rust's budget is **per-StratumAnalyzer-instance**: effective budget = `cap × #stratum-analyze
  calls` — an undesigned amplifier, unreconciled with anything in C#.
- Post-guard, the measured cost sink is DOWNSTREAM of the rewrite FST: the morphological affix
  matcher re-processing Optional-flooded shapes (46s FST traversal + 16s freeze on the affix side
  vs 0.3ms in the rewrite FST). Tier-2 #14's merge (~25% Sena speedup) was probed and is NOT
  sufficient to bring the 3 words under cap.

## Plan

1. **Design the budget model deliberately** (decision doc first, small):
   - Replace the per-instance step counter with ONE per-`parse_word` budget threaded through all
     stratum/rule invocations (closes the amplifier).
   - Make the default budget policy explicit: option (a) high global cap (e.g. 1-10M steps —
     calibrate so every C#-completable Amharic/Sena gold word completes), option (b) uncapped +
     wall-clock watchdog like C# (batch already has per-word timing; the complexity-cap work
     [[complexity_cap_plan]] wants caps for pathological grammars — reconcile: cap stays as a
     configurable safety, default sized from calibration, NOT tuned to make slow words fail).
   - Re-calibrate `--step-cap` semantics in hc-cli docs and any tests pinning step counts.
2. **Land the width-guard consolidation first** (it's W1.1, independent, pays for itself).
3. **Rebase + land narrowing** (`97504528` content) under the new budget:
   - Re-measure full Amharic: expect ≥532 with zero regressions AND quantify gains (which of the
     141 misses now match — the narrowing-dependent verb forms). Report exact word lists.
   - Sena first-100 + sena-fast byte/match comparison.
   - Wall-clock honesty: report p50/p95 and worst-word times before/after; some words will
     legitimately take seconds (C# pays the same).
4. **Chase the residual efficiency divergence** (only after 3 lands; measure first):
   C# completes Amharic gold fast; Rust-with-narrowing needed ~74s for ሌባው even with cache+#14.
   Ranked hypotheses from the findings doc:
   a. per-instance budget amplifier (fixed by step 1 — re-measure before chasing further);
   b. keep-longer dedup preferring Optional-flooded shapes (correct per C#; interaction cost —
      compare candidate counts per stratum against a C# trace);
   c. affix-matcher cost over flooded shapes — C# compiles matchers once AND its traversal may
      prune Optional branches differently; instrument branch counts in `hc-fst` traversal on ሌባዬ
      vs the same word's C# trace.
5. **Acceptance:** `RewriteRuleTests.MergeRules`/`MultipleMergeRules`/`ExpandRules` ported and
   passing (D-batch-5 unlocks); Amharic ≥532 with quantified gains; no Sena regression; worst-word
   wall-clock within ~2x of C# master on the same words (from the timing benchmark +
   `golden/master/sena-full.tsv` baselines).

## Non-goals
- Do NOT gate "safe vs unsafe" narrowing rules heuristically (no C#-faithful basis — confirmed).
- Do NOT re-litigate the reframe: narrowing lands when it is a measured net-≥0 under the new
  budget, not before.
