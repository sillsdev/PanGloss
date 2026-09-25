# 049 — Copy-agreement pruning of reduplication matches

Kind: efficiency
Status: optimized-in-rust (mirrors [Machine PR #519](https://github.com/sillsdev/machine/pull/519), open)

## Sites

- C# site: `AnalysisMorphologicalTransform.ClassifyRepeatedPartCopies`, `CopyAgreementPatternRule`, `Morpher.PruneDisagreeingCopies` (PR #519, on by default).
- Rust site: `pg-rules/src/morph.rs::copy_agreement_refuses_match` and `ana_allomorph_matches`; `pg-rules/src/stratum.rs::AnalyzerConfig::prune_disagreeing_copies`; `pg-parse/src/morpher.rs::Morpher::with_prune_disagreeing_copies`.

## Difference

Unapplying an affix-process allomorph whose output copies an input part two or more times matches each copy as an independent capture, and only the first capture rebuilds the base. Both engines used to keep every split and let synthesis reject the ones whose copies differ. With pruning on (the default in both), a match is skipped when two copies of an unmodified part differ in segment count or have a segment pair that does not unify.

## Why no parse can change

Synthesis writes every copy of a part from the same input, and every later change to one copy (phonology, outer affixes) is unapplied before this rule, so a non-unifiable copy pair cannot synthesize the surface. A part is never pruned when the rule modifies it, a capture failed or is zero-width, or a copy contains an optional node (an unapplied deletion), including an optional run before the capture that reaches the start of the shape. Rust compares every copy pair where C# compares each against the first; with three or more copies Rust may prune more, but only matches that synthesis would reject.

## Evidence

- `pg-parse/tests/csharp_port_affix_process.rs`: `full_copy_keeps_only_the_split_whose_copies_agree`, `disagreeing_copies_are_removed_only_when_pruning`, `unifiable_distinct_segments_survive_copy_agreement_pruning`, `optional_left_prefix_keeps_copy_agreement_undecidable`, and prune-off/on variants of `reduplication_rules` and `modify_from_input_rules`. Mutants: never-prune fails the disagreeing-copies test; pruning undecidable copies fails the optional-prefix test; pruning everything fails five tests. `pg-rules/tests/stratum_gate.rs::copy_agreement_pruning_is_on_by_default` pins the default.
- Conformance at Machine `34215889` adds reduplication x phonology words (`suffixing-extension-slot-ordering`: `pambam`; `metathesis-phase-isolation`: `hasaasa`, `haasa`, `titula`, `hiasa` and negatives). `-Mode conformance-test -Scope all` passes 2,725/2,725 with pruning on.
- Release comparison, v0.3.3 vs v0.4.0 (`machine/scratchpad/review-0923/bench040`): identical analyses and unavailable lists on all 7,455 words completed by both across the five reference grammars. Aweti: 172 -> 191 completed within 20 s, 5.3x on the 172 shared words, word-list index 182 (Maxwell export) 60 s timeout -> 2.7 s. Sena, Amharic and Mbugwe measured 4-12% slower; see follow-up.

## Follow-up

The v0.4.0 path builds a `HashMap` of repeated parts for every affix allomorph match even when no part repeats, which is the suspected cause of the slowdown on grammars with no copy rules. An allocation-free guard is in progress on `perf/copy-prune-no-alloc`.
