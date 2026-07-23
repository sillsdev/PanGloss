## Context

Execution order, prerequisites, and exclusive ownership are governed by
`openspec/changes/STAGING.md`; its internal task groups are serial merge units.

HermitCrab and the Rust port's model are assumed complete. `model.rs` is a frozen compatibility
surface; ordinary development may fix semantics but does not add constructs or fields.

PanGloss needs evidence at four different levels: construct witness, interaction coverage, corpus
recall, and supported language. Existing gates sometimes prove only network reachability or one
recalled analysis per word.

## Decisions

- The ledger is a one-time audited snapshot of the frozen model and is versioned; hand-maintained prose is a rendered view.
- No source-AST parser or runtime reflection system is required. A future model-shape change is outside the standing product assumption and must deliberately reopen this coverage contract before merge.
- Each row records `compiled`, `overapproximated`, `peeled`, `confirm-only`, or `honest unsupported`.
- Correctness uses deduplicated Machine `WordAnalysis.Equals` identity: ordered stable morpheme IDs,
  root position, and category/POS. Rust `guessed` is a required separate Rust-parity annotation;
  duplicate multiplicity remains diagnostic evidence.
- Key semantic decisions begin with cited Machine and applicable LibLCM precedent. Preserve it by
  default; any divergence records rationale, compatibility effects, and focused proof.
- Every enabled recipe knob must own a necessary witness; dormant constructs fail non-vacuity.
- Semantic tests use deterministic limits; timing thresholds live in performance lanes.

## Dependencies

None. All later OpenSpec changes depend on this contract before claiming coverage.
