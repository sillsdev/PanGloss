# Phase 2 sub-plan: Metathesis rules (W4) — COMPLETED

> **OUTCOME (2026-07-08, landed on `rust`):** full port shipped as planned —
> `pg-rules/src/metathesis.rs`, authored-`Group` lowering in the pattern bridge, analysis
> feature-exchange + synthesis physical splice, `RuleCache` compilation. Along the way it also
> found and fixed a real `pg-fst` RightToLeft traversal bug. Empirical constraint discovered
> against the C# oracle: switch groups are always 1 node wide in practice — wider groups crash
> the real C# engine, so Rust pins that rather than "fixing" it into divergence. All 3
> MetathesisRuleTests ported and passing (`csharp_port_metathesis.rs`), 3 oracle conformance
> fixtures frozen (`rust/conformance/metathesis/`), `metathesis_conformance.rs` replays them.
> All three corpora stayed byte-identical (none uses metathesis).
> The plan below is preserved as the design rationale/C# map. Nothing remains open.

**Status at planning time:** zero Rust lines exist; `pg-grammar/src/load.rs:376` hard-rejects any grammar containing
`<MetathesisRule>` (whole load fails — correct stopgap, becomes wrong once this lands).
**Effort:** L. **Dependencies:** none — the `Group`/`get_offsets` capture primitive it needs already
exists in `pg-fst` and is production-proven by Tier-2 #12 and `morph.rs::compile_parts`.
Source: audit `rust/parity-out/audit/phase2/B-phonology-parity.md` §4 (full C# read).

## C# implementation map (.worktrees/parse-opt/src/SIL.Machine.Morphology.HermitCrab/PhonologicalRules/)

- **`MetathesisRule.cs`** (37 lines) — model: `Direction`, a full match `Pattern` containing two
  named `Group`s, `LeftSwitchName`/`RightSwitchName` (the group names to swap).
- **`AnalysisMetathesisRule.cs`** (69) — wraps `AnalysisMetathesisRuleSpec` in
  `IterativePhonologicalPatternRule`, `MatcherSettings{ Direction=reverse(rule.Direction),
  Filter=Segment|Anchor, Unification, UseDefaults=true, Nondeterministic=true }`.
- **`AnalysisMetathesisRuleSpec.cs`** (132) — the analysis mechanic:
  - Ctor rebuilds the pattern in three zones: pre-group nodes as-is; **the two groups re-added in
    swapped order** (constraints cloned + `Modified=Clean` pinned); post-group nodes as-is. The
    compiled matcher is pre-swapped, so a forward match on the post-metathesis surface locates the
    pre-metathesis material directly.
  - `MatchSubrule` always true (no environments, no MPR gating — see edge cases).
  - `ApplyRhs`: overall span = min start/max end over ALL group captures; then for each
    `(leftNode,rightNode)` pair zipped from the two groups' ranges (Segment-typed only), **union
    each node's FeatureStruct into the other** and mark both dirty. Analysis does NOT physically
    reorder nodes — it's a feature-content exchange at fixed positions.
- **`SynthesisMetathesisRule.cs`** (67) — forward direction, `Filter=Segment|Boundary|Anchor &&
  !IsDeleted()`, deterministic (no Nondeterministic flag).
- **`SynthesisMetathesisRuleSpec.cs`** (143) — the physical reorder:
  - Ctor keeps document order (no swap), groups pass through with `Modified=Clean`.
  - `ApplyRhs`: resolve both groups' shape-node ranges, then physically swap the ranges via
    `MoveNodesAfter` (Remove + AddAfter per Segment node, marked dirty). **Non-Segment nodes
    (mid-span boundaries) do NOT move** — segments jump over a boundary that stays put.
  - Morph annotations spanning the region are removed and rebuilt from children's new positions.
  - The C# file carries a "RUSTIFY Stage 2" comment (lines 78-80): group-capture OFFSETS go stale
    across structural mutation — resolve captures to node references BEFORE mutating. Free guidance;
    in Rust terms: materialize both ranges as index lists first, then splice.

## Rust implementation shape

New module `pg-rules/src/metathesis.rs`, driven from the same stratum plumbing as `rewrite.rs`
(analysis: iterative loop over `MutShape`; synthesis: same). Steps:
1. **Loader:** replace the `load.rs:376` rejection with a real `MetathesisRuleDef` (pattern +
   left/right switch group names + direction) in `model.rs`. DTD: check every attribute against
   `XmlLanguageLoader`'s metathesis branch (attrs beyond the pattern: id, direction, mult-app?).
2. **Bridge:** `PatternBridge::compile_nodes` has NO `Group` case today (Tier-2 #12's groups are
   synthesized internally, not authored). Add authored-`Group` lowering to `CompileNode::Group`.
3. **Analysis:** build the swapped pattern (mechanical child reorder), compile once into the
   `RuleCache`, match with the analysis settings above, and on match union lane pairs
   (plain `MutNode.lanes |=`, mirroring `syn_feature`'s per-node mutation style). Remember the
   char-def identity dimension: after a union the node's identity is broadened — follow the
   `syn_feature` precedent (reset `char_def` to `NO_CHAR_DEF` on modified nodes) unless the C#
   FS-union semantics say otherwise (StrRep unions on zero-feature grammars — verify against
   `CharacterDefinitionTable` behavior before choosing; document the choice).
4. **Synthesis:** on match, materialize both group ranges as node-index vectors, then splice
   `MutShape.nodes` (the same primitives `syn_narrow` uses), skipping non-Segment nodes in place.
   Rebuild any `MorphRecord`-relevant ordering if a morph span straddles the swap (check how
   `MorphRecord.order` is derived — spans are derived, not stored, per Tier-1 #5's design).
5. **Caching:** both compiled patterns go in `RuleCache` (grammar-static).

## Fixtures (port C# `MetathesisRuleTests.cs`, 3 tests, + 2 new)
- `SimpleRule` — adjacent CV→VC swap, both directions (analysis un-swap + synthesis re-swap
  round-trip).
- `ComplexRule` — non-switch context nodes before/after the groups.
- `SimpleRuleNotUnapplied` — negative case: pattern doesn't match → no fire.
- NEW: boundary node physically inside the switch span (the MoveNodesAfter skip branch).
- NEW: confirm metathesis subrules cannot be MPR-gated (both C# specs hardcode
  `IsApplicable=true`) — pin that with a test so nobody "fixes" it into divergence.

## Verification
Standard protocol (master plan §protocol). All three corpora must be byte-identical (no corpus
uses metathesis); the new fixture tests carry the correctness load. Oracle-diff each fixture
grammar through the C# engine to generate expected outputs before writing assertions.
