# 004 — `Word::current_non_head()`: last-element read vs C#'s index-based read

## Kind
Behavioural (latent — not observed on any current fixture, but reachable via the public API).

## Status
Fixed-in-rust.

## C# site
`Word.CurrentNonHead` (Word.cs:453-461): `_nonHeadAppIndex == -1 ? null : _nonHeadApps[_nonHeadAppIndex]`
— an explicit index into `_nonHeadApps`, distinct from "the physically last element."

## Rust site
`pg_rules::word::Word::current_non_head` (`rust/crates/pg-rules/src/word.rs`). Before the fix, this
read `non_heads.last()` — the physically last pushed element — instead of indexing by
`non_head_app_index`.

## What differs
`.last()` and the index-based read agree only while `non_heads` never holds more than one
un-consumed entry beyond the confirmed ones — true for every grammar `csharp_port_compounding.rs`
builds, but not guaranteed by the public API: `pg_parse::Morpher::generate_words` pushes one
non-head per `GenMorpheme::NonHead` with no stem-count gate (analysis-side only), so two non-heads in
one `generate_words` call reach `non_heads.len() == 2`. After the first compound confirms,
`.last()` would incorrectly re-read the already-consumed non-head instead of the next one down the
index — a stale read that C#'s index-based access cannot produce.

## Can it change a parse?
Yes, in principle, on the direct-generation API (`generate_words`) with two non-heads in one call —
`current_non_head()` feeds `synth_compound`'s non-head gate directly. Not shown to be reachable via
`Morpher::parse_word` (the analysis→synthesis pipeline), only via the lower-level generation API.

## Evidence
`csharp_port_generation.rs::direct_api_compounding_two_non_heads_resolve_distinct_slots` exercises
the two-non-head generation path directly and passes with the index-based fix. The doc comment on
`current_non_head()` in `word.rs` names the fix and the reason to never "simplify" it back to
`.last()`.

## Upstream
None, not applicable — pure Rust-side bug, found while fixing entry 003.

## Notes
This is a good example of a bug that a narrower test suite (one exercising only `parse_word`, never
raw `generate_words` with multiple non-heads) would never surface — it was found by reasoning about
the public API's full reachable state space, not by a failing assertion.
