# 105: Lexical-lookup alternatives are replayed against stem-name and MPR gates before synthesis

Kind: efficiency.
Status: optimized-in-rust. Same parses; machine PR #570 (`perf/hc-lookup-replay-pr`) is open, so no accepted Machine change is claimed.
Evidence: `pg-parse/tests/lookup_replay_gate.rs`; the oracle conformance gate with the replay on; the ignored corpus measurement in `pg-parse/tests/edge_prefilter_gate.rs`.

## C# site

`Morpher.IsPrunedByLookupReplay` and `LookupReplay.IsMprFeasible`, applied to each alternative after the maximum-alternatives count and before `_synthesisRule.Apply`.

## Rust site

`pg-parse/src/lookup_replay.rs` (`LookupReplay::is_feasible`), called per expanded alternative in `Morpher::parse_word_core_selected`. `Morpher::with_lookup_replay(false)` switches it off for differential runs. The check is skipped while tracing, so a traced parse still shows every rejected synthesis. Roots in a family, guessed roots and supplied roots are never rejected.

## Why it cannot change a parse

It rejects only when a rule on the alternative's trail can never apply: the rule's required stem name differs from the root's, or no allomorph's required MPR features can hold, or no allomorph's excluded MPR features can hold. The required check uses a superset of the real MPR set (root features, every earlier trail output and every realizational output, ignoring removals). The excluded check uses a subset (the root's features) and runs only when no output can reach an overwrite group and no compounding or unknown entry is on the trail. Compounding and unknown entries switch off the excluded and stem-name checks.

PanGloss records no allomorph per trail entry, so every rule is checked against all its allomorphs; C# can use the recorded one. The replay stores nothing between alternatives and builds only three MPR sets from the grammar. Template-state and final-state replay is not ported (machine dropped it on cost).

## Measured effect

`work_steps` on the first 100 words of each reference corpus (words that hit a 5M cap or fail to segment are left out of both arms; analyses identical for every compared word), off then on, with the replay's own feature-set operations: Indonesian 1,641,756 to 1,641,756 (0 operations); Aweti 26,365,993 to 25,065,504 (-4.9%, 28,487 operations); Amharic 6,880,683 to 6,880,683 (0); Mbugwe 15,865,920 to 15,835,750 (-0.2%, 10,192 operations); Sena 2,400,890 to 2,400,890 (0). Not a loss on any real grammar, so it stays on. PanGloss PR #13 (`df2e1f14`, analysis-selected allomorph filtering) restricts which allomorph synthesis tries; it does not drop alternatives before synthesis, so the two do not overlap.
