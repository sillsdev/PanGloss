# 005 — Prefix-commutes-with-compounding: a misdiagnosed "missing recursion" gap

## Kind
Stale-claim.

## Status
Not-a-bug. The original diagnosis in this port's own history was wrong; verified directly against
`hc.dll`.

## C# site
`AnalysisCompoundingRule.Apply` (cs:61-62) and `CompoundingRuleTests.cs:48-71`.

## Rust site
`pg_rules::morph::resolve_non_head_roots` (`rust/crates/pg-rules/src/morph.rs`).

## What differs
Nothing, once the test is read correctly. This case was previously `#[ignore]`d in the Rust port
under the claim "compounding analysis never recurses into the non-head" — implying an engine gap
where a non-head that is itself the product of a prefix rule (e.g. "didat" = prefix + "dat") could
never be resolved back to a bare root during compounding analysis.

That diagnosis misread `CompoundingRuleTests.cs:48-71`: the C# test inserts the tense prefix
*without* resetting `rule1.Subrules`, so `rule1` still carries reconfiguration 2's
`Rhs = { CopyFromInput("nonHead"), "+", CopyFromInput("head") }` (cs:31-39). Under that RHS, the
non-head is the literal root "pʰut" and the *affixed* span "didat" is the **head**, which simply
flows through the stratum's ordinary rule cascade after compounding unapplication (unapplying the
prefix rule the normal, non-compounding way) — it never needs compounding analysis to recurse into a
non-head at all. C# has, correctly, no such recursion:
`AnalysisCompoundingRule.Apply` discards any split whose non-head is not already a bare root — the
structurally identical gate `pg_rules::morph::resolve_non_head_roots` performs via a direct lexicon
search.

## Can it change a parse?
No. Both engines return empty for "pʰutdidat" under the (mis-ported) head+nonHead grammar
reconstruction, and both return `5+PAST+9|(pʰ)ut+?di+?dat` under the faithful nonHead+head grammar.

## Evidence
Verified directly against the live C# oracle (`hc.dll`) at test-authoring time — both the
mis-ported and the corrected grammar were run through `hc.dll` and matched Rust's output in both
cases. `csharp_port_compounding.rs::simple_rules_3_prefix_commutes_with_compounding` also notes a
related test-authoring wrinkle: C#'s root assertion (`AssertRootAllomorphsEquals(output, "9")`)
targets the head root, which is the *last* morpheme in this surface-ordered join, so
`root_gloss_set`'s first-morpheme heuristic (correct only for head-first compounds) cannot express
the assertion — the Rust test uses `WordAnalysis::root_morpheme_index` instead. That is a test-authoring
accommodation, not an engine divergence.

## Upstream
None, not applicable — no divergence exists.

## Notes
Kept as a catalogue entry specifically because the ORIGINAL claim ("compounding analysis never
recurses into the non-head") is exactly the kind of plausible-sounding, never-checked-against-the-
oracle claim this catalogue exists to prevent from calcifying. Anyone re-encountering a similarly
worded suspicion about non-recursive compounding analysis should re-read `CompoundingRuleTests.cs:48-71`
before trusting it.
