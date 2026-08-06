# pg-foma precision.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/src/precision.rs` implementation comments so
the source can carry a one-line pointer instead of the full argument. Each section corresponds to
one call site; the site names the function/type so this doc can be found from either direction.

## `flag_id`: why the name field must be both dot-free and zero-digit-free

Two independently verified bugs drove this function's `ENV{id}` format (no dot, digit `0`
replaced with `Z`), both found by bisecting a real compiled network down to a single symbol.

**Dot-delimited fields.** foma-rs's `flag_check` DFA (`crates/foma/src/flags.rs`, a bug-for-bug
port of the real C table) treats every dot-delimited run after the type letter as another field. A
name containing a literal `.` (e.g. the old `"ENV.0007"`) makes a P/U/N/E-typed symbol (exactly two
fields allowed) invalid — not a flag at all, silently degrading to an ordinary literal multichar
symbol no real surface text can ever match — while an R/D-typed symbol (value optional) silently
splits at the embedded dot, giving every constraint the same flag name ("ENV") distinguished only
by value, i.e. one shared piece of cross-constraint state instead of independent ones.

**The digit `0`.** A literal `0` anywhere in a flag symbol's text breaks matching for the whole
symbol once it is spliced next to other text on the same lexc tape: `@P.ENV10.n@` and even the
lexc-escaped `@P.ENV1%0.n@` (`crate::tags::lexc_tag`'s own zero-escaping convention) both fail to
match at all when appended after a surface like `"seru"`, while `@P.ENV1Z.n@`, with the zero digit
replaced, works correctly. `lexc_tag`'s `%0` convention is only proven for a tag symbol occupying
an entire lexc side alone (its only use before this module) — a symbol spliced onto the end of
ordinary surface text is a materially different case, and escaping does not fix it there.

`flag_id` therefore avoids the digit `0` altogether (`Z` substitutes for it, and is never itself
produced by `u32::to_string`, so the substitution is injective) rather than escaping it.

## `tests/pk1_precision_recall_invariance.rs`: the recall-invariance harness

The knob is performance-only, so the composite propose→confirm path must reach IDENTICAL confirmed
analyses at `PrecisionConfig::Strip` (default) and `PrecisionConfig::AllFlags`, and the raw
candidate set must only ever SHRINK (never gain a candidate) between them.

**Why this file drives `apply_up`/peel/confirm directly instead of `FomaAnalyzer`.**
`FomaAnalyzer::new`/`FomaProposer::new` both hardcode `emit::emit(g)` (`PrecisionConfig::Strip`)
internally, and this file's ownership is scoped to `emit.rs`/`precision.rs`/`lib.rs`/its own tests
only — `analyzer.rs`/`composite.rs` are left untouched. So it reimplements the same
propose→confirm composite (`propose(word)` UNION `peel_candidates(word, propose)`, deduped by
`(morphemes, root_index)`, then `confirm_batch`) using only PUBLIC building blocks:
`emit::emit_with_precision` for a lexc source under either preset, the `foma` crate directly to
compile+`apply_up` it, and `pg_foma::peel`/`pg_foma::confirm`'s already-`pub` pieces verbatim — the
same peel/confirm machinery every other gate in this crate exercises, just re-plumbed to accept
either compiled network.

**Non-vacuity, against a knob that "passes" by doing nothing.** Indonesian declares zero
environment constraints at all, so `AllFlags` there is CORRECTLY byte-identical to `Strip` — proves
nothing about the mechanism biting. Sena DOES have real coverable instances
(`precision::tests::sena_catalog_finds_the_expected_left_literal_instances`), so this file asserts
for Sena specifically: the `AllFlags` lexc source actually contains flag-diacritic symbols, and at
least one Sena corpus word's raw candidate set is a STRICT subset under `AllFlags` (fewer
candidates than `Strip`, not just an equal-size coincidence) — i.e. the flags visibly prune
something, not merely compile to a no-op.

**Test-timing policy.** The default local test run must stay under ~60s and must not depend on the
gitignored real-language corpus fixtures at all — every test in this file loads one, so all four
are unconditionally `#[ignore]`d. `load_grammar`'s `Option`-returning self-skip keeps
`--include-ignored` runs green where the fixture is absent (CI); the `#[ignore]` is what keeps
them out of the default run at all, run speed aside. Run the full set locally with
`cargo test -p pg-foma --release --test pk1_precision_recall_invariance -- --include-ignored`.
