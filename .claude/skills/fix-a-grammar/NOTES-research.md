# Research notes for designing the `fix-a-grammar` skill's process

Not the skill. This captures what the technique-catalogue research turned up that bears on
*process* design — trigger detection, what existing tooling already does for you vs. what needs a
human call, and a few structural observations worth folding into the skill when it's designed. The
catalogue and scoring metric themselves are in
`docs/fst-plan/grammar-optimization-techniques.md`.

## The five trigger conditions: automatic detection vs. human judgment

| # | Trigger | Automatically detectable today? | Evidence |
|---|---|---|---|
| (a) | Takes a long time to build | **Numbers exist, no auto-threshold.** `pangloss make-report` measures build time in-process; `Metric::ElapsedMillis` findings exist in the health schema. But `health_evaluator.rs`/`fst_health.rs` always emit `ElapsedMillis`-adjacent findings at flat `Severity::Info`, or only as a `Critical` *trip* if an opt-in wall-clock `ComposeError::ComposeStepTimedOut` deadline (off by default, `HC_COMPOSE_STEP_TIMEOUT_MS`) is both configured and exceeded. There is no `severity_for_build_millis` counterpart to `severity_for_size_bytes`. A human must compare the number against a named target (e.g. the ~2 s soft load budget in `docs/fst-plan/foma-fst-plan.md`'s P3 targets, or the sub-10 ms/word target in `[[build-for-full-scale-grammars]]`) — the tooling will hand you the number but not the verdict. |
| (b) | Produces a large artifact | **Automatic.** `health.rs::severity_for_size_bytes` bands payload bytes into five severities (Ideal/Info/Warning/Error/Critical) per R6's exact decimal-byte thresholds. This is the *only* one of the five triggers with a real, pre-built, magnitude-aware severity function. State/arc counts (`PlanMeasure`, `selection.rs`) are measured but not banded the same way — only bytes are. |
| (c) | Over-relies on confirm to prune (proposer looseness) | **Numbers exist, no auto-threshold, and the deep diagnosis is a manual workflow.** `pangloss fst-health` reports `ProposalCandidateCount`/`ConfirmationCount`/`RejectionShare`/`DuplicateAnalysisRatio`, but every one of these is hardcoded to `Severity::Info` in `fst_health.rs` regardless of magnitude ("a high rejection share is expected overapproximation evidence, not itself a correctness problem" — true, but it also means nothing ever escalates this automatically). The dead-end census's own d1–d6 attribution *does* have a real go/no-go bar (≥20% of failing-candidate time AND ≥15% end-to-end win), but that bar is applied by a human running `cargo run --example deadend_census` and reading the printed table — it is not wired into `pangloss fst-health`'s automatic output at all. These are two separate tools measuring related things at two different depths, and neither one currently escalates severity by magnitude. |
| (d) | Per-candidate cost over ~10 ms | **Numbers exist, no auto-threshold.** `rust/tools/typology-speedup.sh` and `make-report`'s latency section produce real p50/p90/p99 figures with a real timer-floor discipline (never a fabricated bare `0`). No severity band exists for latency the way one exists for size. The ~10 ms target comes from a memory record (`[[build-for-full-scale-grammars]]`), not from anything the CLI itself asserts against. |
| (e) | Cannot faithfully represent some grammar feature | **Fully automatic, and the only trigger with a hard, build-relevant signal.** `pg_foma::backend_selection::select_backends` returns a `CompileDecision::Refuse` report for a backend that cannot represent the grammar, and `preflight.rs::semantic_uncertainty_finding` reports the separate whole-grammar-join reading of the same fact as `Severity::Critical`. `pangloss pack`'s own capability gate hard-fails on the selector's per-backend verdict without `--allow-unproven`. This is the one trigger where "should I reach for this skill" requires zero judgment — the compiler already refuses to proceed. |

**Net observation for skill design:** only (e) is a hard stop today; (b) has a graduated automatic
severity; (a), (c), (d) all have real, already-built measurement but land at a flat `Info` severity
regardless of how bad the number is. If the skill wants an automatic "you should consider
`fix-a-grammar`" nudge for (a)/(c)/(d), it either needs to (i) name explicit numeric thresholds
itself (the skill's own job, not this research task's), or (ii) point at the informal targets
already scattered across memory records and plan docs (`~2s` soft compile budget, `sub-10ms/word`,
the dead-end census's 20%/15% go-bar) and ask the user to apply them by hand, exactly as happens
today. Building `severity_for_build_millis`/an auto-escalated `RejectionShare` band would be a small,
concrete, separately-scoped follow-on if the skill's designers want trigger detection to be more
automatic than judgment-call — flagged here, not done, per this task's read-only scope.

## Other process-relevant observations

- **The scoring metric's own Step 0 (capability + recall) is cheap and already fully tool-backed**
  (`characterize`/`backend_selection::select_backends`, plus a differential-oracle-style parity
  pattern). The skill can require this gate unconditionally with no new tooling — it is pure
  composition of what exists.

- **Generality and regression-risk are the two dimensions with no single existing script**, but both
  decompose entirely into loops over tools that already exist (`characterize`/dead-end census run
  across the whole corpus instead of one grammar; the existing conformance/interaction-coverage gates
  run as-is). This is a natural, scoped, standalone follow-on task if the skill's authors want a real
  `pangloss corpus-generality-report`-style command rather than "run the existing tool N times by
  hand" — worth flagging to whoever designs the skill's own tooling requirements, since a metric that
  requires a human to manually loop a script over ~20 fixtures each time is exactly the kind of thing
  that erodes under time pressure.

- **The interaction-coverage check (`plan_interaction_coverage.rs`) only sees candidates expressed as
  a real `crate::plan::Plan` value.** If the skill's process lets an engineer prototype a candidate as
  an ad hoc script or a hand-rolled lexc/foma invocation outside the reified-plan machinery (very
  plausible for a fast first spike, and arguably *encouraged* by "consider ≥3 models" if the fastest
  way to sketch a third model is a throwaway script), that candidate gets **zero** automatic
  interaction-coverage or plan-DAG-level correctness benefit. The skill should probably say
  explicitly: a candidate that will realistically be adopted should be expressed through `plan.rs`
  before it's scored seriously, not just before it ships — otherwise the scoring exercise quietly
  privileges candidates that happen to already live in the reified-plan world.

- **Two real repo incidents anchor the regression-risk gate, and both are worth the skill quoting
  verbatim rather than paraphrasing**, because they are the difference between "run the tests" as a
  platitude and "run the tests, and also ask this specific extra question": the shared-`constructs.txt`
  -id inheritance defect (`docs/conformance/shared-construct-id-analysis.md`) and the eleven-site
  `TableId(0)` defect invisible to the whole suite until a test was written *specifically* to
  discriminate the table-dependent path
  (`conformance-staging/edge-cases/segment-natural-class-table-binding/STAGING.md`). Both are cases
  where "all existing tests pass" was true and the change was still wrong. The skill's regression-risk
  step should ask "does this reach a combination nothing existing actually discriminates?" as a named
  question, not assume a green `cargo test` settles it.

- **The `fst-precision-knob-spec` teardown is the sharpest cautionary tale available for why the
  metric uses Pareto dominance instead of a weighted score**, and is concrete enough to quote directly
  in the skill if it wants a memorable anchor: a runtime knob that became "an auction" of tradeoffs
  moved measured precision by 0.0002 (0.0504→0.0506) at 8.4× compile cost, and was torn down
  specifically because a tunable weighted tradeoff surface with no stable per-grammar meaning is
  worse than no knob at all.

- **`ComposeStrategy::Lazy`/`LazyLookahead` are a live example of "designed for, not built"** already
  sitting in the type system (`plan.rs`) with a loud panic if ever selected (`build.rs`). This is a
  good candidate for the skill's own worked example/tutorial fixture, if it wants one: a real,
  in-repo, half-finished multi-strategy design that a `fix-a-grammar` exercise could plausibly
  complete for a grammar where build time (trigger a) is the presenting problem.

- **Dead-end-census and this skill will overlap and should say so explicitly.** Dead-end-census is
  scoped to "why is confirm slow, which proposer-precision encoding fixes it" — a specific instance of
  triggers (c)/(d). `fix-a-grammar` is broader (all five triggers, and explicitly requires
  considering grammar-structure reallocation, which dead-end-census's own encodings (E1–E5) mostly
  are NOT — they tighten the proposer's *emission* of an existing structure, they don't reallocate
  strata/tables/gates). The skill should probably say: for a (c)/(d)-shaped problem, run
  dead-end-census FIRST (it is cheap, standing, and already answers "which encoding, if any"); treat
  its output as one input to `fix-a-grammar`'s own broader 3-candidate comparison rather than
  duplicating its workflow.
