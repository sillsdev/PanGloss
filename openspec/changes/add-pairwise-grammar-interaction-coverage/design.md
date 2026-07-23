## Decisions

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; this change is not dispatchable outside that graph.

- Pairwise covering arrays are primary evidence; random fuzzing is a secondary discovery tool.
- The pair universe and legality constraints are versioned with the pinned coverage-ledger version; reports include required, covered, uncovered, illegal, and contains-unsupported counts.
- Required high-risk combinations include table × alpha × strata, direction × mode, quantifier × environment, compounding × template/phonology, peeler × template/phonology, and constraints × tags/confirm.
- Every generated case must have necessary witnesses and complete correctness enumeration.
- Failures are minimized under the same resource envelope and committed as named recipes.

## Dependencies

Depends on ledger schema v1 and a pinned post-Stage-2 ledger revision after multi-table, RTL, Simultaneous, and every dispatched remaining-construct subsection has merged. Unsupported pairs remain visible exclusions, not passes. `interaction_coverage=complete` requires zero uncovered required pairs.
