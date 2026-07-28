# Task 2 bare-root boundary diagnosis

Status: retrospective durable record backed by contemporaneous commit `f892cfd`.
That commit records the RED test failing specifically at sigma membership before
the fix, its GREEN result afterward, the strict-subset miss comparison, and the
independent verification commands/counts.

## First failing boundary

`apply_up` found the target root tag at every pipeline stage: lexc alone,
after rules, after cleanup, and after minimization. The failure first appeared
in the recall harness's word-restrict → project → `fsm_intersect` path: the
upper network sigma omitted the atomic tag symbol even though the relation
contained the tag.

The discriminating pattern was a literal `0` in zero-padded tag numerals.
Sampled misses included `mã` (morpheme 400), `ma` (69), `nã` (106), and
`tonoly` (62); recalled controls included `ta` (894), `me`/`ne` (897), and
`kitã` (395). The foma lexc tokenizer decomposed multichar tag names containing
literal zero digits, leaving incomplete sigma bookkeeping that made
`fsm_intersect` lose otherwise valid paths.

## Rejected hypothesis

Combining-mark tokenization was not causal. `mã` is one precomposed segment,
and the combining-mark-bearing control `kitã` recalled successfully.

## Reduced fix and executable evidence

`tags.rs` encodes zero digits with a reversible non-zero glyph so emitted tag
names remain atomic. Test `d_bare_root_tag_atomicity_boundary` pins the exact
consumer boundary. The current release gate additionally enforces 100/106 and
the exact six-word miss set, preventing the historical 32-recall floor from
masking a regression.
