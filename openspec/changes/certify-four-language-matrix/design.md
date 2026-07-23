## Decisions

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; this change is not dispatchable outside that graph.

- Each row consumes the diagnostic schema and prominently reports scanned words, oracle-producing words, oracle analyses, predeclared exclusions, `timed_out_before_any_result`, `timed_out_after_partial_result`, correctness unit, and pipeline.
- Cold compile and warm sequential lookup/confirm distributions are separate.
- Oracle time is separate from pipeline time; parallel wall and CPU-sum are separate.
- Heterogeneous recall percentages are never ranked across languages.
- Certification is per language and fails closed on truncation, unsupported exercised constructs, or resource-envelope breach.
- Runtime timeout, cap hit, cancellation, worker failure, or partial result makes correctness incomplete. Runtime timeouts are never retroactive exclusions; only predeclared versioned exclusions alter the denominator.
- Certification is scoped to a named pipeline and resource-policy version. A required-stage breach
  blocks that pipeline; terminal resource failures never start another engine automatically.

## Dependencies

Depends on completed `add-grammar-diagnostics`, applicable `profile-fst-compilation` phases,
`add-reference-hermitcrab-parity` when XML reference evidence is requested, the coverage contract,
relevant semantic compilers, hard resource safety, interaction coverage, and calibrated envelopes.
It consumes their versioned reports and SHALL NOT reimplement timing, gloss, completeness, or
resource calculations. Runs only after all code merges on a quiet final commit.
