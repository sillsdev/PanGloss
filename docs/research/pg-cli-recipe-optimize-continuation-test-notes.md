# pg-cli recipe_optimize_continuation.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-cli/tests/recipe_optimize_continuation.rs`
implementation comments so the source can carry a one-line pointer instead of the full argument.

## Why this file exists: the run must survive, not just produce a well-shaped verdict

A per-candidate proposal budget (`--candidate-proposal-work`, since reverted) shipped with 763
passing tests including a purpose-built gate. Every one of them checked the verdict's *shape* —
that it was typed, that it carried its budget, that it could not be relabelled. Not one checked
that the run *survived* producing it. The reported symptom was the opposite of the feature's
purpose: optimizer runs banked fewer candidates with the bound in force than without it, and a
verdict that exists only in a final report that is never written is a silent absence in exactly
the case the banking machinery exists for.

So the durable property this file pins is not "the bound works", it is:

> A candidate that ends in a non-selectable verdict must (a) appear in `progress.jsonl` as itself,
> and (b) leave every other candidate evaluable.

That is a property of the optimizer loop, the evaluator, the progress writer, the supervisor and
the report validator *together*, which is why these tests drive the real `pangloss` binary rather
than `optimize_with_evaluator` directly. Two of the three defects they pin are invisible from
inside `pg-foma`: one is a report the validator refuses to accept, the other is a run whose
artifacts never reach disk.

## Why these bounds and not a per-candidate one

There is no per-candidate resource bound reachable from the CLI today — that is precisely what the
reverted budget was. `--confirmation-work` is the closest thing: the same number is handed to
`RuntimeBudget::confirmation` (a per-candidate post-hoc ceiling yielding
`Certification::ResourceBreach`) *and* compared against the run's running total by `Budget::admits`.
One knob doing double duty means "abandon this candidate" and "end the run" are the same event by
arithmetic: making candidate k breach requires `allowance - prefix[k] < conf[k]`, and continuing
past it requires `prefix[k] + conf[k] <= allowance`, which cannot both hold. A future per-candidate
bound has to break that tie, and when it does, the property above is what it owes.

Every bound in this file is computed from an unbounded run of the same fixture in the same test,
never hardcoded. `Score`'s confirmation counts are exactly reproducible (see `Score::key`'s doc),
so a derived bound is as deterministic as a literal one — and it survives a fixture edit that a
literal would silently turn vacuous.

## `FIXTURE`: the measured, deterministic evaluation order

`conformance-staging/edge-cases/recipe-strata-generic/grammar.xml` was chosen because one unbounded
run of it produces the mix every test in this file needs: several confirmed candidates, one genuine
`identity-mismatch` in the middle of the sequence, and one candidate whose confirmation work is far
above the rest (so a derived bound can single it out). Measured order:

| # | realized strategy | confirmation | verdict |
|---|---|---|---|
| 0-2 | `plan-composed` | 13 each | `full-hc-confirmed` |
| 3 | `templated-underlying-tokens` | 11 | `identity-mismatch` |
| 4 | `tuned-surface-probed` | 30 | `full-hc-confirmed` |
| 5 | `plan-composed` | 13 | `full-hc-confirmed` |

Nothing in the test file hardcodes these numbers; they are recorded here so a reader knows what the
fixture is for.
