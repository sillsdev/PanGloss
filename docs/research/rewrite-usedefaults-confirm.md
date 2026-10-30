# `UseDefaults` confirm step for phonological rewrite rules

`pg_rules::rewrite::pattern_defaults_ok` is a post-hoc confirm step that recovers C#'s
`MatcherSettings.UseDefaults` behavior, which `pg_fst`'s compiled FST cannot itself apply.

## What C# does

C#'s phonological rewrite-rule matcher constructs its `Matcher` with `UseDefaults = true`
(`Analysis/SynthesisRewriteRule.cs:29-37`; `AnalysisMetathesisRule`/`SynthesisMetathesisRule` set
it too, but Metathesis is unported here). The flag flows into `Fst.Transduce`
(`SIL.Machine/FiniteState/Fst.cs:283-330` -> `TraversalMethodBase._useDefaults` ->
`Input.Matches`, `SIL.Machine/FiniteState/Input.cs:49-64`) and from there into
`FeatureStruct.IsUnifiable`/`Subsumes` (`FeatureStruct.cs:994-1017,1085-1114`): for a feature the
*pattern* side pins that the *data* side leaves unset (no `_definite` entry), C# substitutes the
feature's `DefaultValue` for the unset side and re-checks unifiability/subsumption against *that*,
instead of treating "unset" as vacuously compatible with anything.

## Why `pg_fst` cannot do this itself

`pg_fst`'s frozen contract has no analog of "unset vs. explicitly full-mask" (`rewrite.rs`'s own
module doc already flags this as a gap the FST "cannot apply"), so the FST's own match is
`useDefaults=false`-equivalent: a `full_mask` (this port's "unspecified") segment lane always
overlaps any LHS-pinned constraint, over-approximating exactly like the other confirm-step gaps
this module already patches post-hoc (alpha variables via `resolve_bindings`, the
`Type`/`Modified`/`Deletion` symbolic features via the aux bits on `MutNode`).

## The confirm algorithm

`pattern_defaults_ok` is the analogous confirm step for `UseDefaults`: for each LHS-pinned feature
at each matched target position, if the actual node's lane is `full_mask` *and* the feature has a
`default_symbol` (`pg_grammar::featsys::PhonFeatureSystem::default_bits`), the candidate is only
really valid if the default's bits intersect the LHS's pinned bits — mirroring C#'s
`else if (useDefaults && featVal.Key.DefaultValue != null)` branch.

## Scope

Rewrite environments default missing input lanes before matching, including the traversal that
binds variables through quantified patterns. Allomorph environments retain their separate matcher
settings. Narrow synthesis and nonempty Narrow analysis targets, and Epenthesis analysis targets,
also use the target confirmation. Rewrite target variables bind the default when the input lacks
their feature. The rule tests exercise these paths, alongside the existing Feature target fixture.
Three staged `rewrite-default-*` fixtures replay environments, Narrow targets, and Epenthesis
targets against the pinned C# engine. Each detects a difference without the fix.

The full-mask distinction is an observable remaining gap: changing the epenthesis fixture's `a`
to explicitly specify both `nas` symbols makes C# accept `bat` as `|b[ad(aa)]t`, while this
port treats the full mask as absent, substitutes `nas=+`, and rejects it. The staged fixture
itself deliberately leaves `nas` absent and agrees with C#.

## Parameter shapes

`pattern_lanes[k]` is target position `k`'s **full** `W`-wide lane row (every feature, not just the
pinned ones — `node_full_lanes`'s shape, one row per target-pattern node position; for synthesis
this is the LHS's own lanes, for analysis it's the analysis target's combined `LHS ⊕ RHS`, matching
whichever pattern the caller's `target: &Fst` was compiled from). A feature is "pinned" at position
`k` iff `pattern_lanes[k][f] != full_mask(g, f)`. `target_nodes[k]` is target-pattern position `k`'s
already-resolved shape-node index — see `resolve_bindings`'s doc for why this isn't always a
contiguous `node_of[s+k]` slice.
