## 1. Freeze evidence inputs

- [ ] 1.1 Pin final commit, grammar/word-list hashes, toolchain, platform, and resource-policy version
- [ ] 1.2 Declare each language's corpus denominator, exclusions, timeout policy, and correctness unit
- [ ] 1.3 Validate the diagnostic/profile report schema versions and reject locally re-derived or incompatible measurement fields

## 2. Execute serial matrix

- [ ] 2.1 Run Sena with complete analysis-level evidence
- [ ] 2.2 Run Indonesian with explicit reduplication/peeler treatment
- [ ] 2.3 Run Amharic with timeout and partial-result semantics reported separately
- [ ] 2.4 Run Aweti through the shared compiled network and current support manifest
- [ ] 2.5 Capture cold build, warm p50/p95/p99/max, candidate precision, confirm share, sampled peak
      RSS with interval, and states/arcs

## 3. Certify and audit

- [ ] 3.1 Assign supported-language status independently using the common acceptance criteria;
      missing evidence yields `not_evaluated`, not a queue-wide implementation block
- [ ] 3.2 Open targeted follow-on changes for failures; do not patch code during the matrix run
- [ ] 3.3 Reconcile OpenSpec tasks, coverage ledger, FST plans, and stale historical claims
- [ ] 3.4 Publish the final dependency/status table and raw evidence locations
