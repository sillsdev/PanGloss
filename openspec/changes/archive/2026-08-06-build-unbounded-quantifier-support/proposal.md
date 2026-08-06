## Why

`compile-bounded-fst-quantifiers` deliberately scoped itself to finitely bounded quantifiers and left
the genuinely unbounded case (`max="-1"`, the DTD's Kleene sentinel) honestly unsupported. That
refusal was recorded as protecting ADR 0001: "a finite cutoff must never masquerade as unbounded
Kleene semantics."

Re-examined, the refusal rested on a premise that does not hold. The concern rules out *clamping* an
unbounded bound to a finite one — it says nothing against emitting a genuinely unbounded net, which is
what ADR 0001 would prefer. The FST backend has native, exact, finite-size constructions for it, and
the case is not rare: `max` DEFAULTS to `-1` in the loader, and Indonesian's `prule 2` uses it, so the
refusal blocked a construct in a reference grammar.

Full decision record: `docs/conformance/needs-decision-resolutions.md` (row 14).

## What Changes

- Widen the lowering IR's bounded-repetition slot to carry an optional upper bound, so an unbounded
  quantifier lowers rather than bailing.
- Render an unbounded repetition through the backend's own native Kleene operators, never a finite
  expansion — the preflight ceiling continues to apply to finite bounds only, because a native star's
  compiled size is independent of any repetition count.
- Widen the quantifier capability predicate so a rule whose only obstacle was an unbounded quantifier
  reaches confirm-only admission instead of refusal.
- Fix the mirror-rule construction for right-to-left rules to reverse a repetition group's own
  contents, not merely its position among sibling slots.

## Impact

FST proposal coverage only. The complete Rust HermitCrab engine remains the oracle and the
confirmation implementation, and the propose-and-confirm contract is unchanged: the widened
construction is exact, so it neither omits nor silently over-approximates.

The right-to-left fix closes a latent recall-loss path — a shallow reversal produced a mirror rule
that was not the reverse of the original whenever a repetition group's contents were not
palindromic — reachable ever since bounded quantifiers began compiling.
