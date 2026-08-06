## 1. Inventory

- [ ] 1.1 Add `pg-grammar-gen` coverage-ledger schema v1 and perform a one-time reviewed inventory of the frozen public variants and behavior-bearing fields in `pg-grammar/src/model.rs`
- [ ] 1.2 Map every variant to compiler disposition, owning tests, positive witness, and negative witness
- [ ] 1.3 Render the ledger into maintained documentation and reconcile stale Phase B/C/P6 statuses

## 2. Gate contract v2

- [ ] 2.1 Define versioned oracle-analysis records and exact identity/multiplicity comparison in one shared test-support module consumed by Phase-C and corpus gates
- [ ] 2.2 Add public proposer-to-confirm containment helpers used by synthetic and real-language gates
- [ ] 2.3 Define and test a versioned canonical identity matching Machine `WordAnalysis.Equals`:
      ordered stable morpheme IDs, root position, and category/POS; keep Rust `guessed` separately
- [ ] 2.4 Audit existing FST parity gates that compare only `(morpheme_ids, root_index)` and extend
      semantic equality gates to detect category/POS differences without folding in gloss or shape
- [ ] 2.5 Add a key-decision precedent record citing relevant Machine and applicable LibLCM source,
      preserved behavior, any divergence rationale, compatibility impact, and focused tests
- [ ] 2.6 Resolve dense Rust morpheme/POS ordinals to HC XML keys or retained LCM GUIDs, declare the
      identity authority, and return typed `not_comparable` for missing/colliding keys
- [ ] 2.3 Make timeout, truncation, and unsupported status typed and non-certifying
- [ ] 2.4 Require each recipe knob to be necessary for at least one witness analysis

## 3. Verification

- [ ] 3.1 Convert `phase_c_multi_table`, `phase_c_right_to_left`, `phase_c_simultaneous`, and `p6_aweti_gate` to the new contract
- [ ] 3.2 Prove old word-level gates cannot silently pass an analysis-loss fixture
- [ ] 3.3 Validate all ledger rows have an explicit disposition and evidence owner; document that any future model-shape change must reopen this audit rather than silently extending the ledger
