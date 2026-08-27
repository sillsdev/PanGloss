# Templated Underlying Tokens

> Static backend contract (schema v1); this card contains no language, corpus, timing, or machine observations.

- Backend ID: `templated-underlying-tokens`
- Summary: Emits underlying token lanes and applies the whole rewrite cascade.

## Capability envelopes

### `templated-underlying-rewrite` — Underlying-token rewrite cascade
- Control: inherent; always part of this backend's contract.
- Time: `O(E x P x T x F)`
- Space: `O(E + P x T x F)`
- Variables: E: emitted entries, P: ordered rewrite rules, T: template/token lanes, F: feature/unification cost.
- Contributors:
  - E = emitted lexical entries
  - P = ordered rewrite rules and their environment composition
  - T = template obligations and token lanes
  - F = feature/unification cost and fan-out
  - Rule ordering affects cascade depth and intermediate alphabets
  - Null and deletion rules add epsilon/truncation branches to the relation
- Remedies:
  - `regularize-phonology`
  - `order-rules`
- Advice: [authoritative remedy text and shape-specific effort](../../../rust/crates/pg-foma/assets/backend-advice-v1.toml). A remedy would make this backend work for your language only when its stated prerequisites hold.
- Source references: src/emit.rs, src/replace.rs, src/enumerate.rs.

Don't make any change that would make your language invalid!
