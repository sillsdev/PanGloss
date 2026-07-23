## 1. Shared IR

- [ ] 1.1 Define the internal lowered pattern/environment IR and typed unsupported outcomes
- [ ] 1.2 Lower anchors, polarity, groups, alternation, table identity, and quantifier metadata
- [ ] 1.3 Add positive/negative unit witnesses for every IR node and combination boundary

## 2. Migration

- [ ] 2.1 Adapt existing replacement callers without changing their network semantics
- [ ] 2.2 Thread existing logical work accounting through the lowering seam
- [ ] 2.3 Prove enabled/disabled result and network-fingerprint equivalence for supported rules

## 3. Verification

- [ ] 3.1 Run the focused multi-table, RTL, Simultaneous, and lower-module commands from `design.md`
- [ ] 3.2 Update only ledger rows for the lowering seam; do not claim quantifier or metathesis support
