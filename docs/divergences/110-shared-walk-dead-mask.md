# 110: The shared template walk carries a per-template dead mask for the required-slot look-ahead

Kind: optimization (argued parse-preserving).
Status: optimized-in-rust. Machine PR #577 (open, branch `perf/hc-lookahead-on-546`, top commit `bbb81a00`, "Closes #566") is the C# change; the shared trie itself is entry 095 (Machine #546, already ported).

## C# site

`AnalysisAffixTemplatesRule.cs` (`Walk`, `WalkState`, `AnyLive`) on Machine `perf/hc-lookahead-on-546`;
`AnalysisAffixTemplateRule.RequiredSlotCannotUnapply` is the per-template test of entry 109.

## Rust site

`pg-rules/src/stratum.rs` (`walk_slot_trie`, `TrieWalk`, `SlotTrieNode::depth`) and
`pg-rules/src/stratum/required_slot_check.rs` (`shared_look_ahead`). Tracing keeps the per-template walk.

## What changed

Entry 109 made the untraced battery walk each template on its own. The shared walk now runs the
look-ahead at every trie node for every admitted template that passes through it, on the word that
reached the node, from slot `slot_count - 1 - depth` downwards. A template that fails is marked dead
for the subtree below the node while the templates sharing the node go on; a child whose templates are
all dead is skipped before its slot batch runs. The mark is one flag per template per call, set on
entry to a node and undone on leaving it (no cache). Shadow mode carries each template's fired slots
down the walk and counts `violated` and `completed_under_fired` per template, as in 109.

## Why it cannot change a parse

A dead template cannot complete (entry 109's argument, applied to that template's own slots: the node
holds exactly the slots that template has walked, and the slots after it are its own). So each
template's outputs equal those of the shared walk without the look-ahead, and the shared walk equals
the per-template walk (entry 095). Marks are undone on return, so a template dead on one word is not
dead for the word an optional skip or another rule output carries into a sibling branch
(`a_template_dead_in_one_branch_is_alive_in_the_branch_that_skips_the_slot` fails without the undo).

## Evidence

Shadow: 0 violated and 0 completed under a fired check on every conformance fixture
(`required_slot_shadow_gate`) and the five reference corpora. `work_steps` per corpus is in the slice
report (`result-pg-s2-template-walk.md`).

## Coverage

`pg-rules/src/stratum/required_slot_lookahead_tests.rs`:
`the_shared_walk_gives_each_template_its_own_walks_outputs_with_the_look_ahead` (shared on, shared off
and per-template on agree for every template and every word up to four segments);
`a_dead_template_stops_collecting_while_its_siblings_share_the_walk_on` (fewer `work_steps`, siblings
still complete); `shadow_on_the_shared_walk_counts_fired_checks_and_no_violation`.
