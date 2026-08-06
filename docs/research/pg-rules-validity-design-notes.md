# pg-rules validity.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-rules/src/validity.rs` implementation comments so
the source can carry a one-line pointer instead of the full argument. Each section corresponds to
one call site; the site names the function so this doc can be found from either direction.

## `allomorphs_valid_impl`: root-arm disjunctive re-check's omitted morpheme recheck

The W3.2 disjunctive loop for a root morph — C# `Allomorph.cs:127-152`'s
`disjunctiveAllomorph.CheckAllomorphConstraints(null, this, word)` — reads the passed-over
candidate's own `AllomorphCoOccurrenceRules`, but keys the check on the originally-used allomorph
(`this`, i.e. `m.allomorph`), not the candidate (`cand.id`). Morpheme-level co-occurrence rules are
not re-checked per candidate: every disjunctive alternative of a root morph shares that morph's one
morpheme, so the morpheme-level check would be the same set and the same key as the primary check
already performed just above the loop, and re-running it per candidate is provably a no-op. The
Rust port omits it for that reason, not by oversight. The affix arm's disjunctive loop follows the
same shape for the same reason.
