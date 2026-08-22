# Tuned Surface Probed

> Static backend contract (schema v1); this card contains no language, corpus, timing, or machine observations.

- Backend ID: `tuned-surface-probed`
- Summary: Pre-probes surface and deletion junctions, then emits a whole-grammar relation.

## Capability envelopes

### `tuned-surface-closure` — Surface-probed composite closure
- Control: switch-controlled by `PG_FOMA_TUNED_SURFACE_CLOSURE_BUDGET`; default: `managed default`.
- Time: `O(E x J x P + N)`
- Space: `O(E + J + N)`
- Variables: E: emitted entries, J: junction variants, P: ordered rule count, N: composite states.
- Contributors:
  - E = emitted lexical entries
  - J = surface/deletion junction variants
  - P = ordered phonological rules
  - N = reachable composite states
  - Rule ordering changes probe reuse and the number of distinct junctions
  - Null realizations and deletion increase reachable zero-width and truncated branches
- Remedies:
  - `retry-larger-closure-envelope`
  - `use-loop-capable-backend`
  - `order-or-slot-localize-rules`
- Advice: [authoritative remedy text and shape-specific effort](../../../rust/crates/pg-foma/assets/backend-advice-v1.toml). A remedy would make this backend work for your language only when its stated prerequisites hold.
- Source references: src/emit.rs, src/junctions.rs, src/preexpand.rs.

Don't make any change that would make your language invalid!
