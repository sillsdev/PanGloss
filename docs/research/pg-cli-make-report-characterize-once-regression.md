# Regression: one `make-report` invocation must characterize the grammar exactly once

`one_make_report_invocation_characterizes_the_grammar_exactly_once`
(`pg-cli/src/make_report.rs`) pins that one `make-report` invocation resolves the capability
verdict from one `pg_foma::capability::characterize` walk, not one per consumer. Before the fix
this measures, the count was 5 on the refused-grammar path: the preamble, `readiness_verdict::
certify`, and three more inside a single `plan_diagram::build_plan_document`
(`plan_and_profile` twice plus `compose_envelope`), each rebuilding the whole profile — including
real `Simultaneous`-mode `foma::types::Fsm` construction — instead of sharing one
`GrammarSemantics` owner.

The fixture is the refused grammar with no `--allow-unproven`, deliberately: that takes the
`!attempt_compile` branch, so no pack is built and no foma compile runs, and the count measured is
exactly the derivations the shared owner is responsible for. `pack::build_pack`'s trust stamp is
reachable only on the compile path and is fixed by the same shared owner, but is deliberately
excluded from this count: including it would drag in `emit.rs`'s own separate
`compound_chain_depth_and_budget_check` characterize call, which is not one of the duplicated
verdict derivations, and the assertion could not then attribute the total cleanly.

The counter is thread-local (`pg_foma::capability::characterize_call_count`), so the reading is this
test's own thread and cannot be polluted by tests running in parallel. The non-zero assertion is
not redundant: a thread-local count could otherwise "pass" by measuring nothing at all if the work
moved off-thread.
