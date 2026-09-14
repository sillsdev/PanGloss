# 003 — Compounding homophone-disjunction collapse (`Word::non_heads.pop()`)

## Kind
Behavioural.

## Status
Fixed-in-rust. Verified against the live C# oracle (`hc.dll`).

## C# site
`SynthesisCompoundingRule.ApplySubrule` (cs:248-291) and `Word`'s copy constructor (Word.cs:105):
`_nonHeadApps` is cloned forward on every copy and never has an entry removed;
`MorphologicalRuleApplied` (Word.cs:411-429) only moves the separate `_nonHeadAppIndex` pointer
backward on confirmation. The consumed non-head is retained as permanent history specifically so two
literal-homophone entries (byte-identical surface, different lexical entries/glosses) remain
distinguishable at dedup time.

## Rust site
`pg_rules::morph::synth_compound_subrule` (`rust/crates/pg-rules/src/morph.rs`). The bug: this
function called `w.non_heads.pop()` after folding the non-head into the compound's `shape`, on the
theory that a "consumed" non-head could be discarded.

## What differs
Compounding "pʰut" with either of two literal-homophone non-head lexical entries ("dat"/N and
"dat"/V — identical surface, different part of speech and gloss) must produce two distinct analyses.
Rust's `pop()` erased the consumed non-head's `Word` from `non_heads` before `Word::dedup_key()` ran,
so the two candidates' dedup keys became identical (since `dedup_key()` recurses into `non_heads`,
and the discriminating entry was gone) and `Morpher::parse_word` silently folded the two homophones
into one analysis, dropping a real, distinct parse.

## Can it change a parse?
Yes — this is not a hypothetical, it is a demonstrated recall loss: two genuinely distinct analyses
collapse into one whenever a compound's non-head has two or more homophonous lexical entries.

## Evidence
`csharp_port_compounding.rs::simple_rules_1_homophone_disjunction_finding` ports
`CompoundingRuleTests`'s `AssertMorphsEqual` (gloss strings compared pre-deduplication) and fails
before the fix, passes after. `Word::dedup_key()` itself was verified NOT to be the bug (it already
faithfully recurses into `non_heads`) by reading `Word.ValueEquals`/`FreezeImpl` (Word.cs:508-546)
directly — the fix is the deletion of the `pop()` call, with a code comment at that site citing this
finding, not a change to the dedup key.

## Upstream
None, not applicable — this was a pure Rust-side bug; C# was already correct.

## Notes
See entry 004 for a second, related bug found while fixing this one (`current_non_head()`'s
last-element vs index-based read), which the `pop()` fix's own test coverage does not exercise.
