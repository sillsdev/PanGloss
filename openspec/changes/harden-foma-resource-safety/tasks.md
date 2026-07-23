## 1. Budget foundation

- [ ] 1.1 Validate budget configuration and expose effective versioned limits
- [ ] 1.1c Define versioned hard-coded high ceilings for every logical, byte, and wall-time limit;
      reject or contractually clamp excessive values and provide no unlimited setting
- [ ] 1.1d Share one runtime budget schema and absolute ceiling set across Windows, Linux, and WASM;
      permit applications to select only lower effective values
- [ ] 1.1a Make deterministic work counters the primary early-stop path and reserve wall time for the
      outer watchdog; return construct attribution for counter breaches
- [ ] 1.1b Add pre-allocation reservations for exact values and proven conservative lower bounds;
      prohibit rejection based only on heuristic estimates
- [ ] 1.2 Add cumulative build/apply tracking and reusable pre/post net-size checks
- [ ] 1.3 Replace ambiguous timeout handling with typed spawn/panic/disconnect/timeout outcomes

## 2. Guard all operations

- [ ] 2.1 Guard raw lexc and regex compilation, apply initialization, first nets, and no-rule paths
- [ ] 2.2 Unify templated-emitter line breaches with the typed budget result
- [ ] 2.3 Add per-word input/path/output/candidate/time limits to application
- [ ] 2.4 Return atomic per-word complete/incomplete outcomes, preserve completed batch members, and
      accept explicit caller retries of only the incomplete subset under selected apply limits
- [ ] 2.5 Add optional cumulative caller-selected batch budgets and distinct complete, incomplete,
      and not-attempted per-word outcomes with resumable remaining subsets
- [ ] 2.6 Expose explicitly named combined and HermitCrab-only runtime pipelines with the same budget
      and atomic-outcome contract; include the immutable selected pipeline in every result

## 3. Single-worker watchdog and routing

- [ ] 3.1 Define one versioned, length-bounded worker request/result protocol shared by Windows and Linux
- [ ] 3.2 Implement a synchronous parent loop using `Child::try_wait`, wall deadline, and `Child::kill`
- [ ] 3.3 Pin a Rust-1.90-compatible `sysinfo`; sample only the worker PID, record interval/observed
      peak, and kill on sampled-RSS breach without calling it a hard memory ceiling
- [ ] 3.4 Cap grammar/request bytes, stdout/stderr/result bytes, and diagnostic payloads
- [ ] 3.5 Route typed terminal outcomes directly without invoking any second engine or strategy
- [ ] 3.6 Record terminal reason and effective watchdog/logical-budget metadata in diagnostics
- [ ] 3.7 Support explicit new-request retries with a caller-selected larger named envelope; never
      escalate limits or retry automatically
- [ ] 3.8 Keep worker/compiler code out of WASM builds and expose no WASM compile entry point

## 4. Verification

- [ ] 4.1 Cover single-net, no-rule, cumulative-group, worker-failure, path explosion, and timeout cases
- [ ] 4.2 Prove hung workers are killed after the wall deadline and sampled-RSS breaches terminate the
      worker without killing the parent; report sampling overshoot honestly
- [ ] 4.3 Prove request/output byte limits terminate flooding workers with typed outcomes
- [ ] 4.4 Prove every resource failure returns without automatically starting another strategy
- [ ] 4.5 Prove the WASM dependency graph and exported API contain no FST compiler construction
- [ ] 4.6 Run the same watchdog contract suite on Windows and Linux; record a missing runner as
      `not_run` without hiding or failing unrelated verification
- [ ] 4.7 Prove production compiler code launches no descendants; if that fact changes, open a new
      bounded containment proposal rather than expanding this worker implicitly
