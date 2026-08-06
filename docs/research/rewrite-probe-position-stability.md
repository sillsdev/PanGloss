# Why the probing synthesis path soft-deletes instead of removing nodes

`pg_rules::rewrite::probe_narrow` / `probe_sim_narrow` back `hc_hybrid::surface`'s
`SurfacePhonology` port. They are text-identical to `syn_narrow`/`sim_narrow` except that the
matched-and-deleted target span is soft-marked (`ms.nodes[n].deleted = true`, left in place)
instead of `Vec::remove`'d; RHS insertion (`ms.nodes.splice`) is untouched.

## The C# invariant this preserves

C# never physically removes a node on deletion — `Annotation.FeatureStruct[Deletion] == Deleted`
is an ANNOTATION, so a node's POSITION in the shape's node list is stable for the entire life of a
`Word`, across as many phonological rules as run over it. `SurfacePhonology.RenderNodes` /
`SurfaceNodes` rely on exactly this: `outNodes.Skip(1)`/`.Take(underlyingLen)` slice by fixed
position regardless of what deleted in between.

This port's real `syn_narrow`/`sim_narrow` physically `Vec::remove`/`splice` instead — correct and
deliberately unchanged for the real per-word pipeline, which never needs cross-rule position
stability (each pipeline call starts from a freshly re-segmented, freshly-frozen `Shape`). Only the
probing path, which walks every affix underlying form against every alphabet representative and
needs stable positions across that walk, needs the soft-delete behavior.

## Why this is safe: the node-count arithmetic

The soft-delete reproduces C#'s own node-COUNT arithmetic exactly:

- A pure-deletion subrule (empty RHS) never changes total node count, matching C#, where deleted
  nodes are never removed.
- A subrule with a non-empty RHS increases total node count by the RHS length regardless of how
  many LHS nodes it "replaces", matching C#, where the new RHS nodes are real insertions on top of
  the still-present-but-deleted LHS span.

`hc_hybrid::surface`'s own final segment-count check (mirroring `SurfacePhonology.cs:152`'s
`outNodes.Count != underlyingLen + extra`) is therefore sufficient to reject every insertion case
exactly where C# would, without this module needing any special-case "bail" logic of its own.

## What has no C# analog here

`Kind::Epenthesis` (an empty-LHS rule) inserts nodes with no originating position whatsoever,
which this position-preserving model has nothing to anchor them to. `Kind::Metathesis` is likewise
out of scope. Neither of the three reference grammars has such a rule (verified: every
`PhonologicalRule`'s `PhoneticInput` is non-empty, and there are zero `<MetathesisRule>` elements),
so `probe_apply_rule_cached`/`probe_synthesize_stratum` return `ProbeOutcome::Refused` rather than
silently mis-tracking positions on either shape — a conservative stance for a case the gate
grammars never exercise, not a gap in the covered cases.
