# pg-rules analysis_syn_fs_gate.rs: analysis-side syntactic-FS accumulation must narrow, not widen

Regression gate: analysis-side syntactic-FS accumulation must narrow with `PriorityUnion` (the
required value overwrites the accumulated value, per feature) rather than widen with `Add` (a
per-feature value-set union), at the two C# call sites that changed: `AnalysisAffixProcessRule.cs`
(`outWord.SyntacticFeatureStruct.PriorityUnion(_rule.RequiredSyntacticFeatureStruct)`) and
`AnalysisCompoundingRule.cs` (identical, for `HeadRequiredSyntacticFeatureStruct`).

## Why `PriorityUnion` is right

Synthesis computes `PriorityUnion(Unify(stem, rule.Required), rule.Out)` — the rule's output always
wins over whatever the stem carried. Un-applying that rule must be able to recover a stem consistent
with `rule.Required`, so the analysis side priority-unions in the required value the same way
`SynthesisAffixProcessRule` priority-unions in the output value: the requirement replaces the
accumulated value per feature, it does not merely get added as an alternative alongside it. Add's
value-set union let a stale accumulated value survive alongside a fresh, disjoint requirement,
which let a later rule's `IsUnifiable` gate pass against a value the stem never actually had.

## The fixtures

`analysis_syn_fs_gate.rs` keeps the original two-rule chain (a rule-level `<RequiredHeadFeatures>`
inner rule feeding an `<OutputHeadFeatures>`-gated outer rule, in a 3-symbol `num` feature so the
fork isn't the "union covers everything" corner case `pg-featstruct`'s own unit tests already
cover), and adds a second fixture — a category-change chain (head feature `cat`, symbols `n`/`v`)
porting the upstream C# unit test
`AnalysisAffixProcessRule_CategoryChangeChain_RequiredOverridesAccumulatedPos`. It uses
`RequiredHeadFeatures`/`OutputHeadFeatures` rather than `<RequiredPartsOfSpeech>`/
`<OutputPartOfSpeech>` — the semantics under test (`ana_syn_fs`'s `PriorityUnion`) are identical
either way.

### `analysis_required_fs_overrides_accumulated_value_priority_union`

- **Control**: `unify(sg, pl) == None` — sg/pl are disjoint, so this is a genuine fork.
- **Rule 1 ("inner")**: `RequiredHeadFeatures = num:pl`, priority-unioned onto a `num:sg` word.
  Exactly one candidate, and its FS is exactly `pl` — not the old widened `{sg, pl}` (asserted via
  `add(sg, pl, mask)` for contrast).
- **Rule 2 ("outer")**: `OutputHeadFeatures = num:pl`'s `is_unifiable` gate passes directly against
  the narrowed `pl`, with no need for a widened multi-bit lane.

### `analysis_affix_process_rule_category_change_chain_required_overrides_accumulated_pos`

- Word FS starts at `cat:n` (the surface POS after synthesis: root --n2v--> V --v2n--> N).
- Unapplying `v2n` (`Required=V`) leaves the FS at exactly `V`.
- Unapplying `n2v` (`Required=N`) leaves the FS at exactly `N` — not the old widened `{N, V}`
  (`add(v, n, mask)`, asserted for contrast, and separately shown unifiable with `V` to prove the
  old value really would have leaked `third` through).
- `analyze(g, after_n2v, third)` (`Out=V`) is empty: `V` is not unifiable with the narrowed `N`.
  Under old `Add` semantics the accumulated `{N, V}` would still overlap `V` and wrongly admit a
  third rule application the chain's actual FS state cannot support.
