# 095: The untraced affix-template battery runs a slot batch once for every template that shares it

Kind: optimization (argued parse-preserving).
Status: optimized-in-rust. Not proposed upstream; see C# transfer below.

## C# site

`AnalysisAffixTemplateRule.cs` (`Apply`, `ApplySlots`) and `AnalysisStratumRule.cs`, which applies
every template of the stratum to the same input word. Each template owns its own `RuleBatch` per
slot and walks its slots from the last one down, so two templates that end in the same slots run the
same rule batch on the same word once each.

## Rust site

`pg-rules/src/stratum.rs`: `run_template_batch`, `shared_slot_battery`, `walk_slot_trie`,
`SlotTrie`, `template_admits`. Tracing keeps the per-template walk (`analyze_template`,
`template_unapply_slots`), which emits per-template begin/end events and slot outcomes.

## What changed

The untraced battery builds, once per stratum analyzer, a trie of the templates' slots in walk order
(last slot first). Two templates share a trie node only while every slot walked so far has the same
rule list, in the same order, and the same optionality (`slot_optional`). The walk runs each
admitted template's admission gates first, in battery order, then walks the trie depth first: at a
node it emits the word into every admitted template that ends there, and for each child that some
admitted template passes through it charges one work unit, runs `apply_slot_batch` once, recurses
into the child for every output, and, if the slot is optional, continues into the child with the
unchanged word. Per-template outputs are then merged in battery order exactly as before.

## Why it cannot change a parse

`apply_slot_batch(slot, word)` depends only on the slot's rule list and the word (plus the shared
budgets); `apply_one_mrule` reads no template identity. Templates below a trie node have walked
identical slots, so they reach that node with identical words in an identical order. For each
template, the shared walk therefore performs the same sequence of slot batches, recursions, skips
and emissions as its own `template_unapply_slots`, and inserts the same words into its own
`FxHashMap` in the same order, so `into_values` yields the same sequence. Admission gates and
their stats run per template as before. The only difference is that a batch serving `k` templates
runs once instead of `k` times.

Budgets: work and step consumption can only fall (each shared visit replaces one or more
per-template visits). A word that was capped can therefore complete; a word that completed still
completes with the same result. Capped rows are excluded from parity claims.

Memory: all templates' outputs are held until the merge, instead of one template's at a time; this
is bounded by the battery's own output.

## Evidence

`pangloss batch --threads 1 --step-cap 2000000` (Aweti 500000), work units = summed per-word
`WORK_STEPS`, over words complete in both runs; parity by `rust/tools/parse_compare.py`.

| grammar | words compared | parse_compare | less work | fewer steps | fire count |
|---|---|---|---|---|---|
| Sena | 6,146 | IDENTICAL 6,146 | 11.5% | 31.6% | 171,319 shared batches, 448,573 avoided |
| Mbugwe (1,408 of 1,645 words run) | 1,364 | IDENTICAL 1,364 | 18.1% | 19.3% | not printed (run stopped) |
| Aweti | 170 (36 capped in both) | IDENTICAL 170 | 0.35% | 0.16% | 1,939 shared, 2,159 avoided |
| Amharic, Indonesian | 673, 70 | IDENTICAL | 0 | 0 | 0 |
| a private featureless project, three grammar variants plus its C# HC XML export, three word lists each | 563 / 7,206 / 200 per list | IDENTICAL | 0 | 0 | 0 (one template) |

The 448,573 avoided batches on Sena equal, to the unit, the ceiling a measurement-only probe counted
beforehand (same slot suffix, same full word state, earlier template). On Mbugwe about 20 words that
the per-template walk left capped now complete; they are covered by the argument above, not by a
comparison. `-Mode conformance-test -Scope all`: 2043 passed. Against the Machine PR 480 head
fixtures the failure set is the baseline's (the duplicate-name guard and the five
`clitic-rule-designation` words).

## C# transfer

`AnalysisAffixTemplateRule` has the same structure (one `RuleBatch` per slot per template, walked
from the last slot), so the trie ports directly to its sequential `ApplySlots`; it would also let
templates share the compiled slot rules. `ParallelApplySlots` is a layered breadth-first walk and
would need its own design. `MemoizedCombinationRuleCascade` memoizes cascade subtrees by
`AnalysisStateKey`, which reduces how often a battery runs but not the repeated slot batches inside
one battery, so the two compose. The C# gain is unmeasured.

## Coverage

`pg-rules/src/stratum/template_slot_sharing_tests.rs`:
`shared_walk_matches_every_templates_own_walk_in_order` compares the shared walk against every
template's own walk, in order, for every word up to five segments with and without a POS
(fails when slot optionality is dropped from the trie identity);
`untraced_battery_runs_a_shared_slot_batch_once` pins the fire count and that every avoided batch is
one the per-template walk ran (fails with sharing disabled).
