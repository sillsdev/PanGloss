## 1. Covering model

- [ ] 1.1 Derive pairwise factors and legal constraints from the coverage ledger
- [ ] 1.2 Generate deterministic covering arrays with stable case identifiers
- [ ] 1.3 Emit coverage manifests and fail on uncovered required pairs

## 2. Gates

- [ ] 2.1 Generate full-HC oracle witnesses and require complete analysis containment
- [ ] 2.2 Assert every enabled interaction changes or constrains at least one witness
- [ ] 2.3 Separate unsupported, resource-breached, truncated, and semantic-failure outcomes

## 3. Fuzz and minimize

- [ ] 3.1 Add seeded multi-knob fuzzing under hard resource supervision
- [ ] 3.2 Minimize each new failure to a stable named recipe and regression test
