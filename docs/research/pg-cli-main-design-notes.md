# pg-cli main.rs: design notes moved out of comments

Longer arguments pulled out of `rust/crates/pg-cli/src/main.rs` implementation comments so the
source can carry a one-line pointer instead of the full argument. Each section corresponds to one
call site; the site names the function/type so this doc can be found from either direction.

## `GateResult`/`capability_gate`: the enforce/override contract

`capability_gate` decides what `run_batch`/`run_parse` should do about
`pg_foma::capability_entry::evaluate_capability`'s `CompileDecision` for a grammar, given the
resolved `enforce`/`allow_unproven` booleans, and what to print to stderr about it. The function
itself is engine-agnostic — it only implements the enforce/override boolean contract below. Which
engines actually pass `enforce == true` is a policy decision made by the caller: default-enforcing
on the `--engine=foma` path (the FST proposer is what a `Refuse` verdict is about), never enforced
on `--engine=default` (the HC-oracle path never builds or relies on the FST proposer, so there is
nothing for this gate to refuse on its behalf). `--no-enforce-capability` is the escape hatch out of
the foma-path default; `--allow-unproven` only matters with enforcement — passed alone it is
silently inert, never an error.

With `enforce == false` (advisory-only, and what every `--engine=default` invocation gets),
`Admit`/`ConfirmOnly`/`Refuse` are all reported as a preview only; a `Refuse` here never blocks.
With `enforce == true`: `Admit`/`ConfirmOnly` proceed (`ConfirmOnly` is its own first-class
non-failure verdict, so enforcement does not demand `Admit`, only rules out `Refuse`); `Refuse`
with `allow_unproven == false` makes the caller fail hard with no analysis output; `Refuse` with
`allow_unproven == true` force-compiles anyway, with `stderr_lines` carrying an unmissable
`trust=unproven` degraded-trust marker naming every overridden diagnostic.

That marker is a session/report-level notice, not a persistent stamp: `batch`/`parse` produce no
artifact of their own (a TSV file or a `word\tsignature` line, neither a pack), so there is nothing
for a manifest stamp to attach to at this call site. `pangloss pack` is the real, persistent home
for that stamp — it writes `capability_trust`/`CapabilityOverrideRecord` into an actual `.pgpack`
manifest.

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
be faithful): default-enforcing, i.e. `enforce_flag.unwrap_or(true)`; `--no-enforce-capability` is
the escape hatch back to advisory-only. On `Engine::Default` (the full-search HC-oracle path, always
faithful, never builds or relies on the FST proposer): never enforced, unconditionally — only the
FST/foma compile path is gated, and the oracle path is not this gate's concern at all. An explicit
`--enforce-capability` here is advisory-only and gets its own stderr note so it is never silently
swallowed; `--no-enforce-capability` is a silent no-op.
