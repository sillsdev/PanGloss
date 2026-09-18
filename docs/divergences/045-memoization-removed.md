# 045 — Analysis memoization removed from HC-Rust; C# retains it

## Kind
Behavioural under a budget. The memo is an efficiency mechanism, but it saves *steps*, and under a
`--step-cap` the step count decides whether a word completes. Removing it therefore changes which
words return a result on a pathological grammar — see "The cost" below. With no cap, removal is
efficiency-only: the parse set is identical.

## Status
Removed in Rust, deliberate, reported upstream. Supersedes 031 and 032; moots 023 and 025.

## C# site
`AnalysisScope.cs`, `AnalysisStateKey.cs`, `AnalysisStratumRule` / `AnalysisAffixTemplatesRule`.
C# HermitCrab still memoizes; nothing here asks it to stop without its own measurement.

## Rust site
The `pg-memo` crate is deleted. `pg-rules/src/stratum.rs` calls `Cascade::combination` and
`run_template_batch` directly, with no scope threaded through the entry points (which lost their
`_scoped` infix along with the parameter). `Morpher::with_memo`, `--memo=on|off`, the `HC_MEMO_*`
knobs and `AnalysisPolicy::memo` are gone.

`AnalysisStateKey` and `MorphHistoryKey` survived the crate, in
`pg-rules/src/analysis_state_key.rs`: `AnalyzerConfig::merge_equivalent`
(C# `Morpher.MergeEquivalentAnalyses`) keys its fold on them, which is semantics, not caching.
Now that they no longer cross a crate boundary, `status` and `state` are typed as `MorphStatus` and
`FinalTemplateState` rather than opaque `u8`s.

## Why
Measured on HC-Rust, not on `hc.dll`.

**The memo's value collapsed at one commit**, and it is the port of upstream PR #494 + #493
(analysis `Add`→`PriorityUnion`, state-keyed `MergeEquivalentAnalyses`). Bisected on the memo's own
value — `t_memo_off / t_memo_on`, paired and interleaved in the same binary, 300 Sena words,
uncapped, single-threaded, five pairs per point:

| revision | ratio | spread |
|---|---:|---|
| `b37e907e` (parent) | **1.471** | 1.412 – 1.549 |
| `5f06e428` (PR 494/493 port) | **1.001** | 0.932 – 1.036 |

A step probe across the same boundary, which does not depend on wall clock:

| | memo on | memo off | steps the memo avoided |
|---|---:|---:|---:|
| before | 712,547 | 1,485,331 | **772,784** |
| after | 436,608 | 502,150 | **65,542** |

A 91.5% reduction in the work gap the cache exists to close. The analysis changes *remove* the
redundant work rather than making it cheaper to redo, so there is little left to cache.

**What remains does not pay for the memo's own cost.** `state_key` clones a `Shape`, two
`FeatureStruct`s and a rule-count map on every lookup, hit or miss. On Aweti (44 words,
200,000-step cap) the memo was a net cost: 21,262 ms / 362.7 MB peak with it, against
9,683 ms / 43.4 MB without. Every capped word burns exactly 200,000 steps by definition, so those
are comparable work-for-work: 1,136 ms per capped word with the memo against 363 ms without —
each step about 3x dearer. On Sena (300 words, uncapped) it was worth 2.6% of wall clock and
nothing at all in peak memory (identical to one byte).

Ten other candidate commits were measured and none reproduces the effect. Full method and raw
numbers: `docs/research/memo-sena-bisect.md`, `memo-is-a-net-cost.md`,
`memo-entry-work-value.md`, `memo-eviction-tuning.md`, `memory-measurement-repair.md`.

## The cost, which is not zero
The memo saves steps, so under a step cap it is what let some words finish at all. **8 of the 44
Aweti words completed only with it on** — the count `memo_corpus_gate` reported as
`completed only on=8`. After this removal those 8 words hit the cap and return a partial result,
and there is no longer a flag to get them back. This is the price of the change, recorded rather
than buried; anyone weighing the same change in C# under a step or time budget should price it too.

Mbugwe uncapped does not terminate at all (killed at 11,016 CPU-seconds), memo or no memo — step
caps are load-bearing on that grammar independently of this entry.

## Evidence
`-Mode test` and `-Mode conformance-test -Scope all` on the removal commit. The three gates that
existed only to compare memo-on against memo-off — `pg-rules/tests/memo_gate.rs`,
`pg-parse/tests/memo_parity_gate.rs`, `pg-foma/tests/memo_corpus_gate.rs` — were **deleted rather
than adjusted**: with one execution strategy left they can only assert a tautology. The three
`csharp_port_morpher.rs` cases that had been re-pointed at a memo-on/off comparison now compare two
independent `Morpher`s over one grammar, which is what the C# `MorpherTests` originals assert.

## Upstream
Reported as [#509](https://github.com/sillsdev/machine/issues/509), proposing the same removal for
C#, with the bisect and the 8-word cost stated. Related: [#485](https://github.com/sillsdev/machine/issues/485) (the performance
investigation this grew out of), [#456](https://github.com/sillsdev/machine/pull/456) (the memo
itself), [#494](https://github.com/sillsdev/machine/pull/494) and
[#493](https://github.com/sillsdev/machine/pull/493) (the changes that absorbed its gains).

**No correctness bug is claimed against C#.** #493 merged after #485 was filed and #494 is still
open, so the Rust bracket applied both together and cannot isolate them. The upstream claim is a
falsifiable prediction, not a measurement of C#: re-running #485's Mbugwe case on current `master`
should already show under 6%, and merging #494 should take it to approximately zero.
