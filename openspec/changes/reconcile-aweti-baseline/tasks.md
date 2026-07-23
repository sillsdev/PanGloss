## 1. Baseline

- [ ] 1.1 Pin grammar hash, code commit, supported/unsupported rule manifest, and 104-word denominator
- [ ] 1.2 Persist exact recalled and missed analysis records for the honest baseline
- [ ] 1.3 Replace stale 68/104 requirements in the Aweti plan and related docs

## 2. Shared network

- [ ] 2.1 Extract a compiled templated-network constructor used by gate and diagnostics
- [ ] 2.2 Assert matching fingerprint, states, arcs, and rule dispositions across both callers

## 3. Correctness and timing

- [ ] 3.1 Diagnose the bare-root boundary and add a minimal oracle-backed regression
- [ ] 3.2 Implement only the demonstrated bare-root fix; preserve every pre-fix recalled analysis and multiplicity, and record newly recalled analyses in a separately attributed post-fix manifest
- [ ] 3.3 Add named-stage instrumentation with complete/truncated status
- [ ] 3.4 Measure a stratified word set and then the declared corpus on a quiet machine
- [ ] 3.5 Publish a decision report with measured attribution, projected gain, correctness/resource constraints, selected lever or `no safe lever`, and the exact scope for a new bounded OpenSpec change; do not implement the optimization here

## 4. Verification

- [ ] 4.1 Confirm no unsupported rule is silently compiled
- [ ] 4.2 Confirm exact analysis containment/multiplicity and report the bare-root fix's realized end-to-end change separately from any future optimization
