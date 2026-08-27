# Quantifier / `OptionalSegmentSequence` compile gate (`tests/phase_c_quantifier.rs`)

Pins the loader/compiler's ACTUAL disposition per quantifier shape, so a regression (an
out-of-scope shape silently mis-compiling, or a now-supported shape silently regressing back to a
bail) is caught. `pg_foma::replace::pattern_slots` returns `None` on any out-of-scope
`PatternNode::Quantifier` it meets in a rewrite rule's LHS/RHS/environment (inverted-finite,
alpha-nested, or empty-children), which surfaces as `skipped.push(rule.xml_id.clone())`.

## Bounded quantifiers now compile

A finitely bounded, alpha-free quantifier (`min`/`max` both concrete) compiles via
`pg_foma::replace::Slot::Repeat`, foma's native `^{min,max}` bounded-repetition xre operator.
`quantifier_bounded_environment_compiles_and_matches_oracle` is the containment fixture: a bounded
quantifier inside a rule's right environment, checked against `pg_parse::Morpher` at both its
`min` and `max` boundary occurrence counts, plus a negative control below `min`.

## Genuinely unbounded quantifiers also now compile

The construct's original, unbounded (`max="-1"`) shape used to be this file's honest-skip witness.
`Slot::Repeat`'s `max: Option<u32>` widening now compiles this shape too (foma's native
`*`/`^>N` unbounded-repetition operator), so both witnesses are renamed and flipped to prove the
new disposition. The former finite-max ceiling is removed; large finite bounds now flow through
native lowering. An inverted-finite, alpha-nested, or empty-children quantifier stays unsupported
as before.

## Why the environment, not the LHS/RHS focus

This is a documented, load-bearing choice, not an arbitrary one. `pg_rules::rewrite::width_matches`
has a "shared width-mismatch guard" requiring a matched span's physical width to equal the rule's
raw `lhs.nodes.len()`/`rhs.nodes.len()` — a plain node count that is always exactly 1 for "one
`Quantifier` node occupies the whole LHS/RHS", regardless of how many physical segments it
actually consumes. A Quantifier match whose real width differs from that fixed count (any
`max > 1`, or a `min == 0` zero-occurrence skip) is silently discarded by this guard before the
RHS is ever applied — independent of this change; the guard predates it and exists for an
unrelated scenario that merely also catches this one. This is a real, pre-existing, now-surfaced
confirm-engine gap, documented in `pg_foma::replace`'s module doc ("Confirm-engine finding") rather
than silently worked around, following the same "recall-preserve, don't paper over" discipline as
the RTL precedent (`tests/phase_c_right_to_left.rs`'s "Known, out-of-scope oracle gap" section).

A `Quantifier` used INSIDE an environment has no such gap: `pg_rules::rewrite::left_env_match`/
`right_env_match` test only first-match existence (`Transduce::first_match`) against a
`PatternBridge::compile_pattern`-compiled (Quantifier-faithful) environment FST, never a
positional per-node array — no width count to mismatch. This file's containment fixture therefore
places its quantifier there, where exact oracle equality is provable today, following the same
`fst_candidate_set`/`oracle_candidate_set` methodology `tests/phase_c_right_to_left.rs` and
`tests/two_table_symbol_divergence.rs` already use.

## Why the environment fixture gives every segment its own feature value

`quantifier_env_xml` gives one distinct symbol value per SEGMENT, not per natural-class
membership, matching `tests/phase_c_right_to_left.rs`'s `RTL_FEATURE_ENV_XML` finding:
`pg_parse::Morpher`'s analysis-side unapplication needs this to disambiguate segments. A grammar
with no `PhonologicalFeatureSystem` at all silently failed to fire the rule during synthesis at
all (`Morpher::generate_words` returned the root's raw, un-rewritten spelling unchanged) — a
genuine, pre-existing `pg_rules::rewrite`/zero-phonological-feature-grammar interaction this
fixture works around the same way the RTL fixture already does.
