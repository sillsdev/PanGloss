# pg-foma recipe_optimizer.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-foma/src/recipe_optimizer.rs` implementation
comments so the source can carry a one-line pointer instead of the full argument. Each section
corresponds to one call site; the site names the function/constant so this doc can be found from
either direction.

## The `evaluated_count == selected_count && !budget.admits(usage)` arm: termination only, never quality

Every selected candidate was evaluated, but the *measured* cost of the last one breached a budget
dimension — only `evaluations` is pre-checked; `elapsed`/`build`/`memory`/`confirmation` are known
only after the evaluator returns. Without this arm, a run whose final candidate blew the deadline
reported `Complete`, claiming to have stayed inside a bound it had already exceeded. The
candidate-count deficit branch above this one cannot catch it, because the overrun happens on the
last *selected* candidate, so no candidate is left unevaluated.

`Termination` is set here, `SearchQuality` is deliberately left alone, and that split is
load-bearing rather than stylistic: `SearchQuality` answers "did the search look at everything it
selected?" and `Termination` answers "why did it stop?". In this arm the first answer is yes — the
deficit branch owns the case where it is no — so the two answers genuinely differ, and only the
second one changes.

Downgrading `quality` here as well produced a report that could not be written at all.
`RecipeOptimizationReport::validate` refuses `Approximate` with `unexplored == 0` ("approximate
search must quantify unexplored space"), and `unexplored` is zero by construction on this path —
every selected candidate was evaluated. So the child process exited 1 with no `report.json`, and
`write_supervisor_failure_report` never ran either (it fires only on a deadline/memory kill, not on
a non-zero exit), which meant an entire run's banked candidates were reachable only through
`progress.jsonl`.

Reproduced end to end on the `recipe-strata-generic` fixture with `--confirmation-work` set one
unit below the corpus's total confirmation work; pinned by
`pg-cli/tests/recipe_optimize_continuation.rs::a_final_candidate_that_overruns_an_aggregate_bound_still_writes_a_report`.
