# Independent analysis-attempt and search-work limits

Both per-word caps are **resource containment**. A capped or timed-out result is incomplete, regardless of how many analyses it contains. Grammar representability and production admission follow their existing contracts; this allowance adds neither a refusal nor a semantic filter.

## Ownership and counting

`pg_rules::stratum::StepBudget` owns the analysis-attempt counter and an independent `pg_fst::work::WorkBudget` for the word. `new(attempt_cap)` derives a work allowance with `default_work_cap`; `with_limits(attempt_cap, work_cap)` supplies both explicitly. `Morpher::with_work_cap` overrides only work, leaving its analysis cap unchanged. `Morpher` installs it before surface segmentation and keeps it active through analysis, lookup, alternative expansion, synthesis confirmation, validity and surface matching. Stratum entry points also install their supplied budget so direct calls are bounded. Bounded generation installs the same allowance across its strata.

Synchronous matcher calls use a scoped thread-local adapter because the matcher, trie, alternative and rule APIs are also used independently. A scope guard restores its caller during normal return or unwinding and cannot cross threads; each batch word creates its own allowance on its worker thread. Standalone operations with no scope intentionally retain their uncapped API behavior. Clones of a `WorkBudget` consume shared state.

Analysis checks both caps and increments attempts only at the morphological rule-attempt seam. Confirmation checks only work and the deadline, so it can confirm candidates found before analysis reaches its limit. The production pipeline disables the legacy local synthesis cascade cap rather than passing the analysis-attempt limit into it. Synthesis-only stratum/template helpers, classic generation and bounded-generation classification keep their supplied caps as explicit work limits. `Morpher::with_work_cap` overrides per-word parsing; classic generation retains its constructor allowance.

A successful reservation increments total work by one before the work starts. A refused reservation latches `capped` or `timed_out` and does no additional work. The total never exceeds the cap. Polling at exactly the cap conservatively latches exhaustion: finishing a branch does not establish that the whole search completed. Failed matchers discard their partially assembled result list; the parse owner checks the latch before accepting confirmation. Earlier completed analyses can survive in an explicitly incomplete outcome.

| Phase | Reservation sites |
|---|---|
| Surface input | Each Unicode scalar before allocating the search shape |
| Analysis and synthesis | Cascade branch decisions, attempted morphological rules, synthesis orchestration, template slots and candidates |
| Rule application | Allomorph/subrule choices, analysis matches, non-head root resolution, optional reconstruction and dedup comparisons |
| FST | Start positions, recursive initialization/advance, each inspected arc, register commands, accepting outputs and distinct candidates |
| Phonological rewriting | Subrule choices, iterative passes, candidate spans and insertion sites; nested matching spends the same allowance |
| Root/overlay lookup | Query nodes, trie edges, optional skip branches and accepted entries |
| Alternatives | Each recursive expansion and source replay |
| Confirmation | Lexical/allomorph candidates, expansion outputs, synthesis outputs, environments and disjunctive allomorph checks |
| Guess/surface matching | Recursive match branches and lexical pattern choices |

The existing epenthesis shape bound and grammar-derived finite limits remain in force. No larger limit substitutes for proof of a complete result.

## Costs within a unit

A reservation bounds search branching, not CPU instructions or bytes. Feature comparisons and shape/capture copies operate on the finite grammar and current shape. Input size is reserved before segmentation; subsequent growth is bounded by charged applications/reconstruction and each grammar rule's finite output. Grammar-sized scans, hashing/shape keys, copies and serializing already accepted candidates are finite work within those reservations. Final FST sorting is `O(R log R)` on a result list whose construction has already spent at least one unit per result; dedup candidates also spend reservations. Consequently the allowance does not promise constant latency per unit or a portable millisecond bound.

Grammar loading, lexical index construction and cached matcher compilation precede per-word parsing. Their wall time must be measured separately. Caller-supplied selector callbacks are caller code; the allowance cannot preempt a callback that does not return. The optional deadline remains independent, and no new deadline is the deterministic-cap fix.

## Default work allowance and measured tradeoff

`--step-cap N|unbounded` counts morphological analysis attempts. `--work-cap N|unbounded` independently bounds the reservation sites above. Without a work flag, the effective allowance is `N.saturating_mul(100)`; unbounded analysis derives unbounded work. An explicit work flag overrides that default regardless of flag order. This scales the **configured** attempt allowance, not the attempts spent so far. The multiplier has one owner, `default_work_cap`, shared by parser and CLI default resolution.

At the integrator's 200,000 attempt cap, the default work cap is 20,000,000. The 94 completing diagnostic words in the supplied Windows logs used median 1,456,880.5, nearest-rank p90 8,290,898 and maximum 19,717,051 work units. Their median work/attempt ratio is 33.72 and maximum 2,378.13. A fixed 34-times-actual-attempts budget would therefore penalize precisely the expensive inner-search words. The chosen 100-times-configured-cap default covers the observed maximum work, with about 1.4% margin, while preserving the caller's separate attempt allowance.

At the reported 0.31 ms per 1,000 work units, 20 million units corresponds to roughly 6.2 seconds of search. This estimates containment latency for that host; grammar setup is additional and no constant-latency guarantee follows. Unbounded diagnostic runs also contain timeouts, excluded from the 94-word distribution. The observation is not a completeness proof for capped words or other grammars. Words above 200,000 attempts can still hit the original attempt cap, even if their full work is below 20 million.

The Windows integrator compared 400 Maasai sample words with an 8-second deadline and four workers: v0.6.1 took 558 s / 899 MB, with 232 timeouts and 73 words with analyses. The first shared-unit implementation took 6.7 s / 114 MB, with no timeouts but 366 caps and only 6 words with analyses. That recall regression is why the analysis-attempt unit is restored. Unbounded, that implementation completed 95 words, 55 with analyses. These are integrator measurements; raw logs stay external under `_briefs/logs/maasai-cap/`, without corpus data copied here. Revised independent-limit measurements are pending.

## Diagnostics and synthetic verification

`ParseOutcome.steps` and the `STEPS` line remain morphological analysis rule attempts, matching the attempt diagnostic. Ordinary synthesis spends only work. `work_steps`, rich trace `workSteps`, and the opt-in `WORK_STEPS` line report the work allowance's total. `CAP` and `ParseOutcome.capped` mean either deterministic cap fired, and remain incomplete even when earlier complete confirmations survive. The independent deadline reports `TIMEOUT`. Short invalid surfaces report their input reservations; oversized input can reach the work cap before allocation.

The synthetic gates distinguish mechanisms by effect: an ample work allowance lets a one-attempt-capped parse confirm its prior root; a work cap of 8 stops before the much larger attempt limit; an optional-stem synthesis and deterministic/nondeterministic optional matcher stop at exactly 64 work units with zero analysis attempts. Alternative and trie gates stop at 32 and 16 respectively. Unit tests pin `200000 -> 20000000`, saturation, unbounded defaults and independent cap exhaustion, including confirmation after attempt exhaustion. They justify that work containment acts independently of any unit conversion or deadline; they do not establish the empirical multiplier as a universal ratio.

CLI tests cover separate/equal flag syntax, unbounded overrides, zero refusal, and sequential/parallel effects. Stats tests pin identical-budget reuse and rejection of differing or older budget policies before TSV truncation. The effective work allowance and search-budget semantics version are included in the options identity; older caches must use a separate path.

The integrator reports a clean Windows check and 1,737 passing tests for the first implementation after removing two redundant function-level `must_use` attributes. Those fixes are included. All tests for the revised two-budget implementation require a fresh run; this jail's managed check stops before Cargo. See [divergence 052](../divergences/052-shared-search-work-cap.md) for coverage status.
