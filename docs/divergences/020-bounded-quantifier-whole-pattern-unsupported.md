# 020 — Bounded `Quantifier` spanning a whole LHS/RHS: silently inert, not refused

## Kind
Unported.

## Status
Won't fix. The shape is XML-only: LibLCM models `PhSegmentRule.StrucDesc` as a sequence of `PhSimpleContext` (MasterLCModel.xml, class 128, prop 6), and a `PhIterationContext` is not a simple context, so no FieldWorks project can put a bounded quantifier as the whole structural description. No conformance fixture exercises the shape either (checked 2026-09-16: no grammar.xml in either root has a lone quantified sequence as a rule's PhoneticInput or PhoneticOutput). Closed without a loader lint; reopen only if HC-XML authoring of this shape becomes a supported input.

## C# site
The rule-spec constructors that build a `SynthesisRewriteRuleSpec`/`FeatureAnalysisRewriteRuleSpec`
etc.: every LHS/RHS child is cast to a constraint type a `Quantifier` node does not inherit from, so
**C# throws at grammar-load time** if a bounded `Quantifier` spans an entire LHS or RHS — even though
the format's own DTD and loader otherwise permit authoring the shape.

## Rust site
`pg_rules::rewrite::width_matches` (`rust/crates/pg-rules/src/rewrite.rs`) and its callers.

## What differs
A bounded `Quantifier` spanning the whole LHS or RHS of a rewrite rule compiles as **one node**
regardless of its min/max, so every caller's plain node-count check (`width_matches`) rejects any
real match wider than one segment. This isn't a crash and isn't a refusal at load time either — the
grammar loads without error, and the rule is simply structurally incapable of matching what its
author intended, with no diagnostic pointing at why. The quantifier's grouping is invisible to this
machinery, not merely mis-measured.

**Precise rule each side follows:** C# refuses to load this shape at all (a hard construction-time
type error). Rust loads it silently and the shape becomes functionally dead — a rule that can never
usefully fire as authored, no error, no warning.

## Can it change a parse?
Effectively yes, from the grammar author's point of view: a grammar C# would refuse outright (forcing
the author to rewrite the rule) instead loads "successfully" in Rust and produces a rule that behaves
nothing like what its shape suggests. Whether this is reachable by any grammar this repo currently
runs is unclear — the deliberate design note treats it as an accepted permanent restriction, not an
active bug, but nothing lints it either.

## Evidence
None — no test exercises this shape either loading or (attempting to) match through it, per the
source comment's own framing ("this is the deliberate choice... environments are the contrasting
case"). The comment names the environment case as the deliberate contrast: there, a quantifier is a
pure existence test with no positional array, so it is naturally handled correctly by the same
machinery that mishandles a target-pattern quantifier.

## Upstream
Not applicable in the usual sense — C#'s behavior here (an outright load-time crash) is not something
to converge on; the honest target is a Rust-side loader lint that refuses this specific shape the way
`GrammarError::Unsupported` already refuses other unimplemented constructs (`pg-grammar/src/load.rs`),
rather than loading it into a silently-dead rule.

## Notes
This is the one candidate in this catalogue that most directly risks the "control that cannot act
must fail loudly, not return quietly" rule this repo states elsewhere (CLAUDE.md, "A control that
cannot act must say so"): a rule that can never match as authored currently produces neither a
refusal nor an error, only silence. Recommended follow-up: add a loader-time
`GrammarError::Unsupported` for a bounded `Quantifier` spanning an entire LHS/RHS, converging on C#'s
refuse-rather-than-silently-limit behavior without reproducing its crash.
