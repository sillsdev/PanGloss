# Open work queue

Everything called out as owed, with what "done" means for each. Ordered by what blocks what, not by
size. Delete an entry when it lands; this file is a queue, not a history.

## 1. Gate `strategy_coverage` against the measured matrix -- IN FLIGHT

The last authored claim nothing checks. The table asserts per `(CharacteristicKind,
EmissionStrategy)` what each compiler can propose; `examples/conf_matrix.rs` now measures what they
do. Sound direction: a table entry claiming a strategy CANNOT propose a kind is refuted by any
fixture exhibiting that kind which works on that strategy. The reverse is only suggestive, because a
fixture exhibits several kinds and attribution is not one-to-one.

**Done when:** a report names every agreed / contradicted / unsupported entry, the sound direction is
gated at its real count, and a can-fire test forces a synthetic contradiction and requires the gate
to name it.

## 2. `-Mode run` cannot have its stdout captured -- BLOCKS Aweti/Mbugwe

`Invoke-ProcessInJobObject` already takes `CaptureStdoutPath` and wires it to
`RedirectStandardOutput`, but only `corpus-test` passes it. `run` launches via `Start-Process
-NoNewWindow` with no redirection, so ordinary PowerShell `*>` at the outer invocation captures
nothing (console-handle inheritance). Two long censuses lost their output to this today.

**Done when:** `-Mode run` accepts an opt-in capture path that reaches the existing parameter, live
console output still works when it is not passed, and the Aweti/Mbugwe census can be run with its
output landing in a file.

## 3. Three permanently-red tests are eroding the suite's signal

`recipe_optimize_continuation`'s three tests carry a plain `#[test]`, no `#[ignore]`, no marker.
They fail on every run. Every "green modulo known failures" report today read past them, which is
exactly how a new failure hides behind an old one.

The cause is recorded: no fixture has the required cost profile (a non-confirmed candidate
mid-sequence AND a final candidate carrying cost). Repointing them at whichever fixture passes is
fitting the test to the tree and is explicitly rejected.

**Done when:** either a fixture with the right profile is authored and the tests pass, or the tests
are marked with a machine-readable reason naming what is missing. Not left red.

## 4. Two ratchets that measure the same thing and do not reconcile

`faithfulness_coverage_gate` holds recall at `NoMoreThan { failures: 19 }` -- 19 `(kind, backend)`
pairs. `conf_matrix` reports **12** silently-wrong cells over `(fixture, backend)`. Different
denominators, never compared, so neither can check the other.

**Done when:** the two are reconciled -- either one derives from the other, or the difference is
explained and recorded so a future reader is not left to guess which number is the truth.

## 5. `compose_budget` is dead at its layer -- DELETE, do not thread

Corrected conclusion (an earlier entry in `conformance-containment-inventory.md` said "thread it"
and named the wrong destination). `FomaAnalyzer` owns `peel_budget`, read from the environment;
`FomaProposer` -- what `new_with_budget*` builds -- has no budget field, because the proposer does
not peel. The parameter is unusable at that layer by construction.

Leaves `CompileWorkerRequest.chain_depth_cap` with no valid destination: either `FomaAnalyzer` gains
an explicit-budget constructor, or the field goes and the knob is documented as environment-only.

**Done when:** the parameter is gone from the three `new_with_budget*` signatures, the worker knob
is either routed or removed, and `unused_variables = "deny"` is on -- it costs exactly this one fix
and then kills the class.

## 6. The cheap mechanism subset

`#[must_use]` on verdict / decision / report types. Honest scope: it is defeated by `let _ =`, cannot
see a struct field dropped by callers (the `IdentityDivergence` case), and cannot see a registration
that is never consulted (the `Disposition::Proven` case). It catches one narrow shape and is nearly
free. Gates should also print their denominators in assertion messages.

**Done when:** applied, with no claim that it covers more than it does.

## 6b. Every published fact met into a selection seam needs a can-fire fixture

The one-way pin (a claimed refusal really refuses) is already the pattern. The missing half is a
synthetic fixture proving the fact fires at all. `.claude/skills/conformance-grammars/SKILL.md`
exists to author these.

**Done when:** each fact met at `backend_selection`'s per-strategy seam has one.

## 7. Merge `fix/env-repvariant`

Five commits of Aweti/Mbugwe census tooling, parked but preserved. Needs #2 first to be usable.

## 8. Re-run the matrix at current `main`

`conformance-backend-matrix.md` was measured at `496a6f3c`; `main` has since changed
`backend_runtime.rs`'s certify path and split `IdentityMismatch`. The semantics probably did not
move, but the table has not been re-run since the instrument under it changed, and it is one
`-Mode run` to settle.

**Done when:** re-run, and the doc's measured-at line matches a current commit.

## Future, not queued

- **`PlanComposed`'s marker-subtree gap.** All 36 of its refusals are one shape: a plan requiring a
  `CompositeEmissionMarker` / `StructuralCompositeMarker` subtree `build_controllable` cannot build.
  One gap, over half the unbuildable cells in the matrix. The cheapest route to broader coverage
  whenever coverage becomes the goal.
- **The 12 silently-wrong cells**, starting with `morphotactic-attribute-breadth` -- the only fixture
  where all three backends miss analyses, so there is no correct path to it at all.
- **Aweti/Mbugwe sizing**, parked by instruction until conformance is settled.
