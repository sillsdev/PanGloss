## Why

The current synthetic plan uses coarse construct rows, word-level reachability, and inconsistent
`DONE` labels. Those cannot support claims of complete HermitCrab construct or language coverage.

## What Changes

- Produce a one-time, versioned coverage ledger auditing the frozen public semantic variants and behavior-bearing fields in `pg-grammar/src/model.rs`.
- Add reusable oracle records and proposer-to-confirm analysis-containment gates.
- Require positive/negative construct witnesses and explicit complete/truncated status.
- Reconcile stale Phase B/C/P6 planning status and forbid `honest unsupported` from meaning done.

## Impact

This is the prerequisite for every subsequent semantic, interaction, scale, and language-certification change. It changes test infrastructure and documentation, not production parsing.

The HermitCrab model and its Rust port are treated as complete and closed except for bug fixes; this
change does not build permanent source-reflection machinery for hypothetical new features.

Implementation is dispatched as three serial merge units: inventory/schema, oracle identity and
containment library, then migration of named Phase-C and Aweti fixtures.
