# 018 — Realizational-FS diff: Rust avoids a latent C# null-crash

## Kind
Behavioural.

## Status
Open — a deliberate Rust-side choice, not proposed upstream. This is exactly the shape CLAUDE.md's
oracle-hierarchy rule warns about ("anything Rust believes is an improvement must be proposed
upstream... rather than silently kept"), and it has not been proposed.

## C# site
`Word.ExpandAlternatives` (Word.cs:515-524), specifically its realizational-FS diff step:
`Unify(diff, out newFS)`. C# calls `FeatureStruct.Unify`'s **out-parameter overload**
(FeatureStruct.cs:1043-1047) and discards its own boolean success return. When the unify fails, that
overload sets the out-parameter to C#'s `null` — and `Unify` here is called for its side effect only,
with the return value never checked, so `alternative._realizationalFS` becomes `null` with no
downstream guard against it. Any subsequent C# code path that reads `_realizationalFS` on this
alternative without a null check would crash.

## Rust site
`pg_rules::word::Word::expand_alternatives` (`rust/crates/pg-rules/src/word.rs`), the `real_fs`
accumulation block.

## What differs
Rust's `unify` returns `Option<FeatureStruct>`. Where C# silently produces a `null` on unify failure
and has no caller-side guard, Rust's `expand_alternatives` explicitly leaves `alt.real_fs` **unchanged**
on that failure (`Option::None` from `unify`) rather than reproducing the null. This is a deliberate,
documented choice — not an oversight — described in the function's own doc comment as "strictly
safer than reproducing a crash."

**Precise rule each side follows:** C# replaces `alternative._realizationalFS` with the unify result
unconditionally, crashing later if that result is null. Rust replaces it only when the unify
succeeds, otherwise leaving the prior value in place.

## Can it change a parse?
In principle yes — a case where C#'s unify fails would, in C#, either crash (if something reads the
null) or (if nothing does) proceed with a `null` realizational FS wherever the crash doesn't
actually trigger, which is a different runtime object than Rust's "keep the old value" choice. The
doc comment states this divergence "is never exercised by any oracle-verified fixture" — i.e., in
every grammar tested so far, a real family's shape-equivalent alternatives never carry genuinely
conflicting realizational feature diffs, so the unify never actually fails on real input. This is
stated as an observation, not a proof that it can never fail.

## Evidence
None — not exercised by any oracle-verified fixture, per the source doc comment itself. This is a
documented, reasoned divergence, not a measured one.

## Upstream
None, needed. Per this repo's own stated policy, this divergence should be proposed to
`sillsdev/machine` as a fix to `Word.ExpandAlternatives`'s discarded-success-flag bug (C#'s own
`Unify(diff, out newFS)` call ignoring its boolean return is a real latent defect, independent of
what Rust does about it) rather than left as a silently-kept improvement.

## Notes
This is one of a small number of entries in this catalogue where Rust's behaviour is believed
*better* than C#'s, which is precisely the category CLAUDE.md's oracle-hierarchy section asks to be
surfaced rather than quietly kept. Flagging for follow-up: file the upstream PR, or at minimum record
here why it hasn't been.
