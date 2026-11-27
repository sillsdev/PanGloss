# deletion-reinsertion

Ledger entry (format: `.claude/skills/optimization-research/references/ledger-format.md`). Back to the [ledger](README.md).

- **Status:** measured (2026-11-27). No issue yet. The dominated-gap prune (env `HC_DEL_DOMGAP`, in `ana_narrow_deletion`) exists as an experiment and has not been recorded here yet.
- **Blowup it targets:** a deletion rule whose trigger is a common affix (here: a rule deleting two segments before a frequent suffix, no part-of-speech gate). Analysis re-inserts the deleted segments as an optional node at every site where the context holds; each variant shape re-runs every template and derivation rule. The extra work is proportional to how often the trigger occurs, not to how small the rule is.
- **Measured (release 0.35.0; Tamil grammar variants; work steps; 24 heaviest-tail words ("cap24") and 600 random words):**
  - The rule alone on the base grammar (other additions removed): 60.4M to 92.0M on cap24 (+52%); on the words containing the trigger sequence 4.2x per word, other words with a similar sequence 1.3x to 1.6x, words without it 1.00x. On the 600 random words +15.9% (274.2M to 316.9M).
  - A second rule of the same shape with a rare trigger: +1.4% on the random words.
  - Controls: 22 added noun entries +0.0% (cap24 and random); an added nominal suffix +24% (cap24) and +4.7% (random).
  - Full corpus: base 9.90G, grammar variant 13.24G (+34%); a sub-class-gated formulation of the same rule 13.21G, so the gate form does not change the cost. The growth is far larger than the roughly 1% grammar growth.
  - Per-object view (`pangloss stats <grammar> --group object --word <w> --wide`): morphological-rule attempts go from 2 to 10 up to 48 per rule, all `not_applied`, and work steps from 22K to 250K per rule on the one word examined.
- **Mechanism (candidate prunes, none built):** (a) a re-inserted segment must be consumed by the next morph, so drop re-insertion unless a later unapplication can supply the boundary and context; (b) a per-site feasibility check against the affix inventory at load. The morph boundary needed to decide this is not available at unapply time, which is why C# re-inserts optimistically.
- **Where in PanGloss:** `rust/crates/pg-rules/src/rewrite.rs` (`ana_narrow_deletion`: "re-inserted deleted segment is OPTIONAL"; `reapply_deletion`). In C#: `AnalysisRewriteRuleSpec` / `AnalysisRewriteRule` in sillsdev/machine (the same algorithm).
- **Expected gain:** measured ceiling: +52% on heavy words and +16% on random words for this one rule on this grammar; applies to every grammar with deletion rules keyed to affixes (Tamil, Bantu). Estimate for a sound prune: a fraction of that; unmeasured.
- **Parity risk:** medium. Sound prunes exist, but the exact C# behaviour including duplicate readings must be kept. Related: [optional-run-enumeration](README.md#optional-run-enumeration) (machine#635) is the matching-side cost of the same optional nodes; the Rust side already collapses interchangeable runs (divergence 130).
- **Memo/cache?** No.
- **Prior art:** none external; machine#635.
- **Issues:** none yet. Related: machine#635, #653.
- **Next step:** measure the dominated-gap prune (env `HC_DEL_DOMGAP`) for work and for identity against the C# oracle, with an adversarial fixture hunt; compare with `parse_compare.py --multiset` on the cap24 and random lists.
