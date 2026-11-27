# FST construction strategies, stage 0

The owner supplied and authorized the design in `fst-structural-design.md`, with the
2026-11-05 explicit strategy-axis override. Work inline in `pg-fst-strategies`;
no delegation, push or PR. One final squash commit.

1. Freeze the current scoreboard and routing evidence before changing compilation.
2. Add surface-enum/factored construction names and auto selection inside the single
   TunedSurfaceProbed backend. Keep factored unimplemented, with a typed refusal.
   One configuration carries the selected strategy, ordered auto candidates and
   finite construction bounds. Record every attempted strategy and its outcome.
3. Route analyzer compilation and completed-build compilation through the same
   compiler. Preserve HC confirmation and reject incomplete artifacts.
4. Expose configuration in grammar_pass, fst-health and grammar-health --fst.
   Add per-strategy differential records with occurrence IDs, input hashes, oracle
   identity, candidates, recall, confirmed identity multisets and incomplete rows.
5. Test explicit selection, auto ordering/stopping, refusal types, bounds, routing
   parity and the measurement arithmetic. Preserve the existing named scoreboard.
6. Run all conformance fixtures and the seven requested real grammars, serially
   under 8 GiB virtual address space and a 3600 second hang guard. Preserve all rows.
7. Personally review the complete diff; run managed check, the requested workspace
   suite, and preland under the shared Cargo lock. Commit with the requested trailer
   and write fst-strategies-report.md with measured results and remaining gaps.

Evidence classification: unimplemented construction is correctness/representability;
finite construction/apply bounds are resource containment; readiness cannot bypass
an attempt. No alpha lowering, partial-only semantics, new cache, or factored
implementation belongs in this lane. Existing alpha refusals remain baseline gaps.

Verification uses pg.ps1, with the owner-requested full workspace cargo test also
run under the lock. Environment: PANGLOSS_MACHINE_CHECKOUTS=/mnt/h/repos/machine,
POWERSHELL_TELEMETRY_OPTOUT=1, CARGO_BUILD_JOBS=4, RUST_TEST_THREADS=4.
