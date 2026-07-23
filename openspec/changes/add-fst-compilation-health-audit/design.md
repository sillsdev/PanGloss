## Decisions

- Depend on `define-fst-compilation-health`, coverage inventory, budget APIs, and applicable compile
  profile events. Consume each metric from its owner exactly once.
- Preflight reports constructs, quantifier/alternative products, alpha tuples, templates/slots,
  predicted emitted work, peeled/confirm-only expansion, and unknown/unbounded work without foma.
- Treat semantic and cost uncertainty differently. A disposition that might omit a HermitCrab
  analysis fails closed. Unknown growth in a recall-preserving construction is reported and then
  attempted under the worker watchdog and logical work budgets; uncertainty alone is not Critical.
- Observed audit adds actual emitted bytes/lines, per-stage time, intermediate/final states/arcs,
  FST payload bytes, candidates, paths, confirmation share, and application distributions.
- `pangloss fst-health` supports preflight-only and observed modes. Normal compilation emits the same
  findings through standard warning/error output.
- Recommendations are deterministic mappings from registered causes to applicable remedies. AI may
  consume the JSON but does not create canonical findings.
- Terminal resource findings prioritize actionable grammar changes and carry the effective envelope,
  reached counter, and partial measurements. They may describe an explicit larger-envelope retry,
  but neither the audit nor compiler retries or escalates limits automatically.
- Findings label exact values, proven lower bounds, and heuristic estimates distinctly. Only an
  exact value or conservative lower bound may prove that remaining work cannot fit and stop it
  before allocation; uncertain estimates remain advice while actual work is attempted and counted.
- Remedies never mutate the source grammar. The compiler may perform an internal optimization only
  when the owning lowering supplies a correctness argument preserving the complete analysis set;
  otherwise ordering, constraint, and decomposition ideas are reported for external review and a
  subsequent compile-and-compare cycle.
- The package builder consumes admission: Warning publishes; Error needs an explicit recorded
  override; Critical, incomplete, truncated, or watchdog-terminated work cannot publish.

## Ownership and verification

Owns the Rust audit evaluator, CLI/report rendering, finding integration, and artifact admission
adapter. Metric producer files remain owned by their profile/budget changes.

Run from `rust/`:

- `cargo test -p pg-foma fst_health_preflight`
- `cargo test -p pg-foma fst_health_observed`
- `cargo test -p pg-cli fst_health`
- `cargo test -p pg-wasm analysis_package_admission`
