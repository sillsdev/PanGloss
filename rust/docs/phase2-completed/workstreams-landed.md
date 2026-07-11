# Phase 2 — landed workstreams (the "already done" record)

This is the permanent record of Phase-2 work that has LANDED on `rust`, with the rationale that
drove each decision. The active plan (`rust-optimizations-phase2.md`, repo root) contains only
what remains. Sub-plans with their own design docs live alongside this file
(`metathesis-w4.md`, `narrowing-budget-w8.md`, `test-port-w11.md`, `evidence-w12.md`).

**Git note:** `rust` was squashed 2026-07-08 to a single commit on master; pre-squash history is
on branch `rust-pre-squash-backup` — commit hashes below from before the squash resolve only via
that branch. Post-squash hashes (`9cf31048`, `6642a29d`, `92d2d5da`, `1abb849c`, `94228b16`,
`99f72024`, …) are on `rust` directly.

## Context: the audits that scoped Phase 2

Four per-region parity audits at `rust/parity-out/audit/phase2/` (gitignored; regenerate by
re-running the audit protocol): `A-morphology-parity.md`, `B-phonology-parity.md`,
`C-loader-parity.md`, `D-test-coverage-map.md`, all at pre-squash HEAD `cdace6fa`. They
re-verified every phase-1 finding (FIXED/STILL-OPEN/WRONG with code citations) and swept the full
C# surface: the complete DTD (72 elements / ~140 attrs — every attr ported, documented
dead-in-C#'s-own-loader, or a named gap) and C#'s 68 `[Test]` methods (at audit time: 8 true
equivalents, 28 different-scenario, 4 no-test, 23 blocked, 5 no-surface). Audit C's dead-in-C#
attribute list is permanent: those are correct non-implementations, never gaps.

## W1 — Hardening sweep (LANDED)

Six items, all shipped:
1. **Shared width-mismatch guard** consolidated across `ana_feature`/`ana_epenthesis`/
   `syn_feature`/`syn_narrow`. The over-wide-`ENTIRE_MATCH` span bug (Optional nodes transparently
   consumed widen a match beyond the pattern) had been fixed independently twice and was still a
   silent-wrong-mutation at `ana_epenthesis` and an **engine panic** (`rhs_pins[k]` positional
   indexing, no bounds check) at the synthesis sites. **Important correction discovered later
   (verified in C# source):** the guard is NOT a port — C#'s Narrow/Epenthesis Unapply loops are
   count-bounded (mark exactly `_targetCount` nodes) and structurally immune; the guard is Rust
   catching up to a safety C# gets for free.
2. **Tier-2 #11 anti-FS negation** in `ana_feature` (`mask & !bits` idiom from `bind_or_check`).
3. **`syn_narrow` alpha-variable RHS resolution** (the `rhs_vars` step `syn_feature` had).
4. **Stopgap lints:** `RewriteMode::Simultaneous` (was parsed, silently treated as Iterative) and
   `requiredPOS` on phonological subrules (was silently always-false) now hard-reject at load;
   63/64-symbol boundary lint tests added.
5. **Doc corrections** (bridge.rs/segment.rs/morph.rs stale claims).
6. **`IsTemplateRule` distinction:** template-interaction checks in `synth_affix*` now gated on
   whether a template slot references the rule (C# `SynthesisAffixProcessRule.cs:64-105`).

`requiredPartsOfSpeech` gate later landed correct-but-inert on Amharic (wave 3): all 3 real
subrules using it are Narrow merges, which were blocked until W8 — W8 was the payoff gate.

## W2 — Loader completeness N1/N2/N3 (LANDED)

All three audit-C findings shipped: **N1** `PhonologicalFeatureSystem@isActive` honored
(`.SingleOrDefault(IsActive)` semantics, not last-block-wins); **N2** phonological
`defaultSymbol`/`UseDefaults` modeled and threaded into the rewrite matcher's per-lane confirm
step; **N3** root-allomorph `PhoneticShape` pattern-language fallback (`[NatClass]`,
`([NatClass])`, `[NatClass]*`) ported as a root-allomorph-loader-only path, matching C#'s
`GetShapeNodes(allowPattern=true)` scoping. Dedicated gate tests:
`loader_n{1,2,3}_*_gate.rs`; N3's end-to-end oracle replay went live after wave 4's root_trie
CdSet fix (the test was `#[ignore]`d on exactly that gap until then).

## W3 — Silent-wrong semantics (LANDED)

- **MPR feature groups** `MprGroupMatchType::All/Any` + `MprGroupOutput::Overwrite` implemented at
  `mpr_ok` and the `w.mpr.union` sites (was loaded-but-flattened-to-overlap). Oracle fixtures in
  `rust/conformance/mpr-groups/`.
- **Disjunctive-allomorph / free-fluctuation final re-check** (the long-deferred #5d, the audits'
  #1-ranked morphology gap): per-rule-application passed-over allomorph indices
  (C# `appliedAllomorphIndices`) + the `Allomorph.cs:127-152` rejection loop in `validity.rs`.
  **Fixed a real divergence** and un-ignored W11's disjunctive test. Keying detail that matters:
  the re-check keys on the ORIGINAL allomorph per `Allomorph.cs:137`. An OR-semantics pin test
  guards the disjunction interpretation.
- **Discontinuous-morph environment span derivation** — probing fixture built; **fixed a real
  mis-anchoring divergence** (`discontinuous_env_gate.rs`, fixture in
  `rust/conformance/allomorphy/discontinuous-env/`).

## W4 — Metathesis (LANDED) → see `metathesis-w4.md`

## W5 — Realizational cluster (LANDED, pre-squash `3a85851c`, 335 tests)

The largest unported morphology surface, previously hard-linted. The whole cluster turned out
XML-expressible: StemName regions; `LexFamily` + `CheckBlocking` (implemented as a post-pass,
argued equivalent to C#'s 3 inline call sites); `RealizationalAffixProcessRule` as a third
`MorphRuleDef` variant (C# has no `MaxApplicationCount` for it — Rust uses `u16::MAX`);
`ChooseInflectionalStem` + a `real_fs` pre-gate (previously inert); presence-only `IsBlocked`;
`expand_alternatives` realizational-FS diff via a new `hc-featstruct::subtract`. 3 oracle fixtures
MATCH (`rust/conformance/realizational/`); D-batch-3 ported (StemNames 12/12, RealizationalRule,
ProdRestrictRule). Kept linted with scope notes: FootFeatures. **Surprise worth remembering:** the
C# oracle accepts `RealizationalRule` directly in a stratum's `morphologicalRules` — the DTD
comment is misleading.

## W6 — Co-occurrence rules (LANDED, pre-squash `e4c19c69`, 322 tests)

Both `MorphemeCoOccurrenceRule` and `AllomorphCoOccurrenceRule`, line-for-line at the
`validity.rs` gate (shared with W3.2's re-check). 3 fixtures (`rust/conformance/cooccurrence/`),
+2 MorpherTests. **Oracle-XML authoring gotchas recorded:** no `--` inside XML comments,
globally-unique `id` attrs, strict DTD element order.

## W7 — Generation API (LANDED, pre-squash `47550d8b`, 344 tests)

`generate_words` + `generate_words_from_analysis` (reusing `WordAnalysis`), `GenMorpheme` enum,
`mrule_apps: Vec<Option<MRuleId>>` for C#'s null compounding slot (proven parse-path-inert),
left-side-reversed `PermuteOtherMorphemes` interleave (mechanically proven vs C# with a
discriminating fixture), `hc_generate_words` FFI (WordAnalysis overload only — the 3-arg
realizational-FS overload has no wire encoding, scoped out) + `hc-rs generate` CLI. FFI
round-trip parse→regenerate 15/15 (`generate_round_trip.rs`).

**UPDATE (P8, 2026-07-10): the "5 GenerateWords assertions stay unported" scope cut below is
STALE — they were ported in a W11 batch-7 follow-up, before this note was corrected.** Original
text, kept for history: ~~**Ratified scope cut:** 5 GenerateWords assertions in
AffixProcessRuleTests Suffix/PrefixRules stay unported (same multi-stratum grammar shape
`csharp_port_affix_process.rs` already omits) — tracked in the finish plan's P8.~~ In fact
`rust/crates/hc-parse/tests/csharp_port_affix_process.rs`'s `suffix_rules` test (module doc
"W11 batch-7 remainder") already ports all 5 `GenerateWords` round-trip assertions from
`AffixProcessRuleTests.cs:418-437` (entries 32/33 × sSuffix/edSuffix, entry 34 × edSuffix),
using `Morpher::generate_words` once it landed later in this same workstream. `PrefixRules` has
no `GenerateWords` calls in its C# body at all, so there was never anything to port there. Verified
2026-07-10: `cargo test -p hc-parse --test csharp_port_affix_process suffix_rules` passes. No
outstanding GenerateWords scope cut remains for this test file; P8(b) closed with no code change.

## W8 — Budget model + narrowing (LANDED) → see `narrowing-budget-w8.md`

Headline: global per-`parse_word` `StepBudget`, general narrowing/expansion analysis, the
`untruncate()` phantom-wildcard fix (found by a Fable agent), and the "pathological family"
false-alarm resolution. Follow-ups O1/O2/V1/P1 in the finish plan.

## W9 — Oracle-diff probes (DONE) + waves 3-4 fix-downs

11 conformance fixtures frozen (`rust/conformance/{affix-shapes,rewrite}/`), report at
`rust/parity-out/audit/phase2/W9-probe-report.md`. Initial verdicts: MATCHES — infix, circumfix,
noncontiguous, expand; DIVERGES — truncate, merge/multiplemerge (expected, W8-gated), quantifier,
multiplesegment, disjunctive, longdistance. **W9.3 verdict: nothing real uses
`RewriteMode::Simultaneous` — keep the lint** (revisit only if a real grammar ever trips it).

**Wave 3** (pre-squash `af86887a`, 325 tests): ONE root cause — char_def/cd_set staleness after
lane widening — flipped FOUR diverging fixtures at once (multiplesegment, longdistance,
quantifier, disjunctive) and un-ignored 3 tests. Lesson: apparent multi-fixture divergence
clusters deserve a shared-root-cause hunt before per-fixture fixes.

**Wave 4** (pre-squash `b1237d8d`, 354 tests): truncate marker fixed (floating MarkMorph fallback
rides pure-truncation hops — W9's truncate fixture MATCHES; all 4 affix-shape fixtures pinned by
`affix_shapes_conformance.rs`); root_trie CdSet edges fixed (made N3 end-to-end live);
subsumed_affix fixed via a `MorphStatus` 4-state enum (Real/Floating/SubsumedChild/SubsumedFirst
— two "separate" findings were two halves of C#'s `ApplyRhs` fallback branch). Precise findings
docs left for what wave 4 did NOT fix: anchor_rules cross-table StrRep (→ P5), boundary_rules
bare-root epenthesis = syn_epenthesis word-initial (→ P1), deletion multi-position reinsertion
power-set (→ P2).

## W10 — Sena full corpus (C# baseline DONE; Rust run + diff in flight)

C# master baseline complete: `rust/parity-out/golden/master/sena-full.tsv`, all 7121 words, via
the watchdog wrapper (`run-sena-baseline.ps1`: STARTED sentinel + TSV-growth liveness + 150s
stall-kill + `--start=N` resume). vs sena-fast gold: 299/302 byte-identical; the 3 diffs are
master-too-slow timeouts, not disagreements. **Gotchas that will bite again:** sena-fast gold
(Jul 4) and current sena-words.txt (Jul 6) diverge in word ORDER past idx 299 — always join by
word text, never by index; idx 2649 `mudapionawo`'s TIMEOUT row is a budget-wall cut, not a
measured 150s timeout. The Rust-side equivalent runner is `rust/tools/run-sena-rust.ps1`
(`--threads 1` required — it's the only per-word-flush mode; stderr not stdout carries the
completion/panic markers). Remaining W10 work → finish plan V2.

## W11 — C# test-suite port (SUBSTANTIALLY DONE) → see `test-port-w11.md`

## W12 — Evidence engine baselines (DONE; fuzzing pending) → see `evidence-w12.md`

## Cross-cutting: landing protocol + agent-ops lessons

- **Landing protocol that works:** rebase branch onto `rust` in its worktree → clippy + full
  tests + Indonesian/Amharic gates on the rebased result → ff-merge → force-remove worktree
  (**check untracked-but-gitignored deliverables first** — a blanket `*.tsv` gitignore once
  silently excluded every conformance expected.tsv and worktree removal deleted them; fixed with
  `!/conformance/**/expected.tsv`) → delete branch.
- **Subagent discipline:** hardened briefs (commit incrementally, never reset, no background
  waits); two spurious "usage policy" API kills were resumed cleanly via SendMessage with a
  re-grounding brief. When a Fable agent hit its usage limit mid-task, its draft fix was
  preserved/committed first and a Sonnet agent continued with a full-findings brief — no loss.
- **Sonnet capability boundary (John, 2026-07-09):** several deep parsing-semantics items failed
  under Sonnet and needed Fable (the W8 untruncate root-cause is the canonical case). The finish
  plan flags each remaining item with a model recommendation.
