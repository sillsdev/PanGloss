# pg-cli main.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-cli/src/main.rs` implementation comments so the
source can carry a one-line pointer instead of the full argument. Each section corresponds to one
call site; the site names the function/type so this doc can be found from either direction.

> **Current product policy (2026-08-23).** The implementation details below describe the legacy
> capability-gate behavior, not the production flag surface. `--allow-unproven` is a
> developer-build-only capability override: it may lose valid parses and its output is never
> eligible for certification or production publication. `--no-enforce-capability` is a legacy
> developer-only escape and must be absent/rejected in production. The separate
> `--remove-size-limits` developer stress control removes internal deterministic size/work caps
> only; exact completion and external watchdog/RSS containment, bounded I/O, and the absolute
> ceiling remain mandatory. `Error` may be complete/accurate stress evidence but is
> production-unready; `Critical` is a correctness gap.

## `GateResult`/`capability_gate`: the enforce/override contract

`capability_gate` decides what `run_batch`/`run_parse` should do about the `CompileDecision` in
`GATED_BACKEND`'s own compatibility report — read via `gated_backend_decision` over
`pg_foma::backend_selection::select_backends_for_grammar`, fail-closed to `Refuse` if the selector
never reported on that backend — for a grammar, given the resolved `enforce`/`allow_unproven`
booleans, and what to print to stderr about it. The function
itself is engine-agnostic — it only implements the enforce/override boolean contract below. Which
engines actually pass `enforce == true` is a policy decision made by the caller: default-enforcing
on the `--engine=foma` path (the FST proposer is what a `Refuse` verdict is about), never enforced
on `--engine=default` (the HC-oracle path never builds or relies on the FST proposer, so there is
nothing for this gate to refuse on its behalf). In the legacy developer CLI,
`--no-enforce-capability` escaped the foma-path default and `--allow-unproven` only mattered with
enforcement; neither is a production capability, and production must reject both.

With `enforce == false` (advisory-only, and what every `--engine=default` invocation gets),
`Admit`/`ConfirmOnly`/`Refuse` are all reported as a preview only; a `Refuse` here never blocks.
With `enforce == true`: `Admit`/`ConfirmOnly` proceed (`ConfirmOnly` is its own first-class
non-failure verdict, so enforcement does not demand `Admit`, only rules out `Refuse`); `Refuse`
with `allow_unproven == false` makes the caller fail hard with no analysis output; `Refuse` with
`allow_unproven == true` force-compiles anyway in the developer/test build, with `stderr_lines`
carrying an unmissable `trust=unproven` degraded-trust marker naming every overridden diagnostic.
Because the override may omit valid parses, that output is never a certification or
production-publication result.

Every branch writes to stderr, never stdout, matching `print_grammar_warnings`'s convention: the
`batch`/`parse` protocol output (TSV rows to a file; the `word\tsignature` line) is never among
`GateResult`'s lines, so a conformance runner reading only that protocol output cannot be perturbed
by anything here. The whole gate is pure (no I/O of its own) so it is directly unit-testable without
capturing process stderr; callers print `stderr_lines` themselves.

`GateResult::overridden` is exposed as a plain bool, not only baked into `stderr_lines`' text, so a
test or any future non-stderr consumer can key off the degraded-trust fact directly rather than
string-matching stderr; `#[allow(dead_code)]` reflects that it is genuine, documented API surface
with no non-test reader yet, not an oversight.

## `resolve_capability_enforcement`: the hard scoping rule

Resolves the effective `enforce` boolean `capability_gate` takes, from the parsed `--engine` and the
user's explicit `--enforce-capability`/`--no-enforce-capability` choice. On `Engine::Foma` (the path
that actually builds the FST proposer, where a `Refuse` verdict means the shippable proposer cannot
be faithful): default-enforcing, i.e. `enforce_flag.unwrap_or(true)` in the legacy developer CLI;
production has no bypass. On `Engine::Default` (the full-search HC-oracle path, always
faithful, never builds or relies on the FST proposer): never enforced, unconditionally — only the
FST/foma compile path is gated, and the oracle path is not this gate's concern at all. An explicit
`--enforce-capability` here is advisory-only and gets its own stderr note so it is never silently
swallowed; `--no-enforce-capability` is a silent no-op.
