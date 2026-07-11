# Phase 2 sub-plan: C# test-suite port to 68/68 (W11) — SUBSTANTIALLY COMPLETED

> **OUTCOME (2026-07-08/09):** batches 1-4 and 6-7 executed. The `csharp_port_*.rs` suite now
> covers all 8 C# test files: affix_process (18), affix_template (4), compounding (6, 2 ignored),
> generation (6), lex_entry (5), metathesis (3), morpher (7), rewrite (14, 7 ignored). Workspace
> total 373 tests. Every remaining `#[ignore]` carries a precise, verified finding string — the
> ignores ARE the remaining-parity worklist, mirrored as P-items in the finish plan
> (`rust-optimizations-phase2.md`). Remaining W11 slivers → finish-plan P8: the 5 GenerateWords
> assertions on the multi-stratum Suffix/PrefixRules grammar shape (ratified scope cut, fold in
> when that grammar shape is ported), the "68 denominator" drift reconciliation, and un-ignoring
> tests as P1-P6 fixes land. Bucket-E scope notes stand; guesser API still needs John's yes/no.
>
> **P8 CLOSEOUT (2026-07-10):** both slivers resolved, no code changes. The 5 GenerateWords
> assertions were already ported (a later W11 batch-7 follow-up this note was never updated to
> reflect) — see the W7 section of `workstreams-landed.md`. The 68-denominator drift is reconciled
> below: 68 was already correct (grounded in parse-opt's real count, A+B+C+D+E totals unchanged),
> the top-level copy's 7 missing/4 extra tests are two independent, unrelated drifts — the missing
> 7 split 3 Bucket-E (already documented) + 4 Bucket-D (named here for the first time, previously
> uncounted-by-name rows of the existing D=23), the extra 4 are out of scope by the same rationale
> as the already-Bucket-E'd `RoundTripXml`. Not a gap in the port.

**Baseline (audit D, HEAD `cdace6fa`):** of the canonical 68 C# `[Test]` methods
(8 files under `.worktrees/parse-opt/.../SIL.Machine.Morphology.HermitCrab.Tests/`):
**A=8** true Rust equivalents, **B=28** feature-covered-different-scenario, **C=4**
feature-works-no-test, **D=23** blocked on unported/unconfirmed features, **E=5** no product
surface. Full 68-row mapping: `rust/parity-out/audit/phase2/D-test-coverage-map.md` §3 (the
implementer's checklist — regenerate if lost).

**Fixture strategy (decided):** hand-port as Rust end-to-end tests over small loaded grammars
(the existing `*_gate.rs` style, asserting on `Word.syn_fs`/signatures) — NOT an XML fixture
exporter. Rationale: the newest gate files already assert on internal state directly, defusing the
old exporter rationale. Where expected outputs are non-obvious, oracle-diff the fixture grammar
through the C# engine first and pin its output.

**Known drift to fix first (S):** the `rust` branch's top-level C# test copy is missing 7
`MorpherTests` present in parse-opt's canonical suite — reconcile the copy (or document why) so
"68" stays the agreed denominator.

**RECONCILED (P8, 2026-07-10).** Direct count, both checkouts, all 8 canonical files
(`[Test]` methods under `tests/SIL.Machine.Morphology.HermitCrab.Tests/**` in each worktree):

| file | top-level copy | parse-opt (canonical) |
|---|---|---|
| AffixProcessRuleTests | 19 | 19 |
| AffixTemplateTests | 4 | 4 |
| CompoundingRuleTests | 3 | 3 |
| LexEntryTests | 6 | 6 |
| MetathesisRuleTests | 3 | 3 |
| MorpherTests | 9 | **16** |
| RewriteRuleTests | 16 | 16 |
| XmlLanguageSerializationTests | **5** | 1 |
| **total** | **65** | **68** |

parse-opt's real total is exactly **68** — the audit's denominator was already grounded in the
right suite; nothing about "68" itself needs to change. The drift is two separate, independent
divergences between the two checkouts (parse-opt is a `ccf750e6`/`cdace6fa`-era fork used
read-only as the parity oracle; the top-level copy tracks whatever `rust` branched from, which
picked up later unrelated mainline C# fixes parse-opt never received):

1. **MorpherTests, top-level missing 7** vs. parse-opt: `AnalyzeWord_SingleThreaded_MatchesParallel`,
   `AnalyzeWord_ConcurrentRepeatedParsing_IsDeterministic`,
   `ParseWord_SingleThreaded_MatchesParallel_WithCompounding`,
   `ParseWord_SingleThreaded_MatchesParallel_WithAffixTemplate`,
   `EnableLexicalGating_MatchesDisabled_SimpleAffixGrammar`,
   `IsEdgeStripperQualified_ReturnsFalse_ForReduplication`,
   `IsEdgeStripperQualified_ReturnsFalse_ForInfixation`. The last 3 are already correctly
   Bucket-E'd below ("no product surface"). The other 4 are not named anywhere in this doc's
   batches/buckets by name, but they don't create a gap in the 68 total: **A+B+C+D+E already sums
   to 68 over parse-opt's real 16-method `MorpherTests`, so these 4 were always counted — as
   unnamed rows of Bucket D ("blocked on unported/unconfirmed features"), not Bucket E.** They do
   NOT belong in Bucket E (no product surface would be wrong for one of them — see split below).
   Two of the four (`AnalyzeWord_SingleThreaded_MatchesParallel`,
   `ParseWord_SingleThreaded_MatchesParallel_With{Compounding,AffixTemplate}`) test C#'s
   `maxDegreeOfParallelism`/`Parallel*` intra-word rule-cascade variants; `hc-rules/src/cascade.rs`'s
   own module doc independently ratifies "the `Parallel*` variants are NOT ported (plan §7:
   within-word parallelism has no Rust descendant)" — Rust's only parallelism is ACROSS words
   (`hc-parse/src/batch.rs`'s rayon pool), so there is no intra-word parallel cascade for these 3
   tests to compare a single-threaded run against. Genuinely Bucket D (blocked on an unported
   feature), staying that way until/unless within-word parallelism is ever ported.
   The 4th, `AnalyzeWord_ConcurrentRepeatedParsing_IsDeterministic`, is **not** a "no surface" case
   — Rust does have concurrent parsing against one shared grammar (`hc-parse/src/batch.rs`'s rayon
   pool over multiple words). Its correct out-of-scope reason is narrower: the C# test is a
   regression net for a specific copy-on-write RACE (per-parse `FeatureStruct` clones sharing
   structure with a mutable frozen grammar); Rust's ownership model has no per-parse mutation of
   shared grammar state to race on (the loaded grammar is immutably `Sync`-shared, cloned-not-
   mutated per parse), so the bug class the test guards against cannot occur by construction, not
   because concurrent parsing itself is missing. Also Bucket D-shaped (blocked on a C#-specific
   internal detail with no Rust analog), for a different reason than its 3 siblings — do not lump
   it in with them if this note is ever revisited.
2. **XmlLanguageSerializationTests, top-level has 4 EXTRA** vs. parse-opt (5 vs. 1):
   `Save_MorphemeCoOccurrenceRuleReferencesUnwrittenMorpheme_DoesNotThrowAndOmitsRule`,
   `Save_MorphemeCoOccurrenceRuleReferencesUnwrittenOtherMorpheme_DoesNotThrowAndOmitsRule`,
   `Save_AllomorphCoOccurrenceRuleReferencesUnwrittenAllomorph_DoesNotThrowAndOmitsRule`,
   `Save_MorphemeCoOccurrenceRuleReferencesOnlyWrittenMorphemes_IsWritten`. These are C#
   `XmlLanguageWriter` robustness regression tests (does saving silently omit a rule referencing a
   morpheme not written to the file, without throwing) — zero HermitCrab parsing/generation
   behavior, same "no product surface" rationale as the already-Bucket-E'd `RoundTripXml`. Not part
   of the 68 (parse-opt doesn't have them); no action needed.

Net: **68 stands as the correct, parse-opt-grounded denominator, and the A/B/C/D/E totals are
unchanged** (the 4 concurrency tests were always inside D, just never named by this doc; nothing
moves to E, so E stays 5, not 9). The top-level copy under
`tests/SIL.Machine.Morphology.HermitCrab.Tests/` is not itself a source of truth for the port and
does not need to be edited/reconciled to match it — it's an independently-drifting mainline
checkout, not the audit's basis. No test needs porting as a result of this reconciliation; the
scope status of the 4 newly-named concurrency tests (3 Bucket-D "no Rust descendant", 1 Bucket-D
"different bug class, can't occur by construction") is recorded above for the permanent record.

## Batches (feature-dependency order; from audit D §6)

- **Batch 1 — start immediately, pure test-writing (no engine changes):**
  the 4 bucket-C tests (`AffixTemplateTests.AffixTemplateAppliedAfterMorphologicalRule`,
  `SameRuleUsedInMultipleTemplates`, `AffixProcessRuleTests.WordSynthesisWithBoundaryAtBeginning`,
  `MorpherTests.AnalyzeWord_CannotAnalyze_ReturnsEmptyEnumerable`) **plus the 28 bucket-B
  tightenings** — the single largest lever: each has working machinery, only the exact C# scenario
  is missing. Port them mechanically, one commit per test file, red-on-revert not required (they
  pin current behavior) but oracle-verified expected values required.
- **Batch 2 — after W6 (co-occurrence rules):** the 2 co-occurrence MorpherTests.
- **Batch 3 — after W5 (StemName/MprFeature parts):** `LexEntryTests.StemNames`,
  `CompoundingRuleTests.ProdRestrictRule`.
- **Batch 4 — after W9.1's oracle-diff probes:** `InfixRules`, `CircumfixRules`, `TruncateRules`,
  `NonContiguousRules`. Probe FIRST — if the general affix machinery already matches C#, this
  collapses to test-writing (S each); if not, feature work (M/L) goes back to the W9.1 owner.
  (`NonContiguousRules` also interacts with the iterative-unapply watch-list item.)
- **Batch 5 — after W9.2 probes / W8 narrowing:** the untouched Rewrite cluster —
  `LongDistanceRules`, `QuantifierRules`, `MultipleSegmentRules`, `MergeRules`,
  `MultipleMergeRules`, `ExpandRules`, `DisjunctiveRules`. Merge/Expand analysis-side requires W8.
- **Batch 6 — scope-cut specs:** `AffixTemplateTests.RealizationalRule` (until W5) +
  `MetathesisRuleTests` ×3 (until W4) land as `#[ignore]`d executable specs the moment their
  feature lands, then un-ignore.
- **Batch 7 — after W7's WordAnalysis design:** the 4 structured-analysis/generation MorpherTests.
- **Bucket E (never, document):** `TestMatchNodesWithPattern`, `EnableLexicalGating_*`,
  `IsEdgeStripperQualified_*` ×2, `XmlLanguageSerializationTests.RoundTripXml` — one-line scope
  notes in the test-map doc. `AnalyzeWord_CanGuess_*` needs an explicit user scope decision
  (guesser API: yes/no).
- **Bucket D, named by P8 (68-denominator reconciliation, previously unnamed rows):**
  `AnalyzeWord_SingleThreaded_MatchesParallel`,
  `ParseWord_SingleThreaded_MatchesParallel_WithCompounding`,
  `ParseWord_SingleThreaded_MatchesParallel_WithAffixTemplate` — blocked on C#'s unported
  `Parallel*`/`maxDegreeOfParallelism` intra-word rule-cascade variants (`hc-rules/src/cascade.rs`:
  "within-word parallelism has no Rust descendant"). `AnalyzeWord_ConcurrentRepeatedParsing_
  IsDeterministic` — also Bucket D, but for a distinct reason: Rust's shared-immutable-grammar
  ownership model has no per-parse mutation for the copy-on-write race this test guards against to
  occur on, even though cross-word concurrent parsing itself exists (`hc-parse/src/batch.rs`).

## Verification per batch
Standard protocol; additionally each ported test's expected values must come from the C# oracle
run (cite the oracle invocation in the test's doc comment). Corpus gates after every batch land.
