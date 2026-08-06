# pg-rules widening_gate.rs: analysis-side syntactic-FS accumulation must widen, not narrow

Regression gate: analysis-side syntactic-FS accumulation must widen (`FeatureStruct.Add`, a
per-feature value-set union) rather than narrow (`unify`, an intersection) at the three C# call
sites (`AnalysisAffixProcessRule.cs:63-68`, `AnalysisCompoundingRule.cs:133-138`,
`AnalysisAffixTemplateRule.cs:66`).

## The fixture

Models the motivating Amharic pattern directly: two chained `MorphologicalRule`s. The first
carries a rule-level `<RequiredHeadFeatures>`, accumulated via `Add` onto the analysis candidate's
syntactic FS. The second gates its own `Apply` on
`OutSyntacticFeatureStruct.IsUnifiable(input.SyntacticFeatureStruct)`
(`AnalysisAffixProcessRule.cs:46-49`) against whatever the first rule left behind. A 3-symbol `num`
feature (`sg`/`du`/`pl`) makes the accumulation a genuine multi-bit (disjunctive) lane, not merely
the "delete when the union covers everything" corner case `pg-featstruct`'s unit tests already
cover directly.

- The root starts at `num=sg`; the inner rule requires `num=pl`.
- Narrowing (`unify(sg, pl)`) is disjoint and fails outright — the pre-fix Rust behavior at
  `morph.rs`'s `ana_syn_fs` fell back to the unchanged `sg` value rather than rejecting the
  candidate (its own divergence from C#), but the practical effect on the chain is the same: the
  outer rule's gate sees `sg`, not `pl`.
- Widening (`add(sg, pl)`) unions to `{sg, pl}` (a real two-bit lane) — not disjoint from `pl`, so
  the outer rule's `is_unifiable` gate against `num=pl` still passes.

The outer rule's `Apply` therefore produces zero candidates under narrowing and one under widening:
the chain dies without the fix and survives with it.

## `num_fs`

Builds `{head: {num: symbol}}` the same way the XML loader would (`build_syn_fs`/`load_syn_fs`), so
the test doesn't depend on any lexicon/`AssignedHeadFeatures` machinery, just the bare feature
system the two rules already reference.

## `analysis_chain_survives_only_because_add_widens_not_narrows`

- **Control**: confirms this is a genuine narrowing-vs-widening fork, not a vacuous fixture — `sg`
  and `pl` are disjoint at `num`, so a real `unify` fails outright on this exact pair (the operation
  the pre-fix Rust code used in `ana_syn_fs`).
- **Rule 1 ("inner")**: rule-level `RequiredHeadFeatures = num:pl`, added onto the candidate's
  syntactic FS on unapply. One candidate; its widened FS must retain both `sg` (from the input) and
  `pl` (from the requirement) as a real two-bit lane — not narrowed to just `pl`, and not silently
  left at just `sg`.
- **Rule 2 ("outer")**: gates its own `Apply` on `IsUnifiable` against `OutputHeadFeatures = num:pl`.
  Under narrowing, rule 1's output would carry only `sg`, and `is_unifiable({pl}, {sg})` fails,
  killing the chain. Under the fix, rule 1's output carries `{sg, pl}`, which overlaps `{pl}`, so
  the gate passes and the chain survives.
- **Negative control**: replaying rule 1's step with `unify` instead of `add` (the pre-fix narrowing
  operator) on this exact input reproduces the failure the fix eliminates, pinning down why the old
  code could never have produced a survivable candidate here.
