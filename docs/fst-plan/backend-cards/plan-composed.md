# Plan Composed

> Static backend contract (schema v1); this card contains no language, corpus, timing, or machine observations.

- Backend ID: `plan-composed`
- Summary: Materializes the controllable, content-addressed portion of the enumerated plan.

## Capability envelopes

### `plan-composed-materialization` — Controllable plan materialization
- Control: inherent; always part of this backend's contract.
- Time: `O(G x R x F + Q)`
- Space: `O(G + R x F + Q)`
- Variables: G: gate groups, R: rewrite rules, Q: required plan subtrees, F: feature/unification cost.
- Contributors:
  - G = reachable gate groups
  - R = rewrite rules in authored order
  - Q = required plan subtrees
  - F = feature/unification cost and fan-out
  - Rule ordering changes the content-addressed replacement cascade
  - Null, deletion, and structural marker leaves can require unsupported subtrees
  - Branching multiplies gate-group and replacement combinations
- Remedies:
  - `use-whole-grammar-backend`
  - `implement-required-plan-subtrees`
  - `use-obligation-templates`
- Advice: [authoritative remedy text and shape-specific effort](../../../rust/crates/pg-foma/assets/backend-advice-v1.toml). A remedy would make this backend work for your language only when its stated prerequisites hold.
- Source references: src/enumerate.rs, src/plan.rs, src/build.rs.

Don't make any change that would make your language invalid!
