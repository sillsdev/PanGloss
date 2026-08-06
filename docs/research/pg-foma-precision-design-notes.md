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
