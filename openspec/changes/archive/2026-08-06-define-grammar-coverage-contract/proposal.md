## Why

The current synthetic plan uses coarse construct rows, word-level reachability, and inconsistent
`DONE` labels. Those cannot support claims of complete HermitCrab construct or language coverage.

**Demoted to an evidence role** (per `docs/adr/0001-honest-capability-boundary.md` and
`openspec/changes/STAGING.md` Stage 0B): this ledger is no longer itself the gate. The load-bearing,
dynamic, hard-failing gate is `add-capability-characteristics-check`'s characteristics check. This
ledger's audited construct-by-construct inventory, oracle records, and witnesses are the evidence that
gate consumes when composing and matching its capability envelope.

## What Changes

- Produce a one-time, versioned coverage ledger auditing the frozen public semantic variants and behavior-bearing fields in `pg-grammar/src/model.rs`, authored as an evidence input **into** the Stage 0A characteristics check, not as a standalone pass/fail gate.
- Add reusable oracle records and proposer-to-confirm analysis-containment gates that the characteristics check's per-construct predicates can cite as evidence.
- Require positive/negative construct witnesses and explicit complete/truncated status, so the characteristics check has a non-vacuous basis for each predicate it composes.
- Reconcile stale Phase B/C/P6 planning status and forbid `honest unsupported` from meaning done.

## Impact

This ledger is the prerequisite evidence for `add-capability-characteristics-check` and, through it,
every subsequent semantic, interaction, scale, and conformance-matrix change. It changes test
infrastructure and documentation, not production parsing, and it does not itself hard-fail a
compilation — that authority belongs solely to the Stage 0A gate.

The HermitCrab model and its Rust port are treated as complete and closed except for bug fixes; this
change does not build permanent source-reflection machinery for hypothetical new features.

Implementation is dispatched as three serial merge units: inventory/schema, oracle identity and
containment library, then migration of named Phase-C and Aweti fixtures.
