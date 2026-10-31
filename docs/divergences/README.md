# HC-Rust / hc.dll permanent differences

This folder lists only the places where `pg-parse`/`pg-rules`/`pg-featstruct`/`pg-grammar` (the Rust
port, "HC-Rust") differs from the founding oracle, `SIL.Machine.Morphology.HermitCrab`'s C#
implementation (`hc.dll`), and is **planned to stay different**. Each entry is an intentional,
long-standing difference.

- Upcoming fixes, speedups and unproven equivalences are GitHub issues in
  [sillsdev/PanGloss](https://github.com/sillsdev/PanGloss/issues), not entries here.
- Retired entries (fixed, optimized, moot, or moved to an issue) live in git history; see
  [Retired entries](#retired-entries).
- Per the repo rule, HC-Rust must never produce a different parse than C# without that difference
  being documented here (and, if Rust believes its behaviour is an improvement, reported in a Machine
  issue). A new difference that is meant to stay gets an entry; one that is work-in-progress gets an issue.

## What counts as an entry

One entry per distinct mechanism-level difference between the two engines that is planned to stay,
traceable to a specific C# file/method and a specific Rust file/function.

## Kinds

- **`behavioural`**: the two implementations can produce different parses for some input.
- **`efficiency`**: same observable parses, different work or memory.
- **`representational`**: same behaviour, different data model or API shape.
- **`unported`**: a C# feature/branch Rust does not implement; the entry says what happens if reached.
- **`diagnostic`**: a Rust-only report or tool behaviour with no parse difference.

## File naming

One file per entry: `docs/divergences/NNN-short-slug.md`, `NNN` a zero-padded three-digit number
assigned in discovery order. Numbers are never reused or renumbered; retired numbers stay listed in the
table below, so the gaps in this folder are expected.

## Finding the entry for a file

- **From a C# file or Rust module:** grep this folder's `## C# site` / `## Rust site` lines, or use
  `by-module.md`.
- **From the status table below:** every entry is listed with both sites, kind, status and pinning
  fixture in one row.

## Status table

| id | title | kind | status | C# site | Rust site | pinning fixture/test |
|---|---|---|---|---|---|---|
| 020 | [Bounded `Quantifier` spanning a whole LHS/RHS: silently inert, not refused](020-bounded-quantifier-whole-pattern-unsupported.md) | unported | permanent (wont-fix) | rule-spec constructors (cast every LHS/RHS child to a non-quantifier type; throws on load) | `pg-rules/src/rewrite.rs::width_matches` and callers | none |
| 024 | [Analysis memo key saturates unapplication counts; C# does not](024-memo-key-count-saturation.md) | representational | permanent | `AnalysisStateKey.cs:14-34` | `pg-rules/src/stratum.rs::state_key` (reader is now the merge fold) | `pg-rules/tests/unapplied_rule_counts_reader_gate.rs` |
| 026 | [Syntactic-domain feature structures are provably variable-free; phonological variables live in a separate mechanism](026-featstruct-variable-domain-split.md) | representational | permanent | `FeatureValue`/`SimpleFeatureValue` (shared variable machinery for both domains) | `pg-featstruct/src/{tree,ops}.rs` vs `pg-rules/src/rewrite.rs` (`bind_or_check`) | none — argued sound by construction in `ops.rs`'s module doc |
| 027 | [Guesser matcher bypasses the FST engine, and is bug-compatible with C#'s single-owning-entry fabrication](027-guesser-bypasses-fst.md) | representational | permanent | `Morpher.MatchNodesWithPattern`/`Morpher.LexicalGuess` (Morpher.cs:522-625) | `pg-parse/src/guess.rs` | `pg-parse/src/guess.rs` unit tests (ported from `MorpherTests.TestMatchNodesWithPattern`) |
| 028 | [Confirmation-free accuracy screen: an unmeasured-until-now soundness hazard](028-confirmation-free-accuracy-hazard.md) | efficiency | permanent | (no C# equivalent — Rust-only evaluation harness) | `pg_foma::recipe_accuracy`, `pg_foma::parity::IdentityDivergence` | `parity_divergence_census.rs`; `candidate_only_identities` measured 0 on current fixtures |
| 045 | [Analysis memoization removed from HC-Rust](045-memoization-removed.md) | behavioural under a budget | permanent (removed-in-rust) | `AnalysisScope / AnalysisStateKey` | `pg-memo` deleted; `pg-rules/src/analysis_state_key.rs` keeps the merge key | `-Mode test` + `-Mode conformance-test -Scope all`; `docs/research/memo-sena-bisect.md` and four companion measurement docs |
| 052 | [Independent inner-work allowance preserves analysis attempts](052-shared-search-work-cap.md) | behavioural | permanent | `TraversalMethodBase`, `Word.ExpandAlternatives`, `Morpher` | `pg-fst::work`, `StepBudget`, `Morpher` | synthetic FST/stratum/trie/alternative and parse gates; execution pending ([#83](https://github.com/sillsdev/PanGloss/issues/83)) |
| 059 | [Construct variants FieldWorks cannot author stay refused by the fast path](059-fieldworks-unauthorable-variants.md) | unported (fast path only) | permanent refusal; LT-22826, LT-22827 | `HCLoader.cs` (`LoadMetathesisRule`, affix-process loading); `AnalysisMetathesisRuleSpec.cs` | `pg-foma/src/capability/variants.rs::permanent_refusal_reason` | `pg-foma-backend/tests/conformance_coverage_gate.rs`; staged `realizational-reduplication-no-proposal-route` |
| 070 | [Feature-defined classes in allomorph environments bind, where XAMPLE loses them](070-xample-feature-class-environment.md) | behavioural (XAMPLE) | permanent; FieldWorks export defect LT-22825 | FieldWorks XAMPLE export (`FxtM3ParserToXAmpleADCtl.xsl`, `NatClassStringToHvo`); no C# difference | `pg-grammar/src/compile/environment.rs::load_environment_pattern` | staged `underdefined/06-feature-class`, `11-distinct-feature-class` |
| 076 | [Underdefined natural-class membership decision](076-underdefined-natural-class-membership-decision.md) | behavioural | permanent; upstream discussion open in sillsdev/machine#537 | constrained-class subsumption / analysis-side unification | `pg-grammar-model/src/membership.rs`, `segment.rs`; rule bridges and Foma owner calls | constrained/list membership, unconstrained wildcard, `chained-output-feature-override-loss:zudi`; separate-lane witnesses 07 and 12 |
| 077 | [Missing natural classes drop the whole environment and produce Info](077-missing-natural-class-environment-info.md) | diagnostic | permanent; no parse divergence | FieldWorks environment export / `HCLoader.InvalidEnvironment` | `pg-grammar/src/compile/mod.rs::Ctx::environment_resolution`; `pg-snapshot` diagnostic metadata | staged `underdefined/05-missing-class`; named whole-drop, repaired-control and Info regression |
| 078 | [XAMPLE minimum: authored phonology outranks XAMPLE](078-xample-minimum-authored-phonology.md) | behavioural | permanent under ADR 0008 | XAMPLE ignores authored phonology; FieldWorks/HC discard unreadable literal environments | fwdata compiler and authored phonology; `pg-cli/tests/underdefined_stored_keys_gate.rs` | seven exact minimum exceptions; twelve profile-v2 structured-identity/status tables |
| 086 | [Grammar health reports stored analyses lost to authored phonology](086-stored-analyses-lost-to-authored-phonology.md) | diagnostic | permanent; no new parse divergence | XAMPLE ignores authored phonology; ADR 0008 requires reporting stored analyses that no longer parse | `pg-fwdata` stored-analysis extraction; `pg-cli/src/stored_analysis_health.rs`; `pg-parse` traced synthesis | staged 08 and 12 cases; control, unattributed, and synthesis-cap regressions; revert checks and final gates in lane report |
| 089 | [Grammar health finishes when a stored-analysis comparison cannot](089-grammar-health-bounded-stored-analysis-comparison.md) | diagnostic | permanent; no parse divergence | none — grammar health is a Rust CLI facility | `pg-cli/src/stored_analysis_health.rs::check_with_options`, `compare_words_in_order`, `compare_word`; `pg-cli/src/grammar_health.rs` | `grammar_health_reports_a_capped_wordform_and_compares_the_rest`; `stored_analysis_comparison_stops_at_its_work_budget_identically_at_every_thread_count`; `stored_analysis_comparison_reports_every_capped_wordform_at_every_thread_count`; `grammar_health_stored_analyses_flag_selects_the_comparison` |
| 097 | [FieldWorks `<HC><MaxAlternatives>` is a documented PanGloss parity exception](097-hc-max-alternatives.md) | behavioural | permanent (accepted-divergence) | FieldWorks `HCParser.cs` (`LoadParser`, `ProcessParseException`); `Morpher.cs` (`Synthesize*`), `AnalysisStratumRule.cs`, rule cascades | `pg-fwdata/src/parser_params.rs` (setting omitted); `pg-parse/src/morpher.rs` (separate resource caps and incomplete outcomes) | `pg-conformance-fixtures` unsupported-setting and incomplete-result tests; no positive-value parity fixture by design |

## Retired entries

These entries were removed from this folder. Their latest pre-retirement files are intact at commit
`995cb27a` (for example `git ls-tree 995cb27a docs/divergences/` then
`git show 995cb27a:docs/divergences/<file>`). Entries that
went to an issue are open work, not permanent differences. Evidence under `evidence/` and `history/` is kept.

| id | title | went to |
|---|---|---|
| 001 | Analysis syntactic-FS fold: Add vs PriorityUnion | Superseded by Exact fold (149f88df); residual is 002; git history |
| 002 | Analysis syntactic-FS fold: exact inverse of synthesis | #26 |
| 003 | Compounding homophone-disjunction collapse | Fixed in Rust (or not a bug); git history |
| 004 | `Word::current_non_head()` index vs last-element | Fixed in Rust (or not a bug); git history |
| 005 | Prefix-commutes-with-compounding misdiagnosis | Not a bug; git history |
| 006 | Discontinuous-morph environment anchoring (W3.3) | Fixed in Rust (or not a bug); git history |
| 007 | `ModifyFromInput` char-def staleness | Fixed in Rust (or not a bug); git history |
| 008 | Morph-attribution drop on input-morph subsumption | Fixed in Rust (or not a bug); git history |
| 009 | Analysis-side `char_def` staleness in rewrite rules | Fixed in Rust (or not a bug); git history |
| 010 | Root lookup needs unification, not char-def identity, on feature-bearing tables | Fixed in Rust (or not a bug); git history |
| 011 | Missing word-initial epenthesis synthesis site | Fixed in Rust (or not a bug); git history |
| 012 | RTL analysis direction inversion for multi-node targets | Fixed in Rust (or not a bug); git history |
| 013 | Boundary node counted as a spurious epenthesis site | Fixed in Rust (or not a bug); git history |
| 014 | Iterative epenthesis cascading | Fixed in Rust (or not a bug); git history |
| 015 | Deletion-composition loses candidates behind an interposed Optional | Fixed in Rust (or not a bug); git history |
| 016 | `RewriteMode::Simultaneous` real semantics | Fixed in Rust (or not a bug); git history |
| 017 | RTL + bounded quantifier: shallow-reverse mirror risk | Code fixed; status text under #36 |
| 018 | Realizational-FS diff: failed unify nulls the FS in C#, kept in Rust | #29 |
| 019 | Root-allomorph trie previously mis-indexed pattern allomorphs | Fixed in Rust (or not a bug); git history |
| 021 | Guesser FFI symbols could return an unmarked guessed analysis | Fixed in Rust (or not a bug); git history |
| 022 | `Morpher.MaxStemCount`: hardcoded then made configurable | Fixed in Rust (or not a bug); git history |
| 023 | `AnalysisStateKey` representation: interned ids vs live references | Moot after memoization removal (045); git history |
| 025 | Two in-flight re-entrancy guards where C# has one | Moot after memoization removal (045); git history |
| 029 | Declared morpheme tags can vanish from the compiled lexc alphabet at depth | #30 |
| 030 | Disjunctive-allomorph / free-fluctuation re-check (W3.2), formerly deferred | Fixed in Rust (or not a bug); git history |
| 031 | Analysis cascade memoization | Moot after memoization removal (045); git history |
| 032 | Template-battery memoization | Moot after memoization removal (045); git history |
| 033 | Final-template analysis pruning | #27 |
| 034 | Stratum analysis-state merging | #26 |
| 035 | Template/slot feature collisions and widening | #26 |
| 036 | Zero-width morpheme identity preservation | PR #9 |
| 037 | Oracle annotation ordering instability | #64 (machine#506) |
| 038 | Edge-segment prefilter | Optimized in Rust; git history; Machine PR #574 |
| 039 | fwdata circumfix conditioning encoding | Fixed in Rust (or not a bug); git history |
| 040 | FST rewrite/metathesis compilation was blind to cross-table shared representations | Fixed in Rust (or not a bug); git history |
| 041 | Oracle-side (`pg-rules`) phonological/metathesis/allomorph resolution defaulted to table 0 | Fixed in Rust (or not a bug); git history |
| 042 | Metathesis relocation left a stale origin-table char-def identity at the surface-match gate | Fixed in Rust (or not a bug); git history |
| 043 | Synthesis-direction stratum reassignment diverged from hc.dll's surface rendering | Reverted in Rust to match hc.dll; git history |
| 044 | RTL rewrite-rule FST construction refused `Segments`-shaped patterns entirely | Fixed in Rust (or not a bug); git history |
| 048 | `batch --stats` parsed each HC word twice | Optimized in Rust; git history |
| 049 | Copy-agreement pruning of reduplication matches | Optimized in Rust; git history (049 refreshed in PR #13) |
| 050 | A part-of-speech-scoped deletion blocks metathesis analysis of other words | Fixed in Rust; still open upstream (machine#520); #28 |
| 051 | Invalid snapshot environments refused the whole grammar | #35 |
| 053 | Grouped ad hoc prohibitions were omitted from `.fwdata` imports | Fixed in Rust (or not a bug); git history |
| 054 | `.fwdata` word-boundary identity was inferred from a missing marker record | Fixed in Rust (or not a bug); git history |
| 058 | Repeated environment alpha variables lose agreement in HC-Rust | Fixed in Rust on main (7638a6e5) |
| 065 | A nullable environment is rejected at the word edge | Fixed in Rust on main (7638a6e5) |
| 066 | Repeated alpha environments require optional FST proposals | Lower-level proposal work fixed; whole-backend admission superseded by 116 |
| 067 | Ambiguous disagreement requires confirmed membership proposals | Lower-level proposal work fixed; whole-backend admission superseded by 116 |
| 068 | Permanent refusal reasons for unauthorable rewrite variants | #31 |
| 069 | Analysis binds an alpha feature overwritten by the replacement | Fixed in Rust on main (7638a6e5) |
| 071 | Literal rewrite unapplication needs StrRep identity | Accepted upstream (machine#538); git history |
| 072 | Provisional letter units | #32 |
| 073 | Provisional letters excluded from classes with conditions | #32 |
| 074 | Featureless phonemes excluded from classes requiring values | #32 |
| 075 | Provisional definitions always apply | #32 |
| 079 | Partial explicit classes in ambiguous-disagreement proposals | Fixed in Rust on main (7638a6e5) |
| 081 | Inflectional process allomorphs require their classes | Fixed in Rust (or not a bug); git history |
| 082 | Inflectional circumfixes require the prefix half's classes | Fixed in Rust (or not a bug); git history |
| 083 | Narrowing unapplication loses an RHS match across an interposed Optional | Accepted upstream (machine#540); stale text under #36 |
| 087 | Narrowing unapplication unions the LHS into the matched RHS nodes | #33 |
| 088 | Grammar health compared stored analyses serially and import repeated provenance work | Optimized in Rust; git history |
| 090 | Morphological and allomorph-environment matching lost segment identity on wide featureless tables | Fixed in Rust (or not a bug); git history |
| 091 | Featured grammars lost segment identity in allomorph and rewrite environments | Fixed in Rust (or not a bug); git history |
| 092 | Compounding analysis trace nodes: `CompoundingRuleAnalysis` with the subrule index where C# master emits `MorphologicalRuleAnalysis` | #34 |
| 093 | `Morpher.DeletionReapplications` was never read: deletion unapplication always ran one pass | Fixed in Rust (or not a bug); git history |
| 094 | FieldWorks `<HC><MaxRoots>` was never read: every project compounded at most two stems | Fixed in Rust (or not a bug); git history |
| 095 | The untraced affix-template battery runs a slot batch once for every template that shares it | Optimized in Rust; git history |
| 096 | FieldWorks `<HC><MergeAnalyses>` was never read: analyses were always merged | Fixed in Rust (or not a bug); git history |
| 098 | FieldWorks `<HC><GuessRoots>` was never read; PanGloss guessed only on `--guess` | Fixed in Rust (or not a bug); git history |
| 099 | FieldWorks `<HC><MergeMSAs>` was never read: equal MSAs loaded as separate morphemes | Fixed in Rust (or not a bug); git history |
| 100 | A malformed HCLoader boolean is a warning in PanGloss but loads no parser in FieldWorks | #35 |
| 101 | FieldWorks `<HC><MakePartialsFinal>` was never read: final templates were enforced only by a CLI flag | Fixed in Rust (or not a bug); git history |
| 102 | Clitic rules and blocking strata (Machine #529) were unported | Fixed in Rust (or not a bug); git history |
| 103 | A re-inserted segment-class node had no identity on a featureless grammar | Fixed in Rust (or not a bug); git history |
| 104 | Expansion unapplication loses an RHS match across an interposed Optional | Fixed on main; git history |
| 105 | Lexical-lookup alternatives are replayed against stem-name and MPR gates before synthesis | Optimized in Rust; git history |
| 109 | Required-slot look-ahead | Optimized in Rust; git history |
| 110 | Shared template walk dead mask for required-slot look-ahead | Optimized in Rust; git history |
| 111 | Ordered required-slot look-ahead | Optimized in Rust; git history |
| 112 | Unbounded-run purity for required-slot look-ahead | Optimized in Rust; git history |
| 113 | Analysis-selected affix allomorphs guide synthesis | Optimized in Rust; git history |
| 114 | Realizational rule synthesis application cap | Fixed in Rust; git history |
| 115 | Alpha-variable environment matching loses alternatives | Fixed in Rust in PR #13; git history |
| 116 | FST alpha-variable environment support | Planned: broken until the alpha variable FST fix; tracked in [#80](https://github.com/sillsdev/PanGloss/issues/80); git history |
| 117 | Ad hoc allomorph prohibitions covered only the last MSA copy of a multi-MSA stem | Fixed in Rust (PR #101, issue #99). Source: the Rust port's fwdata compile step (`pg-grammar/src/compile/lexicon.rs` kept one allomorph id per GUID; `compile/mod.rs` `strata_assign_co_occurrence` attached the rule to it). C# verdict: `hc.dll` and `HCLoader.LoadAllomorphCoOccurrenceRules` (`Src/LexText/ParserCore/HCLoader.cs:2212-2238`, `Permute` 2240-2258, FieldWorks commit `620cd9903f46941ee85947615dba15c8888c8e7a`) keep every copy and emit one Exclude rule per copy and per combination; an `hc.dll` replay of the equivalent XML (rule on the last copy only parses, on every copy no parse) confirms C# is correct and HC-Rust was wrong. Case `pg-parse/tests/adhoc_allomorph_copies.rs`; git history |

Issue numbers are in [sillsdev/PanGloss](https://github.com/sillsdev/PanGloss/issues); the `#NN` entries
above refer to work tracked there.

## Evidence and upstream reporting

For each active shared concern, record a Machine issue link, any fix PR or posted PR comment,
Rust implementation status, exact fixture location, and what was actually run at which commits.
An issue is not a fix PR; a posted comment is not a patch; a unit test is not a conformance grammar.
A green test with the fix disabled is not evidence that the fix is necessary. A grammar mutation
proves fixture sensitivity, not an engine fix-removed regression. Compare complete identity
multisets and statuses; skips, caps, missing rows and timeouts are not alignment passes.
Rust-only implementation bugs need not become Machine issues. Shared semantic concerns and
C# oracle dependencies do; classify hypotheses and coverage gaps without presenting them as bugs.

Audit: PanGloss `0006b938` (including Exact `149f88df`); Machine conformance base `8bad1934`;
Machine #493 merged at `52d069f8`. Live PR states checked 2026-09-15; 049 and 050 added 2026-09-25 against PanGloss `6ca152d0` (v0.4.0) and Machine conformance `34215889` (pin moved to `f412c252` on 2026-09-26, after #480 was rebased). Historical notes below
`history/` are dated evidence, not substitutes for checking the current head.
