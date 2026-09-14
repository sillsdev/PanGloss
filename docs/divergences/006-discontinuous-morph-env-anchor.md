# 006 — Discontinuous-morph environment anchoring (W3.3)

## Kind
Behavioural.

## Status
Fixed-in-rust. This is the exact case CLAUDE.md's oracle-hierarchy section names as the worked
example of why HC-Rust-only fixtures are dangerous — it is documented here in full for that reason.

## C# site
`Word.GetMorphs`/`MarkMorphs` (Word.cs; commit `97fa7721` in the port's own history for the split),
consumed per-annotation by `Allomorph.IsWordValid`'s environment clause (Allomorph.cs:110-125): every
morph *occurrence* (one `word.GetMorphs(allomorph)` entry) must independently satisfy an environment.

## Rust site
`pg_rules::morph::attribute_morphs` (`rust/crates/pg-rules/src/morph.rs`) for the record split, and
`pg_rules::validity` (`rust/crates/pg-rules/src/validity.rs`) for the environment check that consumes
those records.

## What differs
Before the fix, Rust's `attribute_morphs` emitted **one `MorphRecord` per morph**, merging a
discontinuous morph's non-adjacent pieces (a circumfix's two halves, or a root later split by an
infixing rule) into a single record spanning from the first piece to the last. `validity.rs`'s
environment check then anchored at that single merged span — which, for a discontinuous morph, is
not any of the morph's real occurrences, so the check could pass or fail based on context the real
C# per-occurrence check never looks at.

The fix makes `attribute_morphs` emit one `MorphRecord` per **contiguous run** of a morph's output
positions, mirroring C#'s `MarkMorphs` split exactly: a circumfix's two pieces become two separate
records, each checked at its own span, exactly matching C#'s per-annotation
`word.GetMorphs(allomorph)` loop.

**Precise rule each side follows:** C# checks an environment once per contiguous morph occurrence,
never once per whole (possibly discontinuous) morpheme. Pre-fix Rust checked once per morpheme, at a
span covering material the morpheme itself does not occupy.

## Can it change a parse?
Yes, demonstrated: pre-fix, Rust accepted both `xpitz` and `muat`; the C# oracle rejects both,
because the environment check fails specifically at the morph's *second* piece — a fact only visible
when each piece is checked at its own span.

## Evidence
`rust/conformance/allomorphy/discontinuous-env/` is an oracle-diffed fixture (verified against
`hc.dll`, not HC-Rust-only) built specifically to catch this. `pg-parse/tests/discontinuous_env_gate.rs`
replays it and is red-on-revert: collapsing `attribute_morphs`'s contiguous-run split back to one
record per morph makes `xpitz`/`muat` wrongly parse again.

## Upstream
None, not applicable — pure Rust-side bug (the single-merged-morph-record approximation); C# was
already correct.

## Notes
This is the case CLAUDE.md cites verbatim as the reason "an HC-Rust-only fixture over the same words
would have certified the bug as a Construct witness" — an HC-Rust-only fixture would have recorded
`xpitz`/`muat` as correctly parsing, since that was Rust's own (wrong) behavior, and nothing would
ever have caught the divergence from `hc.dll`.
