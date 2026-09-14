# 019 — Root-allomorph trie previously mis-indexed pattern allomorphs

## Kind
Behavioural.

## Status
Fixed-in-rust.

## C# site
`Morpher.cs:39-47`: the constructor partitions each stratum's lexical entries so that a
pattern-shaped allomorph (`IsPattern`, `Morpher.cs:43-44` — e.g. a bare `[Any]*` root) is never
placed into the ordinary `RootAllomorphTrie`; only non-pattern allomorphs are trie-indexed.

## Rust site
`pg_parse::root_trie::RootAllomorphTrie::build` (`rust/crates/pg-parse/src/root_trie.rs`).

## What differs
Before the fix, this module's own doc describes a "prior (wrong) 'index everything' placeholder": a
lexical-pattern entry fell through to a single mandatory unrestricted trie edge, which could match
**any** one-segment word during ordinary (guess-off) lexical lookup. The fix diverts `IsPattern`
allomorphs into `Morpher::lexical_patterns` (`collect_lexical_patterns`) instead, mirroring C#'s
partition exactly — pattern allomorphs are reachable only through the guesser's matching path
(`pg-parse/src/guess.rs`, entry 027), never through ordinary trie search.

## Can it change a parse?
Yes: pre-fix, any one-segment word could spuriously match a lexical-pattern root during ordinary
lookup — a clear over-generation bug, and one that would be silently absorbed by
`Morpher::parse_word`'s downstream gates only by coincidence, not by design.

## Evidence
The module doc names this as "a real divergence from C#, which never trie-indexes a pattern
allomorph at all." No dedicated named regression test was found for this specific fix during this
research pass — **unverified** whether a standalone unit test exists beyond the structural fix and
its doc comment; the general pattern-vs-lexical-lookup split is exercised indirectly by the guesser
test suite (`pg-parse/src/guess.rs`'s unit tests) but that suite tests the guesser path, not the
ordinary trie-lookup path this bug affected.

## Upstream
None, not applicable — pure Rust-side bug; C#'s partition was already correct.

## Notes
Flagging the missing dedicated test as a gap in this catalogue's own evidence base, not just in the
port: a regression test asserting that a one-segment word does NOT spuriously match a lexical-pattern
root via ordinary (guess-off) lookup would close this evidence gap directly.
