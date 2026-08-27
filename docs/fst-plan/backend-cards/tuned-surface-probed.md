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
- Source references: src/emit.rs, src/junctions.rs, src/preexpand.rs.

Don't make any change that would make your language invalid!
