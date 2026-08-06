# Compounding port divergences (`pg-parse/tests/csharp_port_compounding.rs`)

Findings from porting `CompoundingRuleTests` from the C# HermitCrab test suite
(`tests/SIL.Machine.Morphology.HermitCrab.Tests/MorphologicalRules/CompoundingRuleTests.cs`).
Each entry names the divergence, its root cause (traced against the live C# source, not assumed),
and the fix. VERIFIED against `hc.dll` where noted.

## Homophone-disjunction collapse (`simple_rules_1_homophone_disjunction_finding`)

Compounding "pʰut" with either of two literal-homophone non-head entries ("dat"/N and "dat"/V,
byte-identical surface) must produce two distinct analyses, matching C#'s `AssertMorphsEqual`
(gloss strings compared pre-deduplication). `Morpher::parse_word` folded them into one.

Root cause: not `Word::dedup_key()` — that is a faithful, narrow port of C# `Word.ValueEquals`/
`FreezeImpl` (Word.cs:508-546) and already recurses into `non_heads`, whose `root_allomorph`
field would distinguish the two entries if `non_heads` still held the consumed child `Word` at key
time. `pg-rules/src/morph.rs`'s `synth_compound_subrule` called `w.non_heads.pop()` after folding
the non-head into the compound's `shape`, on the theory that the non-head was "consumed". C#'s
`SynthesisCompoundingRule.ApplySubrule` (cs:248-291) and `Word`'s copy constructor (Word.cs:105) do
the opposite: `_nonHeadApps` is cloned forward and never has an entry removed —
`MorphologicalRuleApplied` (Word.cs:411-429) only moves the separate `_nonHeadAppIndex` pointer
backward on confirmation (already faithfully ported as `non_head_app_index -= 1` in
`pg-rules/src/stratum.rs`'s `guided_synth`). The consumed non-head was meant to remain as permanent
history for exactly this dedup disambiguation; the `pop()` erased it.

Fix: delete the `pop()` (see `pg-rules/src/morph.rs`'s comment at that site). `dedup_key()` itself,
and its ~20 call sites in `stratum.rs`/`morpher.rs`, are unchanged.

### A second, related bug: `Word::current_non_head()`

Read `non_heads.last()` (the physically last element) instead of C#'s index-based
`_nonHeadApps[_nonHeadAppIndex]` (Word.cs:453-461). The two agree only while `non_heads` never
holds more than one un-consumed entry beyond the confirmed ones — true for every grammar this test
file builds, but not guaranteed by the public API: `pg_parse::Morpher::generate_words` pushes one
non-head per `GenMorpheme::NonHead` with no stem-count gate (analysis-side only), so two non-heads
in one generation call reach `non_heads.len() == 2`. After the first compound confirms, `.last()`
would incorrectly re-read the already-consumed non-head instead of the next one down the index.

Fix: `current_non_head()` is now index-based. See
`csharp_port_generation.rs::direct_api_compounding_two_non_heads_resolve_distinct_slots` and
`word.rs`'s doc comment on the method.

## Prefix-commutes-with-compounding misdiagnosis (`simple_rules_3_prefix_commutes_with_compounding`)

Previously `#[ignore]`d as a "compounding analysis never recurses into the non-head" engine gap.
That diagnosis was wrong: the original port misread `CompoundingRuleTests.cs:48-71`, which inserts
the tense prefix without resetting `rule1.Subrules`, so `rule1` still carries reconfiguration 2's
`Rhs = { CopyFromInput("nonHead"), "+", CopyFromInput("head") }` (cs:31-39) — the non-head is
"pʰut" (a literal root) and the affixed span "didat" is the head, which simply stays as the word's
shape after compounding unapplication and flows through the stratum's ordinary rule cascade, where
the prefix rule unapplies ("didat" -> "dat" -> root "9").

C# has no recursive re-entry into the rule cascade for a non-head: `AnalysisCompoundingRule.Apply`
(cs:61-62) explicitly discards any split whose non-head is not already a bare root — structurally
the same direct lexicon search `pg_rules::morph::resolve_non_head_roots` performs. Verified against
the live C# oracle (`hc.dll`): the old (mis-ported) head+nonHead grammar returns empty for
"pʰutdidat" in C# too, and the faithful nonHead+head grammar returns
`5+PAST+9|(pʰ)ut+?di+?dat` in both engines.

The C# root assertion (`AssertRootAllomorphsEquals(output, "9")`) targets the head root, which is
the last morpheme of the surface-ordered join here — `root_gloss_set`'s first-morpheme heuristic
(correct only for head-first compounds) cannot express it, so the test asserts via
`WordAnalysis::root_morpheme_index` instead.

## Count-blind assertions (`simple_rules_5_three_root_compound_two_rules`)

`AssertMorphsEqual`/`AssertRootAllomorphsEquals` are defined over a C# `HashSet`/`.Distinct()`
(`HermitCrabTestBase.cs:869-887`, `CompoundingRuleTests.cs:241-244`) — set membership only, never
raw analysis count — matching `assert_morphs_eq` and this test's own root-set check, both of which
dedupe the same way. Rust may (and empirically does) surface the same final compound via more than
one derivation history when two distinct rules can each supply either split point in an unordered
cascade; that duplication is exactly as C#-faithful as the count-blind assertions checked against it.

## `ProdRestrictRule`'s six reconfiguration steps (`prod_restrict_rule`, cs:174-238)

The C# test's six sequential reconfigurations of one in-memory grammar become six grammars, in the
C# step order (each step's entry-side `MprFeatures` state carries over exactly as the C# mutations
leave it — e.g. step 4 still has the head feature on entry `5`, because C# only removes it in step
5):

1. No restrictions — parses as C# does: both dat homophones, `{"5 8", "5 9"}` (previously pinned
   at the known-collapsed `{"5 8"}` only, tracking the homophone-disjunction bug above — now fixed,
   not a new divergence).
2. `headProdRestrictionsMprFeatures` set, no entry carries the feature — no parse.
3. + entry `5` (the head root) carries it — parses again, both homophones: `{"5 8", "5 9"}`.
4. Restriction moved to `nonHeadProdRestrictionsMprFeatures` (entry `5` still carries the
   now-irrelevant feature) — no parse (neither dat entry carries it).
5. Feature moved from entry `5` to entry `8` — parses as `{"5 8"}` only, matching C# exactly (the
   "5 9" split dies: entry `9` doesn't carry the feature, pinning that the gate is per-entry, not
   per-shape).
6. Also `outputProdRestrictionsMprFeatures` — still parses `{"5 8"}` (the output feature is added
   to the produced word's MPR set, never a parse-blocking input gate; C# additionally asserts the
   output set contents on the in-memory rule object, which has no parse surface here).

## `Morpher::with_max_stem_count` (`SimpleRules`' final reconfiguration, cs:76-108)

C#'s `Morpher.MaxStemCount` (Morpher.cs:72) is a settable per-instance property, ctor default `2`
(Morpher.cs:56). `pg_parse::Morpher` hardcoded `max_stem_count: 2` with no constructor knob to
raise it, so the three-root reconfiguration (`MaxStemCount = 3`) had no way to reach 3 roots
through the public API. `2` was always a faithful *default*; hardcoding it also dropped C#'s
configurability. `Morpher` now carries a `max_stem_count` field (default `2`, unchanged) plus
`Morpher::with_max_stem_count`, mirroring C#'s `new Morpher(...) { MaxStemCount = 3 }` usage
exactly — see `pg-parse/src/morpher.rs`'s field doc on `max_stem_count` for the "never explode"
argument (the existing per-`parse_word` step budget/timeout already bounds every candidate
regardless of this gate's value).
