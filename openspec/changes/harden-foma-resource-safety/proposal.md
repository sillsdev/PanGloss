## Why

`ComposeBudget` checks returned nets between operations, but a single foma call can still hang or
allocate excessively; timeout threads are abandoned; raw compile/apply paths bypass checks; apply
traversal is uncapped; and budget failures are not uniformly terminal typed outcomes.

## What Changes

- Add validated cumulative build and apply budgets with uniform typed errors.
- Add a derivation/unapplication **chain-depth** budget dimension (`docs/adr/0003-apply-time-
  containment.md`), tracked alongside the existing states/arcs/tuples/paths/candidates counters, that
  deterministically closes the stack-overflow failure class (the deep 24-level derivation chain; the
  1 GiB-stack workaround) instead of relying on a larger stack.
- Check every newly compiled net, including single/no-rule paths and raw lexc/regex/apply-init operations.
- Run compilation in one native worker with parent-enforced wall timeout, sampled RSS guardrail,
  bounded input/output, and one typed protocol on Windows and Linux.
- Make terminal budget/watchdog failures return immediately without invoking another analysis or
  compilation strategy. A caller may explicitly make a new request.
- Keep compiler construction out of WASM; WASM performs bounded analysis only.
- Add adversarial logical-budget, timeout, sampled-RSS, and bounded-IPC tests.

## Impact

This changes failure behavior, not accepted parse semantics. Production compilation launches one
worker and no descendants. Logical budgets — now including chain depth — are the primary explosion
defense; the parent watchdog is the emergency backstop. The chain-depth dimension is what makes
stack-overflow deterministically boundable rather than merely deferred by a bigger stack. This change
does not build a general process sandbox.
