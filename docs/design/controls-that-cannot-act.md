# Controls that cannot act: four incidents

Extracted from CLAUDE.md. The rule these produced is one line and stays there; these are the
write-ups. Every one was found by accident rather than by anything watching, and each ran, returned,
logged nothing alarming, and enforced nothing.

## A control that cannot act must say so — silence is the bug, not the symptom

This file states in several places that "I could not look" must never read as "everything is fine."
That rule keeps being restated because the same defect keeps arriving in a new costume, and every
instance below was found by accident rather than by anything that was watching. What they share is
that the code ran, returned, logged nothing alarming, and enforced nothing.

Four found in a single session:

- **`gc -Apply` could never delete anything.** `Get-LiveBuildProcesses` counted `sccache.exe`, which
  is the long-lived shared daemon this very script starts and reports healthy in every preflight
  record. So the reclaimer refused unconditionally on every machine where sccache works. Measured:
  32.2GB of disposable target directories, zero compilers running, one "live build process" — the
  cache server. **A reclaimer that can never reclaim is the same defect as a gate that never gates**,
  and it hid behind a refusal message that reads like ordinary caution.
- **A capability probe admitted on panic.** `templated_route_uncovered_refusal` caught a panic from
  the compiler's own emission function and returned "no refusal". The thing that panicked was the
  compiler; a probe that panics is a compile that cannot produce a network. It now refuses with a
  diagnostic naming the panic.
- **A per-request budget bound nothing.** `CompileWorkerRequest.chain_depth_cap` is converted to a
  `ComposeBudget` and passed to `FomaProposer::new_with_budget_and_profile`, whose doc says the
  compile runs under it. The parameter is dropped one layer down. What made this survive is worth
  copying into your suspicion: the ENV-configured cap does bind, because `emit.rs` reads
  `ComposeBudget::from_env()` directly — so every path anyone measured worked, and only the
  programmatic one was dead.
- **A predicate registration that could never be consulted.** `compose_envelope` skips predicates on
  `Disposition::Proven` kinds, so one registered there compiled, ran, changed no test and no
  behaviour. `capability::inert_predicates` now refuses that shape at unit-test time.

Two rules follow, and the second is the one people skip:

1. **A control that cannot act must fail loudly, not return quietly.** Refuse, panic, or error —
   name what you could not do. `None`, `false`, "skipped", and an unused parameter all read as
   success to every caller and every log.
2. **Verify a mechanism by its EFFECT, never by its message.** The submodule sparse path failed on
   every worktree creation for months and looked fine, because the failed attempt left
   `core.sparseCheckout=true` behind and the "full" fallback inherited it — producing the right tree
   while announcing the wrong thing. A plausible-looking result is not evidence the fast path ran.
   Check the count deleted, the bytes on disk, the fire-count of the branch you think you took.

