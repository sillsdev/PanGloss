# 052 — Independent work allowance includes inner search and synthesis confirmation

Kind: behavioural
Status: open

## C# site

Machine revision `a4b29742b6274a01c7398ca3e800a19fa6d2c9aa`:

- `SIL.Machine/FiniteState/TraversalMethodBase.cs`: `Initialize`, `Advance`, `CheckAccepting`.
- `SIL.Machine.Morphology.HermitCrab/Word.cs`: `ExpandAlternatives`.
- `SIL.Machine.Morphology.HermitCrab/Morpher.cs`: constructor, `SynthesizeSequential`, `SynthesizeParallel`.
- FieldWorks revision `089eb9027b6d81be7883c40960f0de3ffa04b699`, `Src/LexText/ParserCore/HCParser.cs`: parser parameter loading.

## Rust site

- `pg-fst/src/work.rs`: shared work allowance and synchronous thread-local scope.
- `pg-fst/src/traverse.rs`: optional continuation construction, starts, arcs, captures and results.
- `pg-rules/src/stratum.rs`: `StepBudget`, analysis and synthesis orchestration.
- `pg-rules/src/word.rs`: recursive alternative expansion.
- `pg-rules/src/cascade.rs`, `morph.rs`, `rewrite.rs`, `validity.rs`: branch and candidate enumeration.
- `pg-parse/src/morpher.rs`, `root_trie.rs`, `overlay.rs`, `guess.rs`, `surface.rs`: word entry, lookup, confirmation and guessing.

## Difference

This is **resource containment**. Rust already exposes an incomplete `CAP` outcome, while HC has no equivalent deterministic per-word work allowance. The old Rust allowance counted morphological analysis attempts; ordinary synthesis checked only a deadline. One analysis attempt could enumerate arbitrarily many FST continuations before the outer counter advanced, and confirmation could do so without spending the allowance at all.

A separate work allowance now covers all those search phases; the analysis-attempt cap retains its original unit. It is reserved before exploring a branch or materializing a candidate, and never exceeds the configured cap. Exhaustion is latched, incomplete matcher results are dropped, and an interrupted confirmation cannot be accepted as a completed parse. Completed confirmations from before exhaustion may remain in an explicitly capped outcome. No readiness or grammar-admission decision is added.

The first implementation reused the attempt limit for inner work. Windows integration found a recall regression (6 words with analyses instead of 73 out of 400), despite clean check and 1,737 passing tests after two clippy fixes. The corrected interface preserves `--step-cap` and adds `--work-cap`; its omitted default is 100 times the configured attempt cap, saturating. `ParseOutcome.steps` counts analysis attempts and `work_steps` counts independent work. Analysis stops on either limit, while confirmation can use remaining work after the attempt cap fires. A work cap can still reduce attempted analysis, but now it has a distinct configured limit. Revised compatibility and containment tests are pending execution.

HC `Advance` recursively explores optional annotations and clones instances with no work allowance. `Initialize` recursively constructs optional starts. `Word.ExpandAlternatives` eagerly builds an `IList<Word>` recursively. HC's `MaxAlternatives` defaults to zero and is tested by the synthesis caller after that list has been built; a positive value does not contain the construction itself. FieldWorks reads `MaxRoots` and `MaxAlternatives`, defaulting the latter to zero. `MaxStemCount` and the existing iterative epenthesis shape limit do not bound these continuation lists.

These are source findings. The same Maasai runtime failure has **not** been reproduced in C#. There is no new shared Machine issue or upstream PR associated with this entry; it does not assert a different correct grammar interpretation. The entry remains open for execution evidence and bounded-run compatibility, rather than claiming an oracle-verified fix.

## Bounds and limitations

One work allowance covers input characters, cascade decisions, template slots, allomorph/subrule choices, optional reconstruction, matcher starts/arcs/recursive continuations/commands/accepts, result dedup comparisons, trie choices, alternatives, lexical candidates, environment checks and surface/guess branches. See [the counting contract](../research/per-word-search-work-cap.md) for reservation sites and costs outside branching.

Grammar loading and matcher compilation are outside the per-word cap. A work unit is not a time or byte bound: shape/feature copies depend on the finite input and grammar, and final sorting depends on the already bounded candidate set. A deadline remains an independent containment control. Neither control proves completeness after it fires.

## Evidence and regression coverage

The existing v0.6.1 release binary was measured against the external FieldWorks sample, without copying its data into this repository. A native stack sample for `enkerai --step-cap 200000` caught `Transduce::advance` beneath `synth_process_allomorph` and the guided synthesis cascade. For `olkiteng`, a batch separates approximately 2.001 s of loading from 358 ms of search: 200,000 outer analysis attempts caused 646,630 FST calls, averaging about 0.332 microseconds per call. The unbounded confirmation phase, rather than a costly individual attempt, explains the first word; loading plus many cheap attempts explains much of the second word's wall time.

Synthetic regression code is present:

- `pg-fst/tests/fst.rs`: `optional_annotation_expansion_is_bounded_inside_one_match` asserts exhaustion at exactly 64 units for deterministic and nondeterministic traversal; `a_complete_optional_match_keeps_its_captures_and_order` compares a generous allowance with standalone traversal.
- `pg-rules/tests/stratum_gate.rs`: `synthesis_optional_stem_paths_share_the_per_word_allowance` builds a synthetic suffix rule and optional stem, asserting no completed confirmation, exactly 64 work units and zero legacy analysis attempts.
- `pg-rules/src/word/tests.rs`: `shared_alternative_tree_cannot_expand_past_the_search_cap` asserts exactly 32 units.
- `pg-parse/src/root_trie/tests.rs`: `optional_root_lookup_cannot_scan_past_the_shared_search_cap` asserts exactly 16 units and compares with a nonempty uncapped control.
- `pg-parse/tests/step_cap_work_gate.rs`, registered in `timing`: capped parse/stats agreement, ordinary-word output and attempt compatibility, per-word reset, oversized input and invalid-input counters.
- `pg-fst/src/work/tests.rs`: exact reservation bound, nested restoration during unwinding, thread isolation and independent deadline behavior.

**Fixture presence:** all new test data is synthetic; no Maasai fixture was added. **Demonstrated coverage:** the integrator reports 1,737 passing tests for the first shared-unit implementation, which nevertheless caused a measured recall regression. The independent-cap revision and new tests are pending execution. The managed check exits 21 before Cargo because this jail has no finite visible `memory.max`; no locally built binary, revised test pass or revised after timing is claimed. The external lane report records the raw baseline artifacts and Windows follow-up commands.

## Compatibility correction

The 94 completing Windows diagnostic rows used at most 19,717,051 work units; `--step-cap 200000` therefore derives 20 million work units. The measured 0.31 ms / 1,000 units predicts roughly 6.2 seconds of search on that host, excluding loading. Median work/attempt ratio is about 34, but maximum exceeds 2,000; the factor applies to the configured cap, never the word's actual attempts. This is a configurable containment default, not a semantic completeness threshold. The research document records distributions and limitations.

Added synthetic tests pin independent attempt/work exhaustion and successful confirmation of a prior root after an attempt cap, plus CLI flag effects and stats cache reuse/refusal. Existing synthesis-only helper and classification limits remain explicit work bounds. The redundant function `must_use` attributes on `enter` are removed while the scope type retains its attribute. CLI help and `CHANGELOG.md` document both flags and counts.
