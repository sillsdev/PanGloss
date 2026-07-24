## Decisions

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; this change is not dispatchable outside that graph.

**Reframed per `docs/adr/0001-honest-capability-boundary.md` and the 2026-07-24 `STAGING.md`
reorganization (Stage 4).** There is no terminal "certification" stage and no external reference
languages: correctness is proven by conformance integration tests over the in-repo **synthetic**
`machine/conformance/` grammars, diffed against committed oracle-authored ground truth, enforced as
the Stage 0A CI gate (`add-capability-characteristics-check`). This change's rename target is
`run-synthetic-conformance-matrix`.

- Each row consumes the diagnostic schema and prominently reports scanned words, oracle-producing words, oracle analyses, predeclared exclusions, `timed_out_before_any_result`, `timed_out_after_partial_result`, correctness unit, and pipeline.
- Cold compile and warm sequential lookup/confirm distributions are separate.
- Oracle time is separate from pipeline time; parallel wall and CPU-sum are separate.
- Heterogeneous recall percentages are never ranked across languages.
- The matrix reports evidence per synthetic conformance fixture, not a per-actual-language
  certification verdict; it fails closed on truncation, unsupported exercised constructs, or
  resource-envelope breach, but "supported" itself is decided by the Stage 0A characteristics-check
  gate, not assigned independently here.
- Runtime timeout, cap hit, cancellation, worker failure, or partial result makes correctness incomplete. Runtime timeouts are never retroactive exclusions; only predeclared versioned exclusions alter the denominator.
- Evidence is scoped to a named pipeline and resource-policy version. A required-stage breach
  blocks that pipeline; terminal resource failures never start another engine automatically.
- The historical actual-language reports (Sena/Indonesian/Amharic/Aweti) are retained as provenance
  only; their data is not migrated into the synthetic conformance suite, and no new identifier or
  fixture introduced under this change names an actual language (family/typology terms belong only
  in comments, per `STAGING.md`'s hard rule).

## Dependencies

Depends on completed `add-grammar-diagnostics`, applicable `profile-fst-compilation` phases,
`add-reference-hermitcrab-parity` when XML reference evidence is requested, the coverage contract
(now an evidence input to Stage 0A, per `define-grammar-coverage-contract`'s demotion), relevant
semantic compilers, hard resource safety, interaction coverage, and calibrated envelopes. It consumes
their versioned reports and SHALL NOT reimplement timing, gloss, completeness, or resource
calculations. It runs continuously in CI as part of the Stage 0A gate rather than once on a quiet
final commit; there is no separate terminal "certification" run for this change to trigger.
