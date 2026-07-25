**Status note:** two genuinely separate pieces of scope live in this one change. The budget/error
foundation (section 1-2, plus the ADR 0003 chain-depth extension called out in `STAGING.md`) is
landed in `pg-foma/src/compose_budget.rs`. The single-worker watchdog/process-routing subsystem
(section 3) is a wholly separate, much larger scope that has **not been started at all** — no
`sysinfo`, `Child::try_wait`, or spawned worker process exists anywhere in `pg-foma`/`pg-cli`; several
code comments explicitly note ADR 0003's in-process cooperative budgets were chosen instead (see
`analyzer.rs`, `compose_budget.rs`). Section 4's verification tasks are scoped to the watchdog and are
therefore also not done.

## 1. Budget foundation

- [x] 1.1 Validate budget configuration and expose effective versioned limits
      (`pg-foma/src/compose_budget.rs::ComposeBudget`, `chain_depth_cap_from_env`,
      `clamp_chain_depth_cap`)
- [ ] 1.1c Define versioned hard-coded high ceilings for every logical, byte, and wall-time limit;
      reject or contractually clamp excessive values and provide no unlimited setting
      (not separately verified — `compose_budget.rs` clamps chain-depth/ordering-multiplicity caps,
      but a byte/wall-time ceiling sweep across every limit was not confirmed)
- [ ] 1.1d Share one runtime budget schema and absolute ceiling set across Windows, Linux, and WASM;
      permit applications to select only lower effective values
      (not separately verified)
- [ ] 1.1a Make deterministic work counters the primary early-stop path and reserve wall time for the
      outer watchdog; return construct attribution for counter breaches
      (counters are the primary early-stop path today, but "reserve wall time for the outer watchdog"
      cannot be true — section 3's watchdog does not exist)
- [ ] 1.1b Add pre-allocation reservations for exact values and proven conservative lower bounds;
      prohibit rejection based only on heuristic estimates
      (not separately verified against this exact wording)
- [x] 1.2 Add cumulative build/apply tracking and reusable pre/post net-size checks
      (`compose_budget.rs::check_size`; `ComposeBudget::check_chain_depth`/
      `check_ordering_multiplicity`)
- [x] 1.3 Replace ambiguous timeout handling with typed spawn/panic/disconnect/timeout outcomes
      (`compose_budget.rs::ComposeError` typed variants; `ApplyOutcome<T>` for apply-path outcomes)

## 2. Guard all operations

- [x] 2.1 Guard raw lexc and regex compilation, apply initialization, first nets, and no-rule paths
      (`ComposeError` variants incl. `NetSizeExceeded`; guards wrap compose call sites)
- [x] 2.2 Unify templated-emitter line breaches with the typed budget result
      (`EmitLineBudgetExceeded`/`GroupBudgetExceeded` variants feed the same typed `ComposeError`)
- [x] 2.3 Add per-word input/path/output/candidate/time limits to application
      (`ApplyBudget` struct + `ApplyDimension`, consumed by `FomaProposer::propose_budgeted`)
- [x] 2.4 Return atomic per-word complete/incomplete outcomes, preserve completed batch members, and
      accept explicit caller retries of only the incomplete subset under selected apply limits
      (`ApplyOutcome<T>`; `pg-cli/src/diagnostics.rs` records `WordApplyStatus::Incomplete` per word)
- [ ] 2.5 Add optional cumulative caller-selected batch budgets and distinct complete, incomplete,
      and not-attempted per-word outcomes with resumable remaining subsets
      (not verified — no batch-level cumulative budget/resumable-subset mechanism found)
- [ ] 2.6 Expose explicitly named combined and HermitCrab-only runtime pipelines with the same budget
      and atomic-outcome contract; include the immutable selected pipeline in every result
      (not verified as a general contract; `add-grammar-diagnostics`'s own diagnose path is explicitly
      combined-only today, per that change's own status)

## 3. Single-worker watchdog and routing

- [ ] 3.1 Define one versioned, length-bounded worker request/result protocol shared by Windows and Linux (not done)
- [ ] 3.2 Implement a synchronous parent loop using `Child::try_wait`, wall deadline, and `Child::kill` (not done — no such loop exists)
- [ ] 3.3 Pin a Rust-1.90-compatible `sysinfo`; sample only the worker PID, record interval/observed
      peak, and kill on sampled-RSS breach without calling it a hard memory ceiling
      (not done — no `sysinfo` dependency or RSS sampling found)
- [ ] 3.4 Cap grammar/request bytes, stdout/stderr/result bytes, and diagnostic payloads (not done)
- [ ] 3.5 Route typed terminal outcomes directly without invoking any second engine or strategy (not done — no worker process to route from)
- [ ] 3.6 Record terminal reason and effective watchdog/logical-budget metadata in diagnostics (not done)
- [ ] 3.7 Support explicit new-request retries with a caller-selected larger named envelope; never
      escalate limits or retry automatically (not done)
- [ ] 3.8 Keep worker/compiler code out of WASM builds and expose no WASM compile entry point
      (not applicable yet — there is no worker; separately, `make-wasm-analysis-only` found WASM
      *still* builds `pg_foma::composite::FomaAnalyzer` directly, i.e. the compiler is NOT out of WASM)

## 4. Verification

- [ ] 4.1 Cover single-net, no-rule, cumulative-group, worker-failure, path explosion, and timeout cases
      (the non-worker cases are covered by `compose_budget.rs`'s own tests; worker-failure is not
      applicable, no worker exists)
- [ ] 4.2 Prove hung workers are killed after the wall deadline and sampled-RSS breaches terminate the
      worker without killing the parent; report sampling overshoot honestly (not done — no watchdog)
- [ ] 4.3 Prove request/output byte limits terminate flooding workers with typed outcomes (not done)
- [ ] 4.4 Prove every resource failure returns without automatically starting another strategy
      (true in spirit for the in-process budget path — every `ComposeError`/`ApplyOutcome::Incomplete`
      returns without an automatic second strategy — but not proven as a watchdog contract)
- [ ] 4.5 Prove the WASM dependency graph and exported API contain no FST compiler construction
      (not done — confirmed false: `pg-wasm` still depends on and constructs `pg_foma` compiler types)
- [ ] 4.6 Run the same watchdog contract suite on Windows and Linux; record a missing runner as
      `not_run` without hiding or failing unrelated verification
      (not_run — no watchdog contract suite exists yet to run on either platform)
- [ ] 4.7 Prove production compiler code launches no descendants; if that fact changes, open a new
      bounded containment proposal rather than expanding this worker implicitly
      (true today by absence of any spawning code, but not formally proven/tested)
