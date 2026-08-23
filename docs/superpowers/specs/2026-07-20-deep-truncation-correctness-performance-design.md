# Deep-truncation-chain correctness and performance completion design

Date: 2026-07-20  
Status: Approved for implementation

> **Historical/superseded acceptance scope.** The four-language performance design and its release
> gates are retained as provenance; they are not the current Indonesian/Amharic/Aweti shipping gate.

## Objective

Finish the planned Aweti work, measure current results for Sena, Indonesian,
Amharic, and Aweti, explain Aweti's runtime, and produce an evidence-backed
speedup plan. The work must preserve PanGloss's permanent propose-and-confirm
architecture and its 100% proposer-recall requirement.

## Current evidence

Main at `fa81ec8` contains the templated underlying emitter, composition
budgets, and dedicated-rule chain restriction. Chain restriction reduced the
Aweti network from 35,846 states and 800,354 arcs to 14,806 states and 270,541
arcs. It preserved the current composition recall floor of 68 of 104 oracle
words.

That floor does not prove correctness. Thirty-six oracle words still miss.
The bare root `mã` misses despite a byte-correct direct lexc entry and still
misses when all phonological rules are removed. The failure therefore occurs
before or within lexc compilation, composition, minimization, token handling,
or the recall intersection. The earlier truncation hypothesis is false for
Aweti: its prototype recovered no misses and made `parua` take more than 280
seconds. The implementation must not revive it.

Aweti's remaining runtime problem lies chiefly in `apply_up` path enumeration.
Compile and composition complete in about three seconds. `parua` appears as the
first raw result in less than one millisecond, while `ti` can enumerate two
million raw results in about 2.1 seconds without reaching the oracle analysis.
The acyclic network has finite but enormous ambiguity and poor result ordering.
Tag decoding may add cost, but it must process every raw path before candidate
deduplication, so path growth remains the leading hypothesis.

Phase C stage 2 exists in unmerged commit `bbb230c`. Its registered worktree
also contains accidental workspace-wide formatting changes. Integration must
review the commit from a clean base and must not merge the dirty worktree.

## Approach

The work proceeds correctness first, then performance, then complete
measurement. This order avoids optimizing a network whose language is still
wrong and prevents repeated four-language benchmark runs.

First, isolate the `mã` failure with paired minimal tests. Compare `mã` with a
nearby recalled bare root through lexc-only compilation, cleanup composition,
minimization, upper projection, tag intersection, and Unicode/token
normalization. The first failing boundary supplies the regression test and the
root-cause hypothesis. Implement one fix at that boundary, then require the
Aweti composition gate to retain every previously recalled word and improve on
68 of 104.

Second, measure `apply_up` rather than infer its cost. Add bounded diagnostic
timers and counters for raw paths, raw bytes, decoded paths, malformed paths,
unique candidates, traversal time, decode-and-deduplicate time, confirmation
groups, and confirmed analyses. Use `parua` as the fast control and probe `an`
and `ti` individually. Optimize only after the counters identify the dominant
stage. The preferred structural fix removes semantics-equivalent morphotactic
paths before traversal or canonicalizes duplicated chain choices. Candidate
and confirmed-analysis sets must remain exact.

Third, review `bbb230c` independently. Preserve its construct gates and honest
unsupported-mode handling when correct, but reject unrelated formatting churn.
Any change that reduces Aweti's supported-rule recall requires an explicit
decision and separate reporting; an honest skip cannot masquerade as a recall
improvement.

Finally, run fresh release gates for Sena, Indonesian, Amharic, and Aweti on an
idle machine. Record corpus scope, oracle denominator, recall or parity,
compile time, network size, lookup time, and total wall time. Historical numbers
may provide context but cannot substitute for this run.

## Components and boundaries

The correctness investigation stays in `pg-foma` tests and the smallest
production component shown to cause the failure. The runtime investigation
instruments the proposer, decoder, and confirm boundaries separately. It must
not fold full-engine oracle time into `apply_up` time.

The four-language report uses the existing release gates:
`f1_sena_gate`, `f2_indonesian_gate`, `f3_amharic_gate`, and
`p6_aweti_gate`. A durable results document records commands and raw log paths.
The follow-on plan ranks speedups by measured payoff, correctness risk, and
implementation cost.

## Safety and failure handling

Run one heavy process at a time in release mode. Keep the oracle step cap at
20,000. Give every Aweti command an external wall-clock watchdog that kills
descendants. Cap general `apply_up` probes at 50,000 raw results; use a larger
cap only for a named, bounded reproduction. Never raise Aweti's enumeration or
pre-expansion budgets. Never run an uncapped Morpher on Aweti.

A timeout, killed process, or missing oracle result is unmeasured evidence, not
a negative score. A speedup fails if it loses a previously recalled candidate,
changes the confirmed analysis multiset, hides an unsupported construct, or
lowers a safety bound.

## Test strategy

Every behavior change follows red-green-refactor. A minimal test must fail for
the observed boundary before production code changes. Focused tests then cover
the fix, Unicode/token edge cases, candidate-set equality, and bounded failure.
The Aweti 68-of-104 baseline and its recalled-word set form a no-regression
gate, not the completion target.

Fresh verification runs the relevant `pg-foma` unit tests, construct gates,
all four language gates, and bounded Aweti diagnostic probes. The results
document must distinguish composition recall, enumerated candidate recall,
confirmed parity, and oracle exclusions.

## Deliverables and completion criteria

Completion requires reviewed implementations of the still-valid planned
fixes, a root-cause-backed improvement to Aweti correctness, and measured
performance evidence. It also requires a fresh four-language results table,
raw command logs, an explanation of Aweti's dominant costs, and a prioritized
speedup plan.

The work is complete only when all focused and regression tests pass, every
previously recalled Aweti word remains recalled, all four language results have
explicit denominators, and the plan maps each recommendation to measured
evidence and a verification gate. If Aweti remains below 100% recall, the final
report must keep the goal open and name each unresolved miss class.
