# pg-parse simultaneous_conformance.rs: fixture findings

Conformance replay for the 4 oracle-verified `RewriteMode::Simultaneous` fixtures under
`rust/conformance/rewrite/simultaneous-*/`, following `rewrite_conformance.rs`'s convention: load
each fixture's `grammar.xml` as authored, parse every word in `words.txt`, and check
`Morpher::parse_word(...).signature()` against the literal signature transcribed from that
fixture's oracle-generated `expected.tsv`. Each fixture's own README documents the oracle-generating
command and the full derivation of every expected value.

All tests are `#[ignore]`d because `rust/conformance/` is not yet pulled into PanGloss as a
submodule; `have_fixture` guards each test so `--include-ignored` runs do not panic on the missing
directory once it does land.

## `simultaneous-feeding`

Direct port of `RewriteRuleTests.MultipleApplicationRules`, tagged Simultaneous. Proves the
headline algorithmic fact: a rewrite under `Simultaneous` computes every match's target+environment
against one fixed pre-rewrite snapshot, so it can never feed another match within the same
application — `"gigugu"` parses (both `u`s' left environments are checked against the original,
unrewritten shape), `"gigugi"` does not (the rule is obligatory wherever its environment holds, so
the un-rewritten form itself never survives).

## `simultaneous-feeding-control-iterative`

The identical rule with `multipleApplicationOrder` omitted (Iterative, C#'s default) — the
mirror-image oracle run. Iterative's cursor re-matches against the shape as mutated so far, so the
first rewrite (which turns the second `u`'s preceding environment from `i` to `u`, no longer the
triggering class) bleeds the second match — `"gigugi"` parses, `"gigugu"` does not. Together with
`simultaneous-feeding`, this is the primary, cleanest, highest-confidence pin of the whole
Simultaneous-vs-Iterative divergence — a real C# unit test transcribed, not a hand-invented scenario.

## `simultaneous-epenthesis`

Direct port of `RewriteRuleTests.EpenthesisRules` sub-case (1): insert an HFU vowel after any high
vowel, tagged Simultaneous, against a real morpheme-boundary-bearing root shape (`"b+ubu"`).

Its `expected.tsv` deliberately freezes the traced/correct signature for `"buibui"` (`|b+?uibui`),
not the live C# oracle's default (non-tracing) path's output, which is confirmed buggy (`-`) via
three independent checks: the real NUnit test passes non-traced; a from-scratch in-memory
reconstruction succeeds non-traced; the same loaded grammar object flips from 0 to 1 result purely
on `TraceManager.IsTracing`. The bug is in C#'s own nogood-memoization cache (`AnalysisScope`,
installed only when not tracing), not in this fixture's construction.

## Memo-cache soundness against the confirmed C# bug shape

C#'s nogood-cache bug is specific to a repeat-until-fixpoint reapplication loop (`SelfOpaquing`)
interacting unsoundly with memoization. Rust's analysis path has its own, unrelated memo cache
(`Morpher::with_memo`/`pg-memo`) — the `self_opaquing` repeat-wrapper needs testing against this
exact fixture shape, since the shape of the risk (a repeat-until-fixpoint loop plus a memo cache) is
precise enough to test for directly even without knowing C#'s exact trigger mechanism.

**Result: sound, with one caveat.** Parsing `"buibui"` through this fixture with Rust's memo cache
on (`Morpher::new`'s default) and off (`with_memo(false)`) gives the identical signature either way
(`|b+?uibui`). Caveat, confirmed via temporary instrumentation then reverted: on this fixture the
`self_opaquing` `while` loop around `ana_epenthesis` in both `analyze` and `analyze_cached`
(`pg-rules/src/rewrite.rs`) runs its body exactly once for `"buibui"` under both memo settings — it
does not actually reapply to a fixpoint here. So this test is solid evidence of memo-cache/
self-opaquing-wrapper consistency in general on this shape, but not evidence that the loop x memo
interaction specifically (a wrapper repeating >=2 times, with memoization active partway through) is
sound — no fixture in this pass drives the loop past one iteration. That narrower claim remains
untested, a reasonable follow-on if a grammar requiring >=2 self-opaquing iterations is ever found
or built.

## `simultaneous-epenthesis-cascade`: a documented scope cut

A hand-designed (not C#-test-derived) rule whose own epenthesized output re-satisfies its own
trigger environment, run Iterative (no `multipleApplicationOrder` attribute). Under the real C#
oracle this crashes with an uncaught `InfiniteLoopException` — the fixture's own `expected.tsv` is
a truncated file containing only the `STARTED` sentinel for word 0, since the batch process died
before writing a result row. That truncation is the ground truth here, not a defect to fix into a
normal row.

Today's `syn_epenthesis` collects every epenthesis site against one pristine snapshot before
applying any of them — which is also exactly why it is correct, as-is, for `Simultaneous` mode — so
it has no per-call rescan loop to cascade through, and cannot reproduce C#'s
crash-via-runaway-self-feeding-Iterative-cursor behavior. No reference grammar (Indonesian/Amharic/
Sena) has a self-referential Iterative epenthesis rule, so this is not a correctness gap on any real
corpus — a pre-existing, narrower-than-full-fidelity property of `syn_epenthesis` that this fixture
is the first to distinguish. Accepted as a permanent scope cut for this pass (a faithful
iteration-cap-to-raised-error rewrite of `syn_epenthesis`'s site-collection loop would be a real,
separate, follow-on task); Rust's actual behavior (no crash, no hang, no parse) is asserted directly
rather than silently left unverified.
