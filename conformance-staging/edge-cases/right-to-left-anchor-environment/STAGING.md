# STAGING: right-to-left-anchor-environment

## Why this fixture exists

`openspec/changes/plan-construct-coverage-completion` task 4.2 extends
`crate::replace::compile_rtl_branch_net`'s reversal-plus-safety-net-union construction to pattern
shapes `crate::replace::pattern_slots` used to refuse unconditionally, for EVERY rewrite-rule
compile (RTL or not): `PatternNode::Segments` and `PatternNode::Anchor`. This fixture pins the
`Anchor` shape specifically — a `Dir::RightToLeft` rewrite rule whose subrule's own right
environment is JUST `finalBoundaryCondition="true"` (an inline word-boundary condition, no other
context node at all).

1. **The structural characterization.** `pg_foma::capability::rtl_reversal_diagnosis`
   (`RightToLeftRewriteDetail::reversal_construction_attempted`) now finds this shape SUPPORTED —
   `true`, where before task 4.2 it was `false` (a bare `Anchor` node in an environment refused
   `pattern_slots` unconditionally, `crate::lower::UnsupportedPatternNode::Anchor`'s own doc).
2. **The capability gate's own verdict.** `RightToLeftRewriteFaithfulReversalPredicate`
   (`right-to-left-rewrite.faithful-reversal-construction`) returns `ConfirmOnly` for this grammar
   — NOT `Refuse`.
3. **The oracle's own correct, anchor-aware behavior.** `pg_parse::Morpher` correctly restricts the
   obligatory rewrite to the WORD-FINAL occurrence of the LHS class only (see "What it pins" below).
4. **The reversal construction genuinely swaps the anchor to the correct edge, not just "compiles
   without crashing."** `Anchor(Right)` — the LAST slot of the original `right_env` — becomes, via
   the EXISTING (unmodified) `reversed_slots` + left/right swap, the FIRST slot of the mirror rule's
   own `left_env`; `fsm_reverse` then turns "start of the mirror/reversed representation" back into
   "end of the real string" for the final network. This is pinned directly at the automaton level,
   independent of any grammar/oracle, by `rust/crates/pg-foma/tests/phase_c_right_to_left.rs`'s own
   `rtl_anchor_reversal_swaps_the_correct_edge` — the test that would fail if the two anchors were
   NOT correctly swapped (it would rewrite the word-INITIAL occurrence instead of the word-final
   one).

## What it pins

- `aae`/`ae`: ROOT1/ROOT2's own correctly-rewritten surface forms ("aaa"/"aa" underlying,
  word-final "a" rewritten to "e", every other occurrence unchanged).
- `aaa`/`aa`: **`expect_fail: true`** each — the roots' own RAW, un-rewritten underlying shapes,
  queried directly. Since the rule is obligatory wherever its environment matches (the word-final
  "a" always satisfies `finalBoundaryCondition`), these strings are NOT valid surface forms for
  their respective roots at all — proving the rule genuinely fires, not vacuously inapplicable.
- `eaa`/`aea`: **`expect_fail: true`** each — structurally-invalid variants of ROOT1's own "aaa"
  where the FIRST or MIDDLE "a" was (wrongly) rewritten instead of the true word-final one. Neither
  is a valid surface form for ROOT1 (the only correct rewrite is "aae") — a load-bearing negative
  control that the environment genuinely discriminates POSITION, not merely "some 'a' rewrites
  somewhere."

## Oracle discipline

**Oracle: `pangloss` (this repo's own Rust engine), NOT the C# founding oracle.** Authored fresh for
this task; `words.yaml` signatures captured by driving `pg_parse::Morpher::parse_word` directly over
every word (a throwaway test, deleted after transcription — see "Verification").

## Verification

Signatures were captured via a throwaway test
(`rust/crates/pg-parse/tests/zz_throwaway_sig_dump.rs`, deleted after transcription) driving
`pg_parse::Morpher::parse_word` directly over every word in `words.yaml`, using the SAME grammar
this directory's `grammar.xml` ships. Cross-checked in-repo by
`rust/crates/pg-parse/tests/conformance_fixtures_gate.rs`'s `all_discovered_fixtures_match_oracle`
test (dual-root discovery, default `cargo test --workspace` suite) — that test is what actually
gates CI. The capability-gate `ConfirmOnly` verdict is additionally pinned directly by
`rust/crates/pg-foma/src/capability.rs`'s own
`right_to_left_predicate_confirm_only_for_anchor_shaped_rule` unit test, and the reversal
construction's own edge-swap correctness by `rust/crates/pg-foma/tests/phase_c_right_to_left.rs`'s
`rtl_anchor_reversal_swaps_the_correct_edge`/`rtl_anchor_fixture_matches_oracle_and_differs_from_
left_to_right`.

## Graduation

Not yet proposed upstream. Candidate destination:
`machine/conformance/edge-cases/right-to-left-anchor-environment/`. On acceptance, delete this
staged copy in the same change (graduation guard enforces this mechanically).
