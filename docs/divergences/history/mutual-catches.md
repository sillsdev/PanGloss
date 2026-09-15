# Mutual catches

Episodes where running, reading, or building one implementation exposed a real defect in the
other — in either direction. Ordinary port gaps (a construct C# has that Rust hasn't built yet, or
vice versa) don't belong here; every entry below is a case where the two disagreed on a concrete
input and one of them was wrong.

## C# caught by Rust-adjacent (conformance) work

### PR #471 — metathesis switch-name-order crash

**The defect.** A `MetathesisRule` whose `leftSwitch` names the *earlier* of its two pattern groups
throws `InvalidOperationException` / `ArgumentException: Only nodes from the same list can be
compared` in `Morpher.Synthesize`, and (independent of the crash) the analysis-side pattern is built
in the wrong order relative to the surface, so even with the crash fixed there was no analysis.
Every pre-existing C# test happened to name the *later* group first, so the intuitive naming was
never exercised.

**How it was found.** While building generated-coverage fixtures for the HermitCrab XML surface on
`integrate-conformance-framework`. The PR body is explicit: "found while building generated-coverage
fixtures... a fixture written to exercise `MetathesisRule@multipleApplicationOrder` wrote its switch
names in the natural order and hit this." Authored with Claude Code (per the PR's own footer),
i.e. this is conformance-fixture-authoring work of the same kind this repo's own
`conformance-grammars` skill does, run against the founding oracle directly.

**Fixed:** `sillsdev/machine` PR [#471](https://github.com/sillsdev/machine/pull/471), merged
2026-08-18, `60a0925f`.

**Pinned:** on the Rust side, by `metathesis-comparison-crash` (a conformance-staging fixture using
`expect_crash`) plus commit `b106276a` (2026-08-19), which records that PanGloss reaches the correct
analysis where C# used to crash, and names PR #471 as the upstream fix.

### PR #474 — RealizationalRule unbounded reapplication

**The defect.** `SynthesisRealizationalAffixProcessRule.Apply` (the synthesis side of a
`RealizationalRule`) never checked `GetApplicationCount` against a cap, unlike
`SynthesisAffixProcessRule`/`SynthesisCompoundingRule`. A `Blockable` `RealizationalRule` not rescued
by `CheckBlocking` could therefore reapply to its own growing output forever, hanging `ParseWord`.
`Morpher.MaxAlternatives` (the general escape valve) is wired into `AnalysisStratumRule` but never
into `SynthesisStratumRule`, so it couldn't catch this either.

**How it was found.** A conformance fixture with two pre-existing `mprRRealTest`-tagged roots hangs
when a third is added; the first two each dodge the bug for unrelated reasons (one has conflicting
`AssignedHeadFeatures`, the other has a `family` that substitutes a word which fails the retry and
stops the recursion after one step). Authored with Claude Code.

**Fixed:** PR [#474](https://github.com/sillsdev/machine/pull/474), merged 2026-08-18, `ba0e245a`.
Fix: add the same application-count guard synthesis affix/compounding rules already have, hardcoded
to a cap of 1 (a realizational rule has no `multipleApplication` DTD attribute, so 1 is definitional,
not configurable).

**Pinned:** not established on the Rust side. See `upstream-prs.md`'s "Merged, and what Rust did
with them" section — no commit or test naming an equivalent realizational-rule hang was found.
**Open action:** author a regression test mirroring PR 474's `MorpherTests.cs` case
(`ParseWord_UnblockedRealizationalRuleMatchesOwnOutput_DoesNotHang`) against `pg_parse::Morpher` to
confirm Rust never had this defect, rather than assuming it from the absence of a bug report.

### PR #480's "one reimplementation bug" — `rightToLeftIterative` multi-site ordering

**The defect (in HC-Rust, found from the C# side).** PR #480's own body: "Twelve of the 33 fixtures
had never been run against that engine [a reimplementation reachable through `PROTOCOL.md`]; eleven
passed and the twelfth found this. `multipleApplicationOrder="rightToLeftIterative"` produces the
left-to-right result when a rule has multiple firing sites in one pass." This is a defect in the
port, caught by testing it against the C# conformance suite.

**Independently found and fixed on the Rust side, same day.** PanGloss's own history shows this
being discovered and closed within one day (2026-08-19), immediately after the
`integrate-conformance-framework` submodule repoint pulled in fixtures that exercise it:
- `dc568ca0` — "pin the open `rightToLeftIterative` multi-site ordering bug (not fixed)." Two
  independent reductions: `pg-rules/tests/metathesis_gate.rs`'s
  `overlap_rightmost_bug_rtl_fires_leftmost_instead` (confirmed root cause:
  `pg_rules::metathesis::match_candidates` sorts every candidate ascending and never consults the
  rule's direction) and `pg-parse/tests/rtl_iterative_multi_site_ordering_gate.rs`'s
  `root3_eeae_should_parse_and_eeee_should_not` (a distinct code path, `pg_rules::rewrite`'s
  `Kind::Feature` analysis, not root-caused to one line in this commit).
- `da1b45a0` — fixes the metathesis case: `match_candidates` now reverses to descending order when
  the compiled FST direction is `RightToLeft`, mirroring the fix `pg_rules::rewrite` already had.
- `08d1cd2e` — fixes the rewrite case: `syn_feature`/`ana_feature` fully rescanned from scratch each
  outer-loop pass, so an environment-rejected site could get an unearned second chance once a later
  site's rewrite changed that environment. Tracks rejected start positions persistently across
  passes on both synthesis and analysis sides.

**Whether these are literally "the reimplementation" PR #480 tested against was not independently
confirmed** — PR #480's body does not name the reimplementation, and this investigation did not find
a cross-reference from the C# side to PanGloss specifically for this finding. Given `PROTOCOL.md`'s
adapter contract and that PanGloss is the only actively-developed HermitCrab reimplementation this
investigation is aware of, it is very likely the same defect, but that identity claim is
**circumstantial, not confirmed by a citation in either repo.**

**Pinned:** `dc568ca0` (the two `#[ignore]`d reductions, which the two fix commits then un-ignore in
substance — not verified here whether the literal `#[ignore]` attributes were removed in the same
commits or a follow-up; check `pg-rules/tests/metathesis_gate.rs` and
`pg-parse/tests/rtl_iterative_multi_site_ordering_gate.rs` directly if this matters).

## Rust caught by its own oracle-fidelity work

These are not C#-vs-Rust divergences in the usual sense — they are bugs in Rust's own confirm engine
(the "oracle" the FST proposer is checked against internally), found by reading C# source directly
while building unrelated FST compilation work, and fixed to match C#'s cited behavior. Included here
because the fix direction (Rust conforming to C#) and the discovery method (adversarial reading of
the founding oracle's source) are exactly the pattern this file exists to track.

### `7bab72cd` — iterative rewrite pick-order not direction-aware

**The defect.** `pg_rules`' Iterative pick-one loops (`syn_feature`, `syn_narrow`, `probe_narrow`)
selected candidate matches by unconditional ascending (leftmost) order, ignoring the rule's declared
direction — so a `rightToLeftIterative` rule behaved identically to `leftToRightIterative` whenever
candidate windows didn't overlap in a way that mattered. Cited C# semantics:
`IterativePhonologicalPatternRule.cs:17-48` (`Match` finds the nearest match to the
`Matcher.Direction`-side edge, then resumes scanning further in that direction) and
`AnalysisRewriteRule.cs:33` (un-application scans the *opposite* direction from synthesis —
"subtle; easy to get backwards").

**Found while:** implementing RTL FST compilation (`pg-foma`), not while doing conformance work —
an ordinary implementation task surfaced a pre-existing correctness bug in the code being relied on
to verify the new FST work.

**Fixed:** same commit, `7bab72cd` (2026-07-25). New `ordered_spans` helper (descending when
direction is `RightToLeft`); `ana_feature`'s candidate list reversed via its existing `rtl` flag.
Deliberately left untouched: `sim_feature`/`sim_narrow`/`probe_sim_narrow` and
`ana_narrow_deletion`/`ana_narrow_general`/`ana_epenthesis`/`syn_epenthesis`, because
`SimultaneousPhonologicalPatternRule.cs:22-36` collects-then-applies every match and has no
pick-one question — cited as "no shape was left alone due to C# ambiguity."

**A downstream consequence worth knowing:** `pg-foma`'s `phase_c_right_to_left` test had *pinned the
bug as data* — its own comment anticipated this revisit — so fixing the oracle required also fixing
that test's expected output (the RTL-declared rule's oracle now confirms `"ab"` instead of `"ba"`).

### `2cdccd08` — metathesis analysis not a true inverse of synthesis

**The defect, two parts.** (1) `build_analysis_pattern` ordered the rebuilt search pattern by *tag
name* while synthesis swaps by *physical position*; for every attested grammar the two happen to
coincide, so this only shows up for the reversed tag convention (which PanGloss's own
`pg-grammar-gen` emits) — analysis found zero parses for either surface form in that case. (2)
`build_analysis_pattern` dropped any node between the two switch groups, while synthesis kept it, so
such rules could never be confirmed.

**Found while:** implementing FST metathesis compilation (`pg-foma`, Stage 2) — same pattern as
`7bab72cd`.

**Fixed:** same commit, `2cdccd08` (2026-07-25). Switch ordering now by physical position (matches
C# for every real grammar; the reversed case exposes a **latent, never-exercised C# bug of its own**
— cited in code — where C#'s literal-constructor logic produces a vacuous non-swapped pattern).
Middle context node preserved unless it resolves to `CharDefKind::Boundary`, matching the one
attested C# shape byte-for-byte, and "correct for a middle SEGMENT node" as a documented, justified
divergence from C#'s literal-constructor mechanics rather than a guess.

### `c4c51df0` — one confirmed C# bug, one non-reproduction

Investigated two long-standing "oracle gap" reports rather than assuming either was still real.

**Finding 1 — `width_matches`/quantifier-as-focus is a genuine, still-open C# bug, and Rust
deliberately does not replicate it.** C#'s rewrite-rule specs (`SynthesisRewriteRuleSpec.cs:33`,
`FeatureAnalysisRewriteRuleSpec.cs:104`, `NarrowAnalysisRewriteRuleSpec.cs:45`,
`EpenthesisAnalysisRewriteRuleSpec.cs:18`, and the metathesis siblings) all do
`Cast<Constraint<Word,ShapeNode>>` over the rule's own LHS/RHS children, but `Constraint<T>` and
`Quantifier<T>` are **sibling** subclasses of `PatternNode<T>` — so a top-level `Quantifier` throws
`InvalidCastException` at `Morpher` construction, even though the DTD explicitly permits the shape
and the XML loader builds it successfully. There is no C# behavior to converge on. Rust's actual
behavior (probed, not assumed): a bounded quantifier as the whole LHS never crashes and never
mis-groups — each child occurrence is matched as its own width-1 site, silently ignoring the
grouping. "Strictly safer than C#'s crash." **This has not been proposed upstream as a PR** — it is
recorded here and in `docs/hermitcrab-rust-port-audit.md`, but no `sillsdev/machine` issue or PR
matching this description was found by this investigation.

**Finding 2 — `ana_epenthesis` "finds no analysis" does not reproduce.** The exact cited fixture was
reloaded byte-for-byte, with and without `rightToLeftIterative`, and run through both
`pg_rules::rewrite::analyze` directly and the full `Morpher` pipeline: all three test words return
the oracle-correct result in both directions. `git log` confirmed no intervening fix that would
explain a since-resolved bug. Conclusion: "the earlier report was a mis-attribution," documented as
a precise non-finding rather than left as an ambiguous open item.

## Rust caught by itself, before this repo's own history — W3.3 (discontinuous morphs)

**The defect.** `attribute_morphs`'s discontinuous-morph handling used a single-merged-record
approximation: it derived each morph's environment-check span from one combined record per morph,
rather than one record per **contiguous run** of that morph's material. For a genuinely
discontinuous morph — a circumfix's two pieces, or a root split by a later infixing rule — this
mis-anchors the environment check. C#'s `MarkMorphs` (per-annotation loop, split in commit
`97fa7721`, 2018-11-28: "Do not allow non-contiguous morph annotations") checks each contiguous run
at its own span; the merged-record approximation checked the whole discontinuous span at once.

**The concrete failure, pre-fix:** Rust accepted `xpitz` and `muat`; the C# oracle rejects both,
because the environment check fails specifically at the morph's *second* piece — a fact invisible to
a single merged span.

**Fixed:** `pg-rules/src/validity.rs`'s doc comment (lines 50-59 as read in this investigation)
attributes the fix to `attribute_morphs` (`morph.rs`) now emitting one `MorphRecord` per contiguous
run, matching C#'s per-annotation loop exactly.

**When:** this fix, and its regression test, are both already present in this repo's very first
commit (`a60d6169`, "Initial import: HermitCrab Rust port, foundation for PanGloss," 2026-07-10) —
the fix predates this repository's own history. It was made on `machine`'s `rust` branch before the
2026-07-10 squash-copy (`docs/hermitcrab-rust-port-audit.md` §1). **That branch no longer exists**
in the local `machine` checkout or on `origin` (`git branch -a` / `git ls-remote --heads origin` both
came up empty for any branch matching `rust`), so the original commit-by-commit history behind W3.3
— who found it, exactly when, against what fixture — **could not be recovered by this
investigation.**

**Was pinned by a dead gate; the gate is now gone and the entry is honestly UNPINNED.**
`pg-parse/tests/discontinuous_env_gate.rs` read its fixture from the v1 layout
(`conformance/allomorphy/discontinuous-env`) and carried `#[ignore]` on both tests, so it skipped
twice over and never once ran in this tree. It was one of 13 such files, all pointing at a fixture
layout the v1 -> v2 migration retired; 7 of the fixtures they named were carried into
`machine/conformance/edge-cases/` under new names and the rest were not. The whole set has been
deleted rather than left skipping — a test that cannot run must say so, and deleting it says so
louder than an `#[ignore]` nobody reads.

What the deletion does **not** do is restore the coverage. No fixture in either root reproduces the
`xpitz`/`muat` shape — a discontinuous morph whose allomorph environment holds at its first piece
and is violated at a later one — so the `attribute_morphs` contiguous-run split is unprotected.
**Open action:** author that fixture against `hc.dll` (per `.claude/skills/conformance-grammars/
SKILL.md`), then add a trace-level pin for it the way `disjunctive_recheck_gate.rs` now does for
W3.2, since the generic replay diffs signatures only and cannot see a rejection *reason*.

**Related, and already caught once:** `056af8e3` (2026-08-31) records that this repo's own circumfix
refusal logic had over-cited W3.3 — a blanket refusal of any circumfix whose half carried a
phonological environment, on the theory that W3.3's per-run anchoring required it. That commit shows
the refusal was unnecessary: `environments_ok` is `any()` within one run and `allomorphs_valid_impl`
requires every run to pass, so the union of both halves' environments on one allomorph already
self-partitions correctly (prefix condition checked at the prefix run, suffix condition at the
suffix run) with no additional scoping mechanism needed. This is a case of the *original engineer's
own reasoning about a mutual catch* being re-examined and found to have over-generalized — worth
knowing before citing W3.3 as justification for a new refusal.

## Cross-pollination, not strictly a "catch" — credit and reverse-direction porting

### `AnalysisStateKey` — a C# fix built on a PanGloss author's own earlier C# work

PR #493's own description: "Comparing `MergeEquivalentAnalyses` to John Lambert's memoization code, I
noticed that it was not computing equivalence correctly. So I borrowed `AnalysisStateKey` from the
memoization code to fix the problem." "John Lambert's memoization code" is PR
[#456](https://github.com/sillsdev/machine/pull/456), "Memoize HermitCrab's sequential analysis
cascade," authored by `johnml1135` — the same person who authors this repository's commits under
`john_lambert@sil.org`. Not a Rust-vs-C# catch, but the direct ancestor of the `AnalysisStateKey`
mechanism this whole PR-493/494 episode revolves around.

### `hc/memo-key-saturation` — PanGloss's fix ported back into C#

See `machine-branches.md` for the branch detail. Commit `d3f227ab` in this repo ("saturate the memo
key rule counts at each rule max_apps," 2026-09-10) precedes the C# branch's equivalent commit
(`0eb2c45c`, "Saturate `AnalysisStateKey` rule-unapplication counts at each rule's cap," 2026-09-11)
by one day, and the C# branch's own PR-description commit (`d1f9302d`) says it cites "PanGloss's
measurements." This is the one clear case in this investigation of a defect or improvement flowing
**from Rust into C#** rather than the usual direction. No PR has been opened for it yet.
