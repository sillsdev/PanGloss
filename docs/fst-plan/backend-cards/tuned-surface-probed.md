# Tuned Surface Probed

> Static backend contract (schema v1); this card contains no language, corpus, timing, or machine observations.

- Backend ID: `tuned-surface-probed`
- Summary: Pre-probes surface and deletion junctions, then emits a whole-grammar relation.

## Capability envelopes

### `tuned-surface-closure` — Surface-probed composite closure
- Control: inherent; always part of this backend's contract.
- Time: `O(E x J x P x F + N)`
- Space: `O(E + J + N x F)`
- Variables: E: emitted entries, J: junction variants, P: ordered rule count, F: feature/unification cost, N: composite states.
- Contributors:
  - E = emitted lexical entries
  - J = surface/deletion junction variants
  - P = ordered phonological rules
  - F = feature/unification cost and fan-out
  - N = reachable composite states
  - Rule ordering changes probe reuse and the number of distinct junctions
  - Null realizations and deletion increase reachable zero-width and truncated branches
  - Closure work is measured as static cost evidence against the managed internal budget
- Remedies:
  - `use-loop-capable-backend`
  - `order-or-slot-localize-rules`
- Advice: [authoritative remedy text and shape-specific effort](../../../rust/crates/pg-foma/assets/backend-advice-v1.toml). A remedy would make this backend work for your language only when its stated prerequisites hold.
- Source references: src/emit.rs, src/junctions.rs, src/preexpand.rs.

Don't make any change that would make your language invalid!
