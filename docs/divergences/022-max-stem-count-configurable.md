# 022 — `Morpher.MaxStemCount`: hardcoded then made configurable

## Kind
Representational (was briefly behavioural, in the sense of an unreachable configuration, not a wrong
answer).

## Status
Fixed-in-rust.

## C# site
`Morpher.cs:56` (ctor default `2`) and `Morpher.cs:72` (`MaxStemCount`, a settable per-instance
property), read at `AnalysisCompoundingRule.cs:45`. C#'s own test suite raises it to 3
(`CompoundingRuleTests.cs:87,105`).

## Rust site
`pg_parse::morpher::Morpher` (`rust/crates/pg-parse/src/morpher.rs`), `max_stem_count` field and
`Morpher::with_max_stem_count`.

## What differs
`pg_parse::Morpher` hardcoded `max_stem_count: 2` with no constructor knob to raise it, so a
three-root compounding reconfiguration (mirroring C#'s own `MaxStemCount = 3` test usage) had no way
to reach 3 roots through the public API — not because `2` was an unfaithful default (`Morpher.cs:56`
sets `2` too), but because C#'s configurability itself was never ported. A genuine three-stem
compound the proposer could otherwise offer would be confirmed as zero analyses regardless, since the
confirm-time engine hardcoded the cap.

**Precise rule each side follows now:** both default to 2; both allow raising it per instance
(`with_max_stem_count` mirrors `new Morpher(...) { MaxStemCount = 3 }` exactly).

## Can it change a parse?
Only in the negative direction pre-fix: a grammar/caller wanting more than 2 compounded stems had no
way to express that in Rust, where C# could. Not a wrong-answer divergence on any grammar using the
default of 2.

## Evidence
`csharp_port_compounding.rs`'s port of `SimpleRules`' final reconfiguration exercises
`Morpher::with_max_stem_count(3)`. `docs/hermitcrab-rust-port-audit.md` §3a records this as "CLOSED
2026-07-25... The gap was the hardcoding; a `with_max_stem_count` builder now exposes it, default
unchanged." The existing per-`parse_word` step budget/timeout already bounds every candidate
regardless of this gate's value, per the field's own doc comment (the "never explode" argument).

## Upstream
None, not applicable — Rust converged to match C#'s existing configurability; no C#-side change
needed.

## Notes
`stratum.rs`'s `AnalysisRunConfig` documents `MaxStemCount`'s default (`2`) as "refuse to unapply a
compounding rule once [this many stems reached]" — worth checking against this entry if that
documentation and `Morpher`'s own default ever drift apart, since they are two separate fields that
must stay in sync by convention, not by shared storage.
