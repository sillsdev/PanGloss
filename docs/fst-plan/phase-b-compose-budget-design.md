# Phase B design: composition-path budget guards (ComposeBudget)

Status: DESIGN 2026-07-20, implementation dispatched. Companion to
`synthetic-stress-grammar-plan.md` (Phase B, vectors V1-V6) and Fix 1's `EnumerationBudget`
(`morphotactics.rs`). Produced by a read-only design investigation against main@9bfea25 plus a
review addendum (§8) covering `emit_underlying_templated` (merged at dfb5025, after the
investigation snapshot).

## 0. Scope correction discovered during investigation

`EnumerationBudget` (`morphotactics.rs:224-323`) guards only the eager-enumeration path
(`preexpand.rs`/`emit.rs`). The P6 composition path (`replace.rs`, `gate.rs`, `uflexc.rs`)
never imports it — zero references. Moreover the P6 path has **no `Result`-returning public
API today**: everything returns bare `Option`/`Fsm`/report structs and the example drivers
`panic!` on failure. Phase B therefore first introduces `Result` into those signatures, then
wires the budget through.

## 1. Vendored-crate findings (foma = "=0.4.0", crates.io — cannot be patched in-tree)

- `Fsm` exposes `pub statecount: i32` / `pub arccount: i32` (`types.rs:223-224`) — size checks
  are free after each call. Precedent: `analyzer.rs:164` already reads `arccount`.
- **No mid-operation hook exists anywhere** — `fsm_compose` (`constructions/products.rs:167`),
  `fsm_union` (`boolean.rs:131`), `fsm_minimize` (`minimize.rs:154`), `fsm_determinize`
  (`determinize.rs:197`) are synchronous tight loops with no callback/cancellation. Confirmed
  by reading the function bodies, not inferred.
- **`fsm_compose` internally minimizes BOTH operands** (`products.rs:216-217`) — every compose
  step already pays a determinize (worst-case exponential). V2's real risk hides inside every
  V1 call site, not just at an explicit final minimize.
- `fsm_union` does NOT minimize (cheap per step) — gate.rs's per-group union fold accumulates
  a non-minimal net whose eventual minimize is the true worst-case moment.
- Large-stack worker threads around this exact call shape already exist
  (`p6_replace_prototype.rs:72-79` 256MB, `p6_aweti_probe.rs:38-43` 512MB) — reusable for the
  wall-clock wrapper. `Fsm` is plausibly auto-`Send` (no Rc/RefCell seen); implementation must
  add a compile-time `assert_send` check.
- `catch_unwind` (used in `p6_aweti_probe.rs`) does NOT catch stack-overflow or OOM aborts.

## 2. `ComposeBudget` (new module `rust/crates/pg-foma/src/compose_budget.rs`)

Fields (env-overridable, HC_* naming like Fix 1): `state_cap` (HC_COMPOSE_STATE_BUDGET),
`arc_cap` (HC_COMPOSE_ARC_BUDGET), `tuple_cap` (HC_COMPOSE_TUPLE_BUDGET, default 5_000 —
Amharic's real worst case is ≤354), `group_cap` (HC_COMPOSE_GROUP_BUDGET, default 64 —
Indonesian needs 2), `line_cap` (HC_COMPOSE_LINE_BUDGET, see §8), `step_timeout`
(HC_COMPOSE_STEP_TIMEOUT_MS, `Option<Duration>`, **default OFF** — mirrors StepBudget's
opt-in convention; the four size caps are **default ON** like Fix 1).

Deliberate departure from `EnumerationBudget`: no AtomicUsize latch — the compose cascade and
gate.rs's per-group loop are strictly sequential (no rayon); a plain `&ComposeBudget` suffices.
Revisit if the group loop is ever parallelized.

Constructors: `from_env()`, `with_caps(..)`, `#[cfg(test)] unbounded()`.

State/arc defaults: see §8 (calibration now possible — the investigation flagged them TBD).

## 3. Error type and checked wrappers

```rust
pub enum NetSizeMeasure { States, Arcs }
pub enum ComposeError {
    NetSizeExceeded { measure: NetSizeMeasure, value: i32, limit: usize, site: &'static str }, // V1
    AlphaTupleBudgetExceeded { surviving: usize, limit: usize, rule_xml_id: String },          // V3
    GroupBudgetExceeded { groups: usize, limit: usize, gated_subrules: usize },                // V6
    EmitLineBudgetExceeded { lines: usize, limit: usize },                                     // V4
    ComposeStepTimedOut { elapsed: Duration, limit: Duration, site: &'static str },            // V2
}
```

Shared wrappers replacing every direct foma call on the P6 path:
`compose_checked` / `union_checked` / `minimize_checked(opts, .., budget, site) -> Result<Fsm, ComposeError>`
— each optionally runs under the §5 deadline wrapper, then checks statecount/arccount.

Signature changes (Result-ification):
- `compile_rewrite_rule_subset` (replace.rs:476) → `Result<Option<(Fsm, Vec<TupleReport>)>, ComposeError>`
  (`Ok(None)` keeps meaning "unsupported construct, skip"; `Err` = breach, not skippable)
- `compile_rewrite_rule` (replace.rs:454), `compile_and_compose_rules` (replace.rs:561),
  `compile_and_compose_rules_gated` (replace.rs:607) → same pattern
- `compile_gated_grammar` (gate.rs:254) → `Result<GatedCompileResult, ComposeError>`
- `emit_underlying_filtered` (uflexc.rs:111) → `Result<UEmitReport, ComposeError>`

Forward-compat: when P6 wires into `FomaProposer`/`FomaAnalyzer`, `FomaError` gains
`ComposeBudgetExceeded(ComposeError)` beside `EnumerationBudgetExceeded` (analyzer.rs:29-52).

## 4. Exact insertion sites

**V1 (size checks via checked wrappers):**
- replace.rs:544 — per-alpha-tuple fold in `compile_rewrite_rule_subset` (highest frequency;
  where V3 and V1 compound), site `"compile_rewrite_rule_subset alpha-tuple fold"`
- replace.rs:589 — `compile_and_compose_rules` cascade fold
- replace.rs:632 — `compile_and_compose_rules_gated` cascade fold
- gate.rs:316 — `lexc .o. rules` per group
- gate.rs:322 — per-group union fold (`union_checked`)

**V2:** covered per-step automatically via the wrappers (compose minimizes internally). Also:
`compile_and_compose_rules`/`compile_gated_grammar` take ownership of their own FINAL
`minimize_checked` instead of leaving it to example drivers — turns a convention into an
enforced invariant.

**V3:** replace.rs:506-513, immediately after `resolve_alpha_tuples` returns and BEFORE the
`for asg in &assignments` loop — check `assignments.len() > tuple_cap`. Cheapest earliest
predictor, same principle as Fix 1's "check the search result before the expensive part".

**V4:** uflexc.rs line-push sites (141 root, 174 prefix, 186 suffix) — incremental
`line_count` check so a pathological grammar bails during the FIRST group's emission.
(Confirmed: EnumerationBudget never applied here.) Plus emit.rs — see §8.

**V6:** gate.rs:260-261, after `partition_entries` and BEFORE the per-group loop —
`groups.len() > group_cap`. Single highest-leverage check (gates all downstream V1/V4 work).
**No graceful fallback by design**: merging/dropping groups is unsound (over/under-firing
gated rules); the only correct response is the typed error → fallback engine for that grammar.

## 5. Wall-clock wrapper (V2, default OFF)

```rust
fn call_with_deadline<F: FnOnce() -> Fsm + Send + 'static>(f: F, timeout: Duration)
    -> Result<Fsm, Duration>
// spawn 256MB-stack worker + mpsc channel; recv_timeout; Err = thread ABANDONED, not killed
```

Feasibility verified: Fsm plausibly Send; the large-stack-worker pattern is proven in this
repo; `StepBudget` (pg-rules/stratum.rs:187-263) is the conceptual precedent (two independent
bounds, wall-clock no-op when unset) but its cooperative polling cannot bound an opaque C-port
call, so it is not reusable code here.

## 6. Test plan (explicit-caps constructors, never env vars; no #[ignore]; no gitignored fixtures)

- replace.rs `compose_budget_tests`: `alpha_tuple_budget_trips_on_synthetic_rule`
  (known survivor count by construction, cap below it); `state_budget_trips_on_tiny_cascade`
  (hand-written xre nets, cap=2); `unbounded_budget_never_trips_on_small_fixture`.
- gate.rs `group_budget_tests`: `group_budget_trips_before_any_group_work_runs` (k=4 gated
  subrules, all 16 combos present → exactly 16 groups; cap=8; assert Err AND elapsed <200ms —
  proves fail-fast); `zero_gated_subrules_collapses_to_one_group_still_passes_strict_cap`.
- uflexc.rs `emit_budget_tests`: `line_budget_trips_incrementally` (20 entries, cap=5, assert
  `lines: 6` — proves first-crossing detection).
- compose_budget.rs deadline tests: slow-closure trips fast, fast-closure passes (sleep
  stand-ins, mirroring StepBudget's own tests).

## 7. Limitations (verbatim from the investigation — the honest part)

- A between-step size check cannot catch a blowup INSIDE one call: if a single
  compose/minimize OOMs or spins, the check after it never runs. There is nothing in the
  vendored crate to checkpoint; the size caps only bound cost accumulating ACROSS calls.
- The wall-clock wrapper detects, it does not stop: the worker thread is abandoned and keeps
  running/allocating until it finishes naturally. Treat `ComposeStepTimedOut` as TERMINAL for
  that grammar (fallback engine), never retry the identical call; a long-lived server
  embedding this must track abandoned-thread count.
- `catch_unwind` is not a safety net: stack-overflow and allocator-OOM abort the process,
  bypassing every check here. Large stacks reduce but don't eliminate overflow risk.
- Full "never blow up" for a single adversarial call needs an external supervisor process —
  out of Phase B scope, noted for the plan's Phase D.

## 8. Review addendum (2026-07-20, post-dfb5025)

The investigation ran against main@9bfea25; `emit_underlying_templated` (emit.rs, merged at
dfb5025) postdates its snapshot. Extensions required in implementation:

1. **V4 for `emit_underlying_templated`**: the same incremental `line_cap` check at its
   lexc-writing sites (`write_tag_entry`/`write_bare` accumulation, or a line counter in
   `EmitCounts`), returning the same typed breach through `EmitResult`'s existing
   `tier: Unsupported` + a new `compose_budget_exceeded`-style report field OR by
   Result-ifying it consistently with §3. Prefer whichever keeps `emit_with_budget`'s
   SurfaceProbed behavior byte-identical (leaf-site rule from dfb5025 applies).
2. **Calibration is now possible**: Aweti end-to-end on the new path measures
   lexc 23,661 states / 346,727 arcs → composed+minimized 35,846 states / 800,354 arcs, <3s.
   Set `DEFAULT_STATE_BUDGET`/`DEFAULT_ARC_BUDGET` with generous headroom above this real
   grammar (suggest 2,000,000 states / 20,000,000 arcs as first calibration — ~50x/25x the
   largest real net today, still far below the enumeration path's 8.8GB disaster), and record
   the calibration basis in the constants' doc comments. Refine in plan Phase D sweeps.
3. The p6 gate/examples (tests) keep calling raw foma functions — acceptable (dev tools), but
   the library-internal final minimize (§4 V2) should still land so future callers inherit it.
